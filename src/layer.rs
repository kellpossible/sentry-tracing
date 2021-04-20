use std::{
    cmp::max,
    mem::swap,
    time::{Instant, SystemTime},
};

use crate::{
    converters::{
        breadcrumb_from_event, convert_tracing_event, FieldVisitor, FieldVisitorConfig,
        FieldVisitorResult,
    },
    TracingIntegrationOptions,
};

use sentry_core::{
    add_breadcrumb, capture_event,
    protocol::{self, Transaction, Value},
    types::Uuid,
    Envelope, Hub,
};
use tracing::{metadata::LevelFilter, span, subscriber::Interest, Event, Subscriber};
use tracing_subscriber::{
    layer::{Context, Layered},
    registry::{LookupSpan, SpanRef},
    EnvFilter, Layer,
};

/// Provides a dispatching logger.
pub struct SentryLayer<S> {
    span_layer: Layered<EnvFilter, SpanLayer, S>,
    event_layer: Layered<EnvFilter, EventLayer, S>,
    breadcrumb_layer: Layered<EnvFilter, BreadcrumbLayer, S>,
}

impl<S> SentryLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    /// Create a new layer instance with the specified options
    pub fn new(options: TracingIntegrationOptions) -> Self {
        let span_layer = SpanLayer {
            #[cfg(features = "strip-ansi-escapes")]
            strip_ansi_escapes: options.strip_ansi_escapes,
            event_type_field: options.event_type_field.clone(),
        };
        let event_layer = EventLayer {
            #[cfg(features = "strip-ansi-escapes")]
            strip_ansi_escapes: options.strip_ansi_escapes,
            attach_stacktraces: options.attach_stacktraces,
            event_type_field: options.event_type_field.clone(),
        };
        let breadcrumb_layer = BreadcrumbLayer {
            #[cfg(features = "strip-ansi-escapes")]
            strip_ansi_escapes: options.strip_ansi_escapes,
            event_type_field: options.event_type_field,
        };

        SentryLayer {
            span_layer: span_layer.and_then(options.span_filter),
            event_layer: event_layer.and_then(options.event_filter),
            breadcrumb_layer: breadcrumb_layer.and_then(options.breadcrumb_filter),
        }
    }
}

impl<S> Default for SentryLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn default() -> Self {
        SentryLayer::new(TracingIntegrationOptions::default())
    }
}

fn is_layer_enabled<L, S>(layer: &L, id: &tracing::Id, ctx: Context<'_, S>) -> bool
where
    L: Layer<S>,
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    if let Some(metadata) = ctx.metadata(id) {
        layer.enabled(metadata, ctx)
    } else {
        false
    }
}

// The SentryLayer dispatches all events and spans to all the underlying layers
// (SpanLayer, EventLayer and BreadcrumbLayer) with "lowest common denominator" filtering
impl<S> Layer<S> for SentryLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn register_callsite(
        &self,
        metadata: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        let span = self.span_layer.register_callsite(metadata);
        let event = self.event_layer.register_callsite(metadata);
        let breadcrumb = self.breadcrumb_layer.register_callsite(metadata);

        if span.is_always() || event.is_always() || breadcrumb.is_always() {
            Interest::always()
        } else if span.is_never() && event.is_never() && breadcrumb.is_never() {
            Interest::never()
        } else {
            Interest::sometimes()
        }
    }

    fn enabled(&self, metadata: &tracing::Metadata<'_>, ctx: Context<'_, S>) -> bool {
        let span = self.span_layer.enabled(metadata, ctx.clone());
        let event = self.event_layer.enabled(metadata, ctx.clone());
        let breadcrumb = self.breadcrumb_layer.enabled(metadata, ctx);
        span || event || breadcrumb
    }

    fn new_span(&self, attrs: &span::Attributes<'_>, id: &tracing::Id, ctx: Context<'_, S>) {
        if is_layer_enabled(&self.span_layer, id, ctx.clone()) {
            self.span_layer.new_span(attrs, id, ctx.clone());
        }
        if is_layer_enabled(&self.event_layer, id, ctx.clone()) {
            self.event_layer.new_span(attrs, id, ctx.clone());
        }
        if is_layer_enabled(&self.breadcrumb_layer, id, ctx.clone()) {
            self.breadcrumb_layer.new_span(attrs, id, ctx);
        }
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        let span = self.span_layer.max_level_hint()?;
        let event = self.event_layer.max_level_hint()?;
        let breadcrumb = self.breadcrumb_layer.max_level_hint()?;
        Some(max(max(span, event), breadcrumb))
    }

    fn on_record(&self, span: &tracing::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        if is_layer_enabled(&self.span_layer, span, ctx.clone()) {
            self.span_layer.on_record(span, values, ctx.clone());
        }
        if is_layer_enabled(&self.event_layer, span, ctx.clone()) {
            self.event_layer.on_record(span, values, ctx.clone());
        }
        if is_layer_enabled(&self.breadcrumb_layer, span, ctx.clone()) {
            self.breadcrumb_layer.on_record(span, values, ctx);
        }
    }

    fn on_follows_from(&self, span: &tracing::Id, follows: &tracing::Id, ctx: Context<'_, S>) {
        if is_layer_enabled(&self.span_layer, span, ctx.clone()) {
            self.span_layer.on_follows_from(span, follows, ctx.clone());
        }
        if is_layer_enabled(&self.event_layer, span, ctx.clone()) {
            self.event_layer.on_follows_from(span, follows, ctx.clone());
        }
        if is_layer_enabled(&self.breadcrumb_layer, span, ctx.clone()) {
            self.breadcrumb_layer.on_follows_from(span, follows, ctx);
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if self.span_layer.enabled(event.metadata(), ctx.clone()) {
            self.span_layer.on_event(event, ctx.clone());
        }
        if self.event_layer.enabled(event.metadata(), ctx.clone()) {
            self.event_layer.on_event(event, ctx.clone());
        }
        if self.breadcrumb_layer.enabled(event.metadata(), ctx.clone()) {
            self.breadcrumb_layer.on_event(event, ctx);
        }
    }

    fn on_enter(&self, id: &tracing::Id, ctx: Context<'_, S>) {
        if is_layer_enabled(&self.span_layer, id, ctx.clone()) {
            self.span_layer.on_enter(id, ctx.clone());
        }
        if is_layer_enabled(&self.event_layer, id, ctx.clone()) {
            self.event_layer.on_enter(id, ctx.clone());
        }
        if is_layer_enabled(&self.breadcrumb_layer, id, ctx.clone()) {
            self.breadcrumb_layer.on_enter(id, ctx);
        }
    }

    fn on_exit(&self, id: &tracing::Id, ctx: Context<'_, S>) {
        if is_layer_enabled(&self.span_layer, id, ctx.clone()) {
            self.span_layer.on_exit(id, ctx.clone());
        }
        if is_layer_enabled(&self.event_layer, id, ctx.clone()) {
            self.event_layer.on_exit(id, ctx.clone());
        }
        if is_layer_enabled(&self.breadcrumb_layer, id, ctx.clone()) {
            self.breadcrumb_layer.on_exit(id, ctx);
        }
    }

    fn on_close(&self, id: tracing::Id, ctx: Context<'_, S>) {
        if is_layer_enabled(&self.span_layer, &id, ctx.clone()) {
            self.span_layer.on_close(id.clone(), ctx.clone());
        }
        if is_layer_enabled(&self.event_layer, &id, ctx.clone()) {
            self.event_layer.on_close(id.clone(), ctx.clone());
        }
        if is_layer_enabled(&self.breadcrumb_layer, &id, ctx.clone()) {
            self.breadcrumb_layer.on_close(id, ctx);
        }
    }

    fn on_id_change(&self, old: &tracing::Id, new: &tracing::Id, ctx: Context<'_, S>) {
        if is_layer_enabled(&self.span_layer, old, ctx.clone()) {
            self.span_layer.on_id_change(old, new, ctx.clone());
        }
        if is_layer_enabled(&self.event_layer, old, ctx.clone()) {
            self.event_layer.on_id_change(old, new, ctx.clone());
        }
        if is_layer_enabled(&self.breadcrumb_layer, old, ctx.clone()) {
            self.breadcrumb_layer.on_id_change(old, new, ctx);
        }
    }
}

/// The event layer sends all the spans it receives to Sentry as transactions
struct SpanLayer {
    #[cfg(features = "strip-ansi-escapes")]
    strip_ansi_escapes: bool,
    event_type_field: Option<String>,
}

impl<S> Layer<S> for SpanLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("Span not found, this is a bug");
        let mut extensions = span.extensions_mut();

        // TODO: implement sampling rate
        if extensions.get_mut::<Trace>().is_none() {
            let mut trace = Trace::new(&span);
            let mut visitor = FieldVisitor::new(
                FieldVisitorConfig {
                    #[cfg(features = "strip-ansi-escapes")]
                    strip_ansi_escapes: self.strip_ansi_escapes,
                    event_type_field: self.event_type_field.as_deref(),
                },
                &mut trace.visitor,
            );

            attrs.record(&mut visitor);
            extensions.insert(trace);
        }
    }

    /// Notifies this layer that a span with the given ID was entered.
    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("Span not found, this is a bug");
        let mut extensions = span.extensions_mut();

        if let Some(timings) = extensions.get_mut::<Trace>() {
            let now = Instant::now();
            timings.idle += (now - timings.last).as_nanos() as u64;
            timings.last = now;
        }
    }

    fn on_exit(&self, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("Span not found, this is a bug");
        let mut extensions = span.extensions_mut();

        if let Some(timings) = extensions.get_mut::<Trace>() {
            let now = Instant::now();
            timings.busy += (now - timings.last).as_nanos() as u64;
            timings.last = now;
            timings.last_sys = SystemTime::now();
        }
    }

    fn on_close(&self, id: span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(&id).expect("Span not found, this is a bug");
        let mut extensions = span.extensions_mut();

        let trace = match extensions.get_mut::<Trace>() {
            Some(trace) => trace,
            None => return,
        };

        let name: String = span.name().into();
        let span_id = trace.span_id;
        let trace_id = trace.trace_id;

        let busy = trace.busy;
        let mut idle = trace.idle;
        let first = trace.first;
        let last = trace.last;
        let last_sys = trace.last_sys;

        idle += (Instant::now() - last).as_nanos() as u64;

        let mut visitor = FieldVisitorResult::default();
        let mut spans = Vec::new();

        swap(&mut visitor, &mut trace.visitor);
        swap(&mut spans, &mut trace.spans);

        visitor
            .json_values
            .insert(String::from("busy"), Value::Number(busy.into()));
        visitor
            .json_values
            .insert(String::from("idle"), Value::Number(idle.into()));

        let mut span_data = protocol::Span {
            span_id,
            trace_id,
            op: Some(name.clone()),
            description: visitor.event_type,
            start_timestamp: first.into(),
            timestamp: Some(last_sys.into()),
            data: visitor.json_values,
            // TODO: propagate error status from child span / event ?
            // TODO: extract status from error object ?
            status: if visitor.expections.is_empty() {
                Some(String::from("ok"))
            } else {
                Some(String::from("internal_error"))
            },
            ..protocol::Span::default()
        };

        // Traverse the parents of this span to attach to the nearest one
        // that has tracing data (spans ignored by the span_filter do not)
        for parent in span.parents() {
            let mut extensions = parent.extensions_mut();
            if let Some(parent) = extensions.get_mut::<Trace>() {
                parent.spans.extend(spans);

                span_data.parent_span_id = Some(parent.span_id.to_simple_ref().to_string());
                parent.spans.push(span_data);
                return;
            }
        }

        // If no parent was found, consider this span a
        // transaction root and submit it to Sentry
        Hub::with_active(move |hub| {
            let mut envelope = Envelope::new();
            envelope.add_item(Transaction {
                event_id: trace_id,
                name: Some(name),
                start_timestamp: first.into(),
                timestamp: Some(last_sys.into()),
                spans,
                ..Transaction::default()
            });

            let client = hub.client().unwrap();
            client.send_envelope(envelope);
        });
    }
}

/// The event layer sends all the events it receives to Sentry as events
struct EventLayer {
    #[cfg(features = "strip-ansi-escapes")]
    strip_ansi_escapes: bool,
    attach_stacktraces: bool,
    event_type_field: Option<String>,
}

impl<S> Layer<S> for EventLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    /// Notifies this layer that an event has occurred.
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        capture_event(convert_tracing_event(
            event,
            ctx,
            self.attach_stacktraces,
            FieldVisitorConfig {
                #[cfg(features = "strip-ansi-escapes")]
                strip_ansi_escapes: self.strip_ansi_escapes,
                event_type_field: self.event_type_field.as_deref(),
            },
        ));
    }
}

/// The breadcrumb layer sends all the events it receives to Sentry as breadcrumbs
struct BreadcrumbLayer {
    #[cfg(features = "strip-ansi-escapes")]
    strip_ansi_escapes: bool,
    event_type_field: Option<String>,
}

impl<S> Layer<S> for BreadcrumbLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    /// Notifies this layer that an event has occurred.
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        add_breadcrumb(|| {
            breadcrumb_from_event(
                event,
                FieldVisitorConfig {
                    #[cfg(features = "strip-ansi-escapes")]
                    strip_ansi_escapes: self.strip_ansi_escapes,
                    event_type_field: self.event_type_field.as_deref(),
                },
            )
        });
    }
}

pub(crate) struct Trace {
    pub(crate) span_id: Uuid,
    pub(crate) trace_id: Uuid,

    visitor: FieldVisitorResult,
    spans: Vec<protocol::Span>,

    // From the tracing-subscriber implementation of span timings,
    // with additional SystemTime informations to reconstruct the UTC
    // times needed by Sentry
    idle: u64,
    busy: u64,
    last: Instant,
    first: SystemTime,
    last_sys: SystemTime,
}

impl Trace {
    fn new<R>(span: &SpanRef<R>) -> Self
    where
        R: for<'a> LookupSpan<'a>,
    {
        let trace_id = span
            .parent()
            .and_then(|parent| {
                let extensions = parent.extensions();
                let trace = extensions.get::<Trace>()?;
                Some(trace.trace_id.clone())
            })
            .unwrap_or_else(Uuid::new_v4);

        Trace {
            span_id: Uuid::new_v4(),
            trace_id,

            visitor: FieldVisitorResult::default(),
            spans: Vec::new(),

            idle: 0,
            busy: 0,
            last: Instant::now(),
            first: SystemTime::now(),
            last_sys: SystemTime::now(),
        }
    }
}
