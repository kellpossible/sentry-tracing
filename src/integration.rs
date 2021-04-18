use sentry_core::{ClientOptions, Integration};
use tracing::Level;
use tracing_subscriber::EnvFilter;

/// Integration that performs
#[derive(Debug)]
pub struct TracingIntegrationOptions {
    /// The sentry specific tracing span/event level filter (defaults to `info`).
    pub filter: EnvFilter,
    /// If set to `true`, breadcrumbs will be emitted. (defaults to `true`).
    pub emit_breadcrumbs: bool,
    /// If set to `true` error events will be sent for errors in the log. (defaults to `true`).
    pub emit_error_events: bool,
    /// If set to `true` warning events will be sent for warnings in the log. (defaults to `false`).
    pub emit_warning_events: bool,
    /// If set to `true` current stacktrace will be resolved and attached
    /// to each event. (expensive, defaults to `true`).
    pub attach_stacktraces: bool,
    /// If set to true, ansi escape sequences will be stripped from
    /// string values, and formatted error/debug values.
    pub strip_ansi_escapes: bool,
    /// If `Some`, values for tracing events with the field name
    /// matching what is specified here will be included in the event
    /// type string: "[target](event_type) tracing event".
    pub event_type_field: Option<String>,
}

impl Default for TracingIntegrationOptions {
    fn default() -> Self {
        Self {
            filter: EnvFilter::new("info"),
            emit_breadcrumbs: true,
            emit_error_events: true,
            emit_warning_events: false,
            attach_stacktraces: true,
            strip_ansi_escapes: false,
            event_type_field: None,
        }
    }
}

/// A Sentry [Integration] for capturing events/spans from the
/// `tracing` framework.
pub struct TracingIntegration {
    pub(crate) options: TracingIntegrationOptions,
}

impl TracingIntegration {
    /// Create a new [TracingIntegration] with the specified `options`.
    pub fn new(options: TracingIntegrationOptions) -> Self {
        Self { options }
    }

    /// Checks if an issue should be created.
    pub(crate) fn create_issue_for_event(&self, event: &tracing::Event<'_>) -> bool {
        match *event.metadata().level() {
            Level::WARN => self.options.emit_warning_events,
            Level::ERROR => self.options.emit_error_events,
            _ => false,
        }
    }
}

impl Default for TracingIntegration {
    fn default() -> Self {
        Self::new(TracingIntegrationOptions::default())
    }
}

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
