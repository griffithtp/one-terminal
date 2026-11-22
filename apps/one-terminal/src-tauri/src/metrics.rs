//! Terminal metrics: open windows and popouts, popout open/dock latency (the
//! `"open … in N ms"` / `"dock … in N ms"` log lines, as a histogram) and
//! launches of out-of-process hosts — the same `ot.app.launches` instruments
//! the Desktop Agent records.

use ot_telemetry::metrics::{Counter, Histogram, KeyValue, Meter, UpDownCounter};
use std::sync::OnceLock;
use std::time::Instant;

pub(crate) struct WmMetrics {
    terminals: UpDownCounter<i64>,
    popouts: UpDownCounter<i64>,
    popout_duration: Histogram<f64>,
    launches: Counter<u64>,
    launch_duration: Histogram<f64>,
}

/// What a popout timing measures.
#[derive(Clone, Copy)]
pub(crate) enum PopoutOp {
    Open,
    Dock,
}

impl WmMetrics {
    pub fn new(meter: &Meter) -> Self {
        Self {
            terminals: meter
                .i64_up_down_counter("ot.terminal.windows")
                .with_description("Open Terminal windows in this process")
                .build(),
            popouts: meter
                .i64_up_down_counter("ot.terminal.popouts")
                .with_description("Open popout windows in this process")
                .build(),
            popout_duration: meter
                .f64_histogram("ot.popout.duration")
                .with_unit("s")
                .with_description("Time to open a popout window or dock it back")
                .build(),
            launches: meter
                .u64_counter("ot.app.launches")
                .with_description("App launches, by launch kind and outcome")
                .build(),
            launch_duration: meter
                .f64_histogram("ot.app.launch.duration")
                .with_unit("s")
                .with_description("Time to open a window or spawn the app's host process")
                .build(),
        }
    }

    /// One out-of-process host launch of `kind` (a fixed label).
    pub fn launch(&self, kind: &'static str, ok: bool, started: Instant) {
        let outcome = if ok { "ok" } else { "error" };
        self.launches.add(
            1,
            &[
                KeyValue::new("ot.launch.kind", kind),
                KeyValue::new("outcome", outcome),
            ],
        );
        self.launch_duration.record(
            started.elapsed().as_secs_f64(),
            &[KeyValue::new("ot.launch.kind", kind)],
        );
    }

    pub fn terminal_opened(&self) {
        self.terminals.add(1, &[]);
    }

    pub fn terminal_closed(&self) {
        self.terminals.add(-1, &[]);
    }

    pub fn popout_opened(&self) {
        self.popouts.add(1, &[]);
    }

    pub fn popout_closed(&self) {
        self.popouts.add(-1, &[]);
    }

    pub fn popout_timing(&self, op: PopoutOp, started: Instant) {
        let op = match op {
            PopoutOp::Open => "open",
            PopoutOp::Dock => "dock",
        };
        self.popout_duration
            .record(started.elapsed().as_secs_f64(), &[KeyValue::new("op", op)]);
    }
}

/// The process-wide instruments, built on first use (after telemetry init).
pub(crate) fn wm() -> &'static WmMetrics {
    static METRICS: OnceLock<WmMetrics> = OnceLock::new();
    METRICS.get_or_init(|| WmMetrics::new(&ot_telemetry::metrics::meter("one-terminal")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_windows_and_times_popouts() {
        let capture = ot_telemetry::testing::Capture::new();
        let m = WmMetrics::new(&capture.meter("one-terminal"));
        m.terminal_opened();
        m.terminal_opened();
        m.terminal_closed();
        m.popout_opened();
        m.popout_timing(PopoutOp::Open, Instant::now());
        m.popout_timing(PopoutOp::Dock, Instant::now());
        assert!((capture.sum("ot.terminal.windows", &[]) - 1.0).abs() < f64::EPSILON);
        assert!((capture.sum("ot.terminal.popouts", &[]) - 1.0).abs() < f64::EPSILON);
        assert_eq!(
            capture.histogram_count("ot.popout.duration", &[("op", "open")]),
            1
        );
        assert_eq!(
            capture.histogram_count("ot.popout.duration", &[("op", "dock")]),
            1
        );
        m.launch("electron_host", false, Instant::now());
        let failed = capture.sum(
            "ot.app.launches",
            &[("ot.launch.kind", "electron_host"), ("outcome", "error")],
        );
        assert!((failed - 1.0).abs() < f64::EPSILON);
    }
}
