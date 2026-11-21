//! Metric instruments for application code (feature `otlp`).
//!
//! Get a [`Meter`] with [`meter`] and build instruments once, at startup:
//!
//! ```ignore
//! let launches = ot_telemetry::metrics::meter("desktop-agent")
//!     .u64_counter("ot.app.launches")
//!     .build();
//! launches.add(1, &[KeyValue::new("outcome", "ok")]);
//! ```
//!
//! Keep attributes low-cardinality: never app ids, URLs or user input.
//! Without an exported metrics signal the meter is a no-op and recording costs
//! almost nothing.

pub use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter};
pub use opentelemetry::KeyValue;

use opentelemetry::metrics::MeterProvider as _;
use opentelemetry::InstrumentationScope;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use std::sync::OnceLock;

static PROVIDER: OnceLock<SdkMeterProvider> = OnceLock::new();

/// Make `provider` the one [`meter`] uses (called once from `init`).
pub(crate) fn install(provider: SdkMeterProvider) {
    let _ = PROVIDER.set(provider);
}

/// A meter for `scope` (the component, e.g. `"desktop-agent"`).
pub fn meter(scope: &'static str) -> Meter {
    let scope = InstrumentationScope::builder(scope).build();
    match PROVIDER.get() {
        Some(provider) => provider.meter_with_scope(scope),
        None => opentelemetry::global::meter_provider().meter_with_scope(scope),
    }
}
