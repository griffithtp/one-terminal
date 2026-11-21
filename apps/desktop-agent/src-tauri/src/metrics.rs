//! App-launch metrics. Attributes stay low-cardinality: the launch kind and
//! the outcome, never the app id.

use ot_telemetry::metrics::{Counter, Histogram, KeyValue, Meter};
use std::sync::OnceLock;
use std::time::Instant;

pub(crate) struct LaunchMetrics {
    launches: Counter<u64>,
    duration: Histogram<f64>,
}

impl LaunchMetrics {
    pub fn new(meter: &Meter) -> Self {
        Self {
            launches: meter
                .u64_counter("ot.app.launches")
                .with_description("App launches, by launch kind and outcome")
                .build(),
            duration: meter
                .f64_histogram("ot.app.launch.duration")
                .with_unit("s")
                .with_description("Time to open a window or spawn the app's host process")
                .build(),
        }
    }

    pub fn record(&self, kind: &'static str, ok: bool, started: Instant) {
        let outcome = if ok { "ok" } else { "error" };
        self.launches.add(
            1,
            &[
                KeyValue::new("ot.launch.kind", kind),
                KeyValue::new("outcome", outcome),
            ],
        );
        self.duration.record(
            started.elapsed().as_secs_f64(),
            &[KeyValue::new("ot.launch.kind", kind)],
        );
    }
}

/// The process-wide instruments, built on first use (after telemetry init).
pub(crate) fn launches() -> &'static LaunchMetrics {
    static METRICS: OnceLock<LaunchMetrics> = OnceLock::new();
    METRICS.get_or_init(|| LaunchMetrics::new(&ot_telemetry::metrics::meter("desktop-agent")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_kind_and_outcome() {
        let capture = ot_telemetry::testing::Capture::new();
        let metrics = LaunchMetrics::new(&capture.meter("desktop-agent"));
        metrics.record("tauri_host", true, Instant::now());
        metrics.record("tauri_host", false, Instant::now());
        let ok = capture.sum(
            "ot.app.launches",
            &[("ot.launch.kind", "tauri_host"), ("outcome", "ok")],
        );
        assert!((ok - 1.0).abs() < f64::EPSILON);
        assert_eq!(
            capture.histogram_count(
                "ot.app.launch.duration",
                &[("ot.launch.kind", "tauri_host")]
            ),
            2
        );
    }
}
