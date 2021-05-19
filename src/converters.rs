use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{Debug, Display},
};

use protocol::TraceContext;
use sentry_backtrace::current_stacktrace;
use sentry_core::{
    event_from_error,
    protocol::Value,
    protocol::{self, Event, Exception, Transaction},
    types::Uuid,
    Breadcrumb,
};
use tracing::{field::Field, span::Attributes, Subscriber};
use tracing_subscriber::{
    layer::Context,
    registry::{LookupSpan, SpanRef},
};

use crate::layer::{Timings, Trace};

fn convert_tracing_level(level: &tracing::Level) -> sentry_core::Level {
    match level {
        &tracing::Level::ERROR => sentry_core::Level::Error,
        &tracing::Level::WARN => sentry_core::Level::Warning,
        &tracing::Level::INFO => sentry_core::Level::Info,
        &tracing::Level::DEBUG | &tracing::Level::TRACE => sentry_core::Level::Debug,
    }
}

/// Configures how sentry event and span data is recorded
// from tracing event and spans attributes
#[derive(Clone, Copy)]
pub struct FieldVisitorConfig<'a> {
    /// If set to true, ansi escape sequences will be stripped from
    /// string values, and formatted error/debug values.
    #[cfg(features = "strip-ansi-escapes")]
    pub strip_ansi_escapes: bool,

    /// If `Some`, values for tracing events with the field name
    /// matching what is specified here will be included as the event
    /// message string.
    pub event_type_field: Option<&'a str>,
}

#[derive(Default)]
pub(crate) struct FieldVisitorResult {
    pub(crate) event_type: Option<String>,
    pub(crate) json_values: BTreeMap<String, Value>,
    pub(crate) expections: Vec<Exception>,
}

pub(crate) struct FieldVisitor<'a> {
    config: FieldVisitorConfig<'a>,
    result: &'a mut FieldVisitorResult,
}

impl<'a> FieldVisitor<'a> {
    pub(crate) fn new(config: FieldVisitorConfig<'a>, result: &'a mut FieldVisitorResult) -> Self {
        Self { config, result }
    }

    fn record_json_value(&mut self, field: &Field, json_value: Value) {
        self.result
            .json_values
            .insert(field.name().to_owned(), json_value);
    }

    /// Try to record this field as the `event_type`, returns true if the field was
    /// inserted and false if the value was discarded
    fn try_record_event_type(&mut self, field: &Field, value: impl Display) -> bool {
        if let Some(event_type_field) = self.config.event_type_field {
            if field.name() == event_type_field {
                self.result.event_type = Some(value.to_string());
                return true;
            }
        }

        false
    }
}

/// Strips ansi color escape codes from string, or returns the
/// original string if there was problem performing the strip.
#[cfg(features = "strip-ansi-escapes")]
pub fn strip_ansi_codes_from_string(string: &str) -> String {
    if let Ok(stripped_bytes) = strip_ansi_escapes::strip(string.as_bytes()) {
        if let Ok(stripped_string) = std::str::from_utf8(&stripped_bytes) {
            return stripped_string.to_owned();
        }
    }

    string.to_owned()
}

impl<'a> tracing::field::Visit for FieldVisitor<'a> {
    /// Visit a signed 64-bit integer value.
    fn record_i64(&mut self, field: &Field, value: i64) {
        if !self.try_record_event_type(field, value) {
            self.record_json_value(field, Value::Number(value.into()));
        }
    }

    /// Visit an unsigned 64-bit integer value.
    fn record_u64(&mut self, field: &Field, value: u64) {
        if !self.try_record_event_type(field, value) {
            self.record_json_value(field, Value::Number(value.into()));
        }
    }

    /// Visit a boolean value.
    fn record_bool(&mut self, field: &Field, value: bool) {
        if !self.try_record_event_type(field, value) {
            self.record_json_value(field, Value::Bool(value));
        }
    }

    /// Visit an `&str` value.
    fn record_str(&mut self, field: &Field, value: &str) {
        #[cfg(features = "strip-ansi-escapes")]
        let value = if self.config.strip_ansi_escapes {
            strip_ansi_codes_from_string(&value)
        } else {
            value.to_owned()
        };

        if !self.try_record_event_type(field, &value) {
            self.record_json_value(field, Value::String(value.into()));
        }
    }

    /// Visit a type that implements `std::error::Error`.
    fn record_error(&mut self, _field: &Field, value: &(dyn Error + 'static)) {
        // As exception_from_error is not public, this calls event_from_error
        // instead and extract the Exception struct from the resulting Event
        let event = event_from_error(value);
        for exception in event.exception {
            self.result.expections.push(exception);
        }
    }

    /// Visit a type that implements `std::fmt::Debug`.
    #[cfg_attr(not(features = "strip-ansi-escapes"), allow(unused_mut))]
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        let mut formatted_value = format!("{:?}", value);

        #[cfg(features = "strip-ansi-escapes")]
        if self.config.strip_ansi_escapes {
            formatted_value = strip_ansi_codes_from_string(&formatted_value)
        }

        if !self.try_record_event_type(field, &formatted_value) {
            self.record_json_value(field, Value::String(formatted_value));
        }
    }
}

/// Creates a breadcrumb from a given tracing event.
pub fn breadcrumb_from_event(
    event: &tracing::Event<'_>,
    visitor_config: FieldVisitorConfig,
) -> Breadcrumb {
    let mut visitor_result = FieldVisitorResult::default();
    let mut visitor = FieldVisitor::new(visitor_config, &mut visitor_result);

    event.record(&mut visitor);

    Breadcrumb {
        ty: "log".into(),
        level: convert_tracing_level(event.metadata().level()),
        category: Some(event.metadata().target().into()),
        message: visitor_result.event_type,
        data: visitor_result.json_values,
        ..Default::default()
    }
}

pub(crate) fn default_convert_breadcrumb<S>(
    event: &tracing::Event<'_>,
    _ctx: Context<S>,
) -> Breadcrumb {
    breadcrumb_from_event(
        event,
        FieldVisitorConfig {
            event_type_field: None,
            #[cfg(features = "strip-ansi-escapes")]
            strip_ansi_escapes: true,
        },
    )
}

/// Creates an event from a given log record.
///
/// If `attach_stacktraces` is set to `true` then a stacktrace is attached
/// from the current frame.
pub fn convert_tracing_event<S>(
    event: &tracing::Event<'_>,
    ctx: Context<S>,
    attach_stacktraces: bool,
    visitor_config: FieldVisitorConfig,
) -> Event<'static>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let mut visitor_result = FieldVisitorResult::default();
    let mut visitor = FieldVisitor::new(visitor_config, &mut visitor_result);
    event.record(&mut visitor);

    let exception = if !visitor_result.expections.is_empty() {
        visitor_result.expections
    } else {
        vec![Exception {
            ty: event.metadata().name().into(),
            value: visitor_result.event_type.clone(),
            stacktrace: if attach_stacktraces {
                current_stacktrace()
            } else {
                None
            },
            module: event.metadata().module_path().map(String::from),
            ..Default::default()
        }]
    };

    let mut result = Event {
        logger: Some("sentry-tracing".into()),
        level: convert_tracing_level(event.metadata().level()),
        message: visitor_result.event_type,
        exception: exception.into(),
        extra: visitor_result.json_values,
        ..Default::default()
    };

    let parent = event
        .parent()
        .and_then(|id| ctx.span(id))
        .or_else(|| ctx.lookup_current());

    if let Some(parent) = parent {
        let extensions = parent.extensions();
        if let Some(trace) = extensions.get::<Trace>() {
            let context = protocol::Context::from(TraceContext {
                span_id: trace.span.span_id,
                trace_id: trace.span.trace_id,
                ..TraceContext::default()
            });

            result.contexts.insert(context.type_name().into(), context);
            result.transaction = parent.parents().last().map(|root| root.name().into());
        }
    }

    result
}

pub(crate) fn default_convert_event<S>(
    event: &tracing::Event<'_>,
    ctx: Context<S>,
) -> Event<'static>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    convert_tracing_event(
        event,
        ctx,
        true,
        FieldVisitorConfig {
            event_type_field: None,
            #[cfg(features = "strip-ansi-escapes")]
            strip_ansi_escapes: true,
        },
    )
}

pub(crate) fn default_new_span<S>(
    span: &SpanRef<S>,
    parent: Option<&protocol::Span>,
    attrs: &Attributes,
) -> protocol::Span
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let trace_id = parent
        .map(|parent| parent.trace_id.clone())
        .unwrap_or_else(Uuid::new_v4);

    let mut result = FieldVisitorResult::default();

    let mut visitor = FieldVisitor::new(
        FieldVisitorConfig {
            #[cfg(features = "strip-ansi-escapes")]
            strip_ansi_escapes: true,
            event_type_field: None,
        },
        &mut result,
    );

    attrs.record(&mut visitor);

    protocol::Span {
        span_id: Uuid::new_v4(),
        trace_id,
        op: Some(span.name().into()),
        description: result.event_type,
        data: result.json_values,
        status: if result.expections.is_empty() {
            Some(String::from("ok"))
        } else {
            Some(String::from("internal_error"))
        },
        ..protocol::Span::default()
    }
}

pub(crate) fn default_on_close(span: &mut protocol::Span, timings: Timings) {
    span.data
        .insert(String::from("busy"), Value::Number(timings.busy.into()));

    span.data
        .insert(String::from("idle"), Value::Number(timings.idle.into()));

    span.timestamp = Some(timings.end_time.into());
}

pub(crate) fn default_convert_transaction<S>(
    trace_id: Uuid,
    span: &SpanRef<S>,
    spans: Vec<protocol::Span>,
    timings: Timings,
) -> Transaction<'static>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    Transaction {
        event_id: trace_id,
        name: Some(span.name().into()),
        start_timestamp: timings.start_time.into(),
        timestamp: Some(timings.end_time.into()),
        spans,
        ..Transaction::default()
    }
}
