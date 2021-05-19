use sentry_core::{ClientOptions, Integration};
use tracing::Subscriber;
use tracing_subscriber::{registry::LookupSpan, EnvFilter};

use crate::{
    converters::{
        default_convert_breadcrumb, default_convert_event, default_convert_transaction,
        default_new_span, default_on_close,
    },
    layer::{ConvertBreadcrumb, ConvertEvent, ConvertTransaction, NewSpan, OnClose},
};

/// Integration that performs
pub struct TracingIntegrationOptions<S> {
    /// Events matching this filter will be sent to sentry as events
    pub event_filter: EnvFilter,
    /// Events matching this filter will be sent to sentry as breadcrumb
    pub breadcrumb_filter: EnvFilter,
    /// Spans matching this filter will be sent to sentry as transactions
    pub span_filter: EnvFilter,
    /// Defines how a tracing event should be converted into a sentry event
    pub convert_event: ConvertEvent<S>,
    /// Defines how a tracing event should be converted into a sentry breadcrumb
    pub convert_breadcrumb: ConvertBreadcrumb<S>,
    /// Defines how a tracing span should be converted into a sentry span
    pub new_span: NewSpan<S>,
    /// Allows inserting additional data into a span as it finishes (such as timings)
    pub on_close: OnClose,
    /// Defines how a set of spans should be converted into a sentry transaction
    pub convert_transaction: ConvertTransaction<S>,
}

impl<S> Default for TracingIntegrationOptions<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn default() -> Self {
        Self {
            event_filter: EnvFilter::new("error"),
            breadcrumb_filter: EnvFilter::new("info"),
            span_filter: EnvFilter::default(),
            convert_event: Box::new(default_convert_event),
            convert_breadcrumb: Box::new(default_convert_breadcrumb),
            new_span: Box::new(default_new_span),
            on_close: Box::new(default_on_close),
            convert_transaction: Box::new(default_convert_transaction),
        }
    }
}

/// A Sentry [Integration] for capturing events/spans from the
/// `tracing` framework.
#[derive(Default)]
pub struct TracingIntegration;

impl Integration for TracingIntegration {
    fn name(&self) -> &'static str {
        "tracing"
    }

    fn setup(&self, cfg: &mut ClientOptions) {
        cfg.in_app_exclude.push("tracing_core::");
        cfg.in_app_exclude.push("tracing_log::");
        cfg.in_app_exclude.push("log::");
        cfg.extra_border_frames
            .push("tracing_core::event::Event::dispatch");
        cfg.extra_border_frames.push("log::__private_api_log");
    }
}
