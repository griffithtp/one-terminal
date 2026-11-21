//! In-memory capture of spans and metrics for tests in application crates
//! (feature `testing`). Keeps OpenTelemetry SDK types out of those crates.
//!
//! ```ignore
//! let capture = ot_telemetry::testing::Capture::new();
//! let metrics = MyMetrics::new(&capture.meter("desktop-agent"));
//! capture.run(|| do_work(&metrics));
//! assert_eq!(capture.spans()[0].name, "fdc3.request");
//! assert_eq!(capture.sum("ot.fdc3.messages", &[("fdc3.message.type", "broadcastRequest")]), 1.0);
//! ```

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{metrics::MeterProvider as _, InstrumentationScope, KeyValue};
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData, ResourceMetrics};
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
use std::collections::BTreeMap;
use tracing_subscriber::layer::SubscriberExt;

/// A finished span, flattened for assertions.
#[derive(Clone, Debug)]
pub struct CapturedSpan {
    pub name: String,
    pub trace_id: String,
    pub span_id: String,
    /// `0000000000000000` for a root span.
    pub parent_span_id: String,
    pub parent_is_remote: bool,
    pub attributes: BTreeMap<String, String>,
}

/// A tracer and meter whose output stays in memory.
pub struct Capture {
    spans: InMemorySpanExporter,
    tracer: SdkTracerProvider,
    metrics: InMemoryMetricExporter,
    meter_provider: SdkMeterProvider,
    dispatch: tracing::Dispatch,
}

impl Default for Capture {
    fn default() -> Self {
        Self::new()
    }
}

impl Capture {
    pub fn new() -> Self {
        let spans = InMemorySpanExporter::default();
        let tracer = SdkTracerProvider::builder()
            .with_simple_exporter(spans.clone())
            .build();
        let metrics = InMemoryMetricExporter::default();
        let meter_provider = SdkMeterProvider::builder()
            .with_reader(PeriodicReader::builder(metrics.clone()).build())
            .build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(tracer.tracer("test")));
        Self {
            spans,
            tracer,
            metrics,
            meter_provider,
            dispatch: tracing::Dispatch::new(subscriber),
        }
    }

    /// A meter recording into this capture.
    pub fn meter(&self, scope: &'static str) -> crate::metrics::Meter {
        self.meter_provider
            .meter_with_scope(InstrumentationScope::builder(scope).build())
    }

    /// Run `f` with this capture's subscriber as the thread's default.
    pub fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        tracing::dispatcher::with_default(&self.dispatch, f)
    }

    /// The subscriber, for async tests (`future.with_subscriber(dispatch)`).
    pub fn dispatch(&self) -> tracing::Dispatch {
        self.dispatch.clone()
    }

    /// Spans finished so far.
    pub fn spans(&self) -> Vec<CapturedSpan> {
        let _ = self.tracer.force_flush();
        self.spans
            .get_finished_spans()
            .unwrap_or_default()
            .into_iter()
            .map(|s| CapturedSpan {
                name: s.name.to_string(),
                trace_id: s.span_context.trace_id().to_string(),
                span_id: s.span_context.span_id().to_string(),
                parent_span_id: s.parent_span_id.to_string(),
                parent_is_remote: s.parent_span_is_remote,
                attributes: s
                    .attributes
                    .iter()
                    .map(|kv| (kv.key.to_string(), kv.value.to_string()))
                    .collect(),
            })
            .collect()
    }

    /// Total of a counter / up-down counter's data points whose attributes
    /// include every pair in `attrs` (latest collection).
    pub fn sum(&self, name: &str, attrs: &[(&str, &str)]) -> f64 {
        self.with_metric(name, |data| match data {
            AggregatedMetrics::U64(MetricData::Sum(s)) => s
                .data_points()
                .filter(|p| matches(p.attributes(), attrs))
                .map(|p| p.value() as f64)
                .sum(),
            AggregatedMetrics::I64(MetricData::Sum(s)) => s
                .data_points()
                .filter(|p| matches(p.attributes(), attrs))
                .map(|p| p.value() as f64)
                .sum(),
            AggregatedMetrics::F64(MetricData::Sum(s)) => s
                .data_points()
                .filter(|p| matches(p.attributes(), attrs))
                .map(|p| p.value())
                .sum(),
            _ => 0.0,
        })
    }

    /// Number of values recorded by a histogram, filtered as in [`Self::sum`].
    pub fn histogram_count(&self, name: &str, attrs: &[(&str, &str)]) -> u64 {
        self.with_metric(name, |data| match data {
            AggregatedMetrics::F64(MetricData::Histogram(h)) => h
                .data_points()
                .filter(|p| matches(p.attributes(), attrs))
                .map(|p| p.count())
                .sum(),
            AggregatedMetrics::U64(MetricData::Histogram(h)) => h
                .data_points()
                .filter(|p| matches(p.attributes(), attrs))
                .map(|p| p.count())
                .sum(),
            _ => 0,
        })
    }

    fn with_metric<T: Default>(&self, name: &str, f: impl Fn(&AggregatedMetrics) -> T) -> T {
        let _ = self.meter_provider.force_flush();
        let finished: Vec<ResourceMetrics> =
            self.metrics.get_finished_metrics().unwrap_or_default();
        let value = finished
            .iter()
            .rev()
            .flat_map(|rm| rm.scope_metrics())
            .flat_map(|sm| sm.metrics())
            .find(|m| m.name() == name)
            .map(|m| f(m.data()))
            .unwrap_or_default();
        value
    }
}

fn matches<'a>(attributes: impl Iterator<Item = &'a KeyValue>, wanted: &[(&str, &str)]) -> bool {
    let have: Vec<(String, String)> = attributes
        .map(|kv| (kv.key.to_string(), kv.value.to_string()))
        .collect();
    wanted
        .iter()
        .all(|(k, v)| have.iter().any(|(hk, hv)| hk == k && hv == v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_spans_and_metrics() {
        let capture = Capture::new();
        let counter = capture.meter("t").u64_counter("t.count").build();
        capture.run(|| {
            let _s = tracing::info_span!("work", answer = 42).entered();
            counter.add(2, &[KeyValue::new("kind", "a")]);
        });
        let spans = capture.spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "work");
        assert_eq!(spans[0].attributes["answer"], "42");
        assert!((capture.sum("t.count", &[("kind", "a")]) - 2.0).abs() < f64::EPSILON);
        assert!(capture.sum("t.count", &[("kind", "b")]).abs() < f64::EPSILON);
    }
}
