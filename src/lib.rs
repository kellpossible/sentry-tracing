//! Adds support for automatic Breadcrumb and Event capturing from logs.
//!
//! The `log` crate is supported in two ways. First, logs can be captured as
//! breadcrumbs for later. Secondly, error logs can be captured as events to
//! Sentry. By default anything above `Info` is recorded as breadcrumb and
//! anything above `Error` is captured as error event.
//!
//! # Examples
//!
//! ```
//! let tracing_integration = sentry_tracing::TracingIntegration::default();
//! let _sentry = sentry::init(sentry::ClientOptions::default().add_integration(tracing_integration));
//!
//! tracing::info!("Generates a breadcrumb");
//! ```
//!

#![doc(html_favicon_url = "https://sentry-brand.storage.googleapis.com/favicon.ico")]
#![doc(html_logo_url = "https://sentry-brand.storage.googleapis.com/sentry-glyph-black.png")]
#![warn(missing_docs)]

mod converters;
mod integration;
mod layer;

pub use converters::{breadcrumb_from_event, convert_tracing_event};
pub use integration::{TracingIntegration, TracingIntegrationOptions};
pub use layer::SentryLayer;
