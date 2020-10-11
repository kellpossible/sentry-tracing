use sentry_backtrace::current_stacktrace;
use sentry_core::protocol::{Event, Exception};
use sentry_core::Breadcrumb;

fn convert_tracing_level(level: &tracing::Level) -> sentry_core::Level {
    match level {
        &tracing::Level::ERROR => sentry_core::Level::Error,
        &tracing::Level::WARN => sentry_core::Level::Warning,
        &tracing::Level::INFO => sentry_core::Level::Info,
        &tracing::Level::DEBUG | &tracing::Level::TRACE => sentry_core::Level::Debug,
    }
}

#[derive(Default)]    
struct FieldVisitor {
    messages: Vec<String>,
    log_target: Option<String>,
}

impl FieldVisitor {
    fn visit_event(event: &tracing::Event<'_>) -> Self {
        let mut visitor = Self::default();
        event.record(&mut visitor);
        visitor
    }
}

impl tracing::field::Visit for FieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "log.target" {
            self.log_target = Some(value.to_owned());
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.messages.push(format!("{}={:?}", field, value));
    }
}

fn format_event<S: tracing::Subscriber>(event: &tracing::Event<'_>, context: &tracing_subscriber::layer::Context<'_, S>) -> String {
    let visitor = FieldVisitor::visit_event(event);
    visitor.messages.join("\n")
}

/// Creates a breadcrumb from a given tracing event.
pub fn breadcrumb_from_event<S: tracing::Subscriber>(event: &tracing::Event<'_>, context: &tracing_subscriber::layer::Context<'_, S>) -> Breadcrumb {
    let event_message = format_event(event, context);

    Breadcrumb {
        ty: "log".into(),
        level: convert_tracing_level(event.metadata().level()),
        category: Some(event.metadata().target().into()),
        message: Some(event_message),
        ..Default::default()
    }
}

/// Creates an event from a given log record.
///
/// If `with_stacktrace` is set to `true` then a stacktrace is attached
/// from the current frame.
pub fn convert_tracing_event<S: tracing::Subscriber>(event: &tracing::Event<'_>, context: &tracing_subscriber::layer::Context<'_, S>, with_stacktrace: bool) -> Event<'static> {
    let visitor = FieldVisitor::visit_event(event);

    // Special support for log.target reported by tracing-log
    let exception_type = visitor.log_target.unwrap_or_else(|| {
        event.metadata().target().to_owned()
    });

    Event {
        logger: Some("sentry-tracing".into()),
        level: convert_tracing_level(event.metadata().level()),
        exception: vec![Exception {
            ty: exception_type,
            value: Some(format_event(event, context)),
            stacktrace: if with_stacktrace {
                current_stacktrace()
            } else {
                None
            },
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    }
}
