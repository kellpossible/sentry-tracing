use crate::{breadcrumb_from_event, converters::convert_tracing_event, TracingIntegration};

use sentry_core::Hub;
use tracing::{span, Event, Subscriber};
use tracing_subscriber::{layer::Context, Layer};

/// Provides a dispatching logger.
#[derive(Default)]
pub struct SentryLayer;

impl<S: Subscriber> Layer<S> for SentryLayer {
    /// Notifies this layer that a span with the given ID was entered.
    fn on_enter(&self, _id: &span::Id, _ctx: Context<'_, S>) {}

    /// Notifies this layer that an event has occurred.
    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        let recorded =
            sentry_core::with_integration(|integration: &TracingIntegration, hub: &Hub| {
                if integration.create_issue_for_event(event) {
                    hub.capture_event(convert_tracing_event(event, &integration.options));
                }

                if integration.options.emit_breadcrumbs
                    && integration.options.filter.enabled(event.metadata(), context)
                {
                    sentry_core::add_breadcrumb(|| breadcrumb_from_event(event, &integration.options));
                }

                true
            });

        if !recorded {
            eprintln!("Tracing event was not recorded by sentry because it has no `TracingIntegration` applied.")
        }
    }
}
