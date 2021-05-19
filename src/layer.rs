use std::{
    cmp::max,
    time::{Instant, SystemTime},
};

use crate::TracingIntegrationOptions;

use sentry_core::{
    add_breadcrumb, capture_event,
    protocol::{self, Breadcrumb, Transaction},
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
    span_layer: Layered<EnvFilter, SpanLayer<S>, S>,
    event_layer: Layered<EnvFilter, EventLayer<S>, S>,
    breadcrumb_layer: Layered<EnvFilter, BreadcrumbLayer<S>, S>,
}

impl<S> SentryLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    /// Create a new layer instance with the specified options
    pub fn new(options: TracingIntegrationOptions<S>) -> Self {
        let span_layer = SpanLayer {
            new_span: options.new_span,
            on_close: options.on_close,
            convert_transaction: options.convert_transaction,
        };
        let event_layer = EventLayer {
            convert_event: options.convert_event,
        };
        let breadcrumb_layer = BreadcrumbLayer {
            convert_breadcrumb: options.convert_breadcrumb,
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

pub type NewSpan<S> = Box<
    dyn Fn(&SpanRef<S>, Option<&protocol::Span>, &span::Attributes) -> protocol::Span + Send + Sync,
>;

pub type OnClose = Box<dyn Fn(&mut protocol::Span, Timings) + Send + Sync>;

pub type ConvertTransaction<S> = Box<
    dyn Fn(Uuid, &SpanRef<S>, Vec<protocol::Span>, Timings) -> Transaction<'static> + Send + Sync,
>;

/// The event layer sends all the spans it receives to Sentry as transactions
struct SpanLayer<S> {
    new_span: NewSpan<S>,
    on_close: OnClose,
    convert_transaction: ConvertTransaction<S>,
}

impl<S> Layer<S> for SpanLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("Span not found, this is a bug");
        let mut extensions = span.extensions_mut();

        // TODO: implement sampling rate
        if extensions.get_mut::<Trace>().is_none() {
            for parent in span.parents() {
                let parent = parent.extensions();
                let parent = match parent.get::<Trace>() {
                    Some(trace) => trace,
                    None => continue,
                };

                let span = (self.new_span)(&span, Some(&parent.span), attrs);
                extensions.insert(Trace::new(span));
                return;
            }

            let span = (self.new_span)(&span, None, attrs);
            extensions.insert(Trace::new(span));
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

        let mut trace = match extensions.remove::<Trace>() {
            Some(trace) => trace,
            None => return,
        };

        trace.idle += (Instant::now() - trace.last).as_nanos() as u64;

        let timings = Timings {
            start_time: trace.first,
            end_time: trace.last_sys,
            idle: trace.idle,
            busy: trace.busy,
        };

        (self.on_close)(&mut trace.span, timings);

        // Traverse the parents of this span to attach to the nearest one
        // that has tracing data (spans ignored by the span_filter do not)
        for parent in span.parents() {
            let mut extensions = parent.extensions_mut();
            if let Some(parent) = extensions.get_mut::<Trace>() {
                parent.spans.extend(trace.spans);

                let span_id = parent.span.span_id.to_simple_ref().to_string();
                trace.span.parent_span_id = Some(span_id[..16].into());
                parent.spans.push(trace.span);
                return;
            }
        }

        // If no parent was found, consider this span a
        // transaction root and submit it to Sentry
        let span = &span;
        Hub::with_active(move |hub| {
            let transaction =
                (self.convert_transaction)(trace.span.trace_id, span, trace.spans, timings);
            let envelope = Envelope::from(transaction);
            hub.client().unwrap().send_envelope(envelope);
        });
    }
}

pub type ConvertEvent<S> =
    Box<dyn for<'a> Fn(&Event<'a>, Context<'a, S>) -> protocol::Event<'static> + Send + Sync>;

/// The event layer sends all the events it receives to Sentry as events
struct EventLayer<S> {
    convert_event: ConvertEvent<S>,
}

impl<S> Layer<S> for EventLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    /// Notifies this layer that an event has occurred.
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        capture_event((self.convert_event)(event, ctx));
    }
}

pub type ConvertBreadcrumb<S> =
    Box<dyn for<'a> Fn(&Event<'a>, Context<'a, S>) -> Breadcrumb + Send + Sync>;

/// The breadcrumb layer sends all the events it receives to Sentry as breadcrumbs
struct BreadcrumbLayer<S> {
    convert_breadcrumb: ConvertBreadcrumb<S>,
}

impl<S> Layer<S> for BreadcrumbLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    /// Notifies this layer that an event has occurred.
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        add_breadcrumb(|| (self.convert_breadcrumb)(event, ctx));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Timings {
    pub start_time: SystemTime,
    pub end_time: SystemTime,
    pub busy: u64,
    pub idle: u64,
}

pub(crate) struct Trace {
    pub(crate) span: protocol::Span,
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
    fn new(span: protocol::Span) -> Self {
        Trace {
            span,
            spans: Vec::new(),

            idle: 0,
            busy: 0,
            last: Instant::now(),
            first: SystemTime::now(),
            last_sys: SystemTime::now(),
        }
    }
}
