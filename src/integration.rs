use sentry_core::{ClientOptions, Integration};
use tracing_subscriber::EnvFilter;

/// Integration that performs
#[derive(Debug)]
pub struct TracingIntegrationOptions {
    /// Events matching this filter will be sent to sentry as events
    pub event_filter: EnvFilter,
    /// Events matching this filter will be sent to sentry as breadcrumb
    pub breadcrumb_filter: EnvFilter,
    /// Spans matching this filter will be sent to sentry as transactions
    pub span_filter: EnvFilter,
    /// If set to `true` current stacktrace will be resolved and attached
    /// to each event. (expensive, defaults to `true`).
    pub attach_stacktraces: bool,
    /// If set to true, ansi escape sequences will be stripped from
    /// string values, and formatted error/debug values.
    #[cfg(features = "strip-ansi-escapes")]
    pub strip_ansi_escapes: bool,
    /// If `Some`, values for tracing events with the field name
    /// matching what is specified here will be included in the event
    /// type string: "[target](event_type) tracing event".
    pub event_type_field: Option<String>,
}

impl Default for TracingIntegrationOptions {
    fn default() -> Self {
        Self {
            event_filter: EnvFilter::new("error"),
            breadcrumb_filter: EnvFilter::new("info"),
            span_filter: EnvFilter::default(),
            attach_stacktraces: true,
            #[cfg(features = "strip-ansi-escapes")]
            strip_ansi_escapes: true,
            event_type_field: None,
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
