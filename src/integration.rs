use tracing::Level;
use tracing_subscriber::EnvFilter;
use sentry_core::{ClientOptions, Integration};

/// Logger specific options.
#[derive(Debug)]
pub struct TracingIntegration {
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
        cfg.extra_border_frames
            .push("log::__private_api_log");

        // let filter = self.effective_global_filter();
        // if filter > log::max_level() {
        //     log::set_max_level(filter);
        // }

        // INIT.call_once(|| {
        //     log::set_boxed_logger(Box::new(SentryLayer::default())).ok();
        // });
    }
}

impl Default for TracingIntegration {
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

impl TracingIntegration {
    /// Initializes an env logger as destination target.
    #[cfg(feature = "env_logger")]
    pub fn with_env_logger_dest(mut self, logger: Option<env_logger::Logger>) -> Self {
        let logger = logger
            .unwrap_or_else(|| env_logger::Builder::from_env(env_logger::Env::default()).build());
        let filter = logger.filter();
        if self.global_filter.is_none() {
            self.global_filter = Some(filter);
        }
        self.dest_log = Some(Box::new(logger));
        self
    }

    /// Returns the level for which issues should be created.
    ///
    /// This is controlled by `emit_error_events` and `emit_warning_events`.
    // #[inline(always)]
    // fn issue_filter(&self) -> EnvFilter {
    //     if self.emit_warning_events {
    //         LevelFilter::Warn
    //     } else if self.emit_error_events {
    //         LevelFilter::Error
    //     } else {
    //         LevelFilter::Off
    //     }
    // }

    /// Checks if an issue should be created.
    pub(crate) fn create_issue_for_event(&self, event: &tracing::Event<'_>) -> bool {
        match event.metadata().level() {
            &Level::WARN => self.emit_warning_events,
            &Level::ERROR => self.emit_error_events,
            _ => false,
        }
    }
}