use crate::{TracingIntegration, converters::convert_tracing_event};

use sentry_core::Hub;
use tracing::{Event, Subscriber, span};
use tracing_subscriber::{Layer, layer::Context};

/// Provides a dispatching logger.
#[derive(Default)]
pub struct SentryLayer;

impl <S: Subscriber> Layer<S> for SentryLayer {
    /// Notifies this layer that a span with the given ID was entered.
    fn on_enter(&self, _id: &span::Id, _ctx: Context<'_, S>) {}

    /// Notifies this layer that an event has occurred.
    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        let recorded = sentry_core::with_integration(|integration: &TracingIntegration, hub: &Hub| {
            if integration.create_issue_for_event(event) {
                hub.capture_event(convert_tracing_event(event, &context, integration));
            }
            print!("sentry captured event");
            true
        });

        if !recorded {
            eprintln!("Tracing event was not recorded by sentry because it has no `TracingIntegration` applied.")
        }
    }
    // fn enabled(&self, md: &log::Metadata<'_>) -> bool {
    //     sentry_core::with_integration(|integration: &TracingIntegration, _| {
    //         if let Some(global_filter) = integration.global_filter {
    //             if md.level() > global_filter {
    //                 return false;
    //             }
    //         }
    //         md.level() <= integration.filter
    //             || integration
    //                 .dest_log
    //                 .as_ref()
    //                 .map_or(false, |x| x.enabled(md))
    //     })
    // }

    // fn log(&self, record: &log::Record<'_>) {
    //     sentry_core::with_integration(|integration: &TracingIntegration, hub| {
    //         if integration.create_issue_for_record(record) {
    //             hub.capture_event(event_from_record(record, integration.attach_stacktraces));
    //         }
    //         if integration.emit_breadcrumbs && record.level() <= integration.filter {
    //             sentry_core::add_breadcrumb(|| breadcrumb_from_record(record));
    //         }
    //         if let Some(ref log) = integration.dest_log {
    //             if log.enabled(record.metadata()) {
    //                 log.log(record);
    //             }
    //         }
    //     })
    // }

    // fn flush(&self) {
    //     sentry_core::with_integration(|integration: &TracingIntegration, _| {
    //         if let Some(ref log) = integration.dest_log {
    //             log.flush();
    //         }
    //     })
    // }
}
