//! Logging, tracing and opt-in OpenTelemetry export for OneTerminal
//! processes. No Tauri and no FDC3 dependency: application code logs and
//! creates spans with the `tracing` macros; each binary calls [`init`] once at
//! startup and [`Telemetry::shutdown`] on exit.
//!
//! - Local logs are always on: JSON lines in a daily file per service (see
//!   [`paths::default_log_dir`]) and readable text on stderr.
//! - Export (feature `otlp`) is opt-in: traces, metrics and logs go to an
//!   OTLP/HTTP collector only when [`config::resolve`] finds an endpoint.
//! - [`propagation`] carries trace context to child processes via
//!   `TRACEPARENT`.

pub mod config;
mod logs;
#[cfg(feature = "otlp")]
pub mod metrics;
#[cfg(feature = "otlp")]
mod otlp;
pub mod paths;
pub mod propagation;
pub mod redact;
#[cfg(feature = "testing")]
pub mod testing;
#[cfg(feature = "otlp")]
pub mod trace;

pub use config::{resolve, Resolved, TelemetryConfig};

use std::sync::OnceLock;
use std::time::Duration;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, Registry};

/// Longest a shutdown or panic flush waits for the collector. Quitting must
/// never hang on an unreachable endpoint; whatever isn't sent by then is
/// dropped.
pub const FLUSH_CAP: Duration = Duration::from_secs(2);

/// The process's telemetry. Keep it alive for the life of the process (its
/// guard flushes the log file) and call [`Telemetry::shutdown`] on exit;
/// `Drop` does the same.
pub struct Telemetry {
    #[cfg(feature = "otlp")]
    providers: Option<otlp::Providers>,
    exporting: bool,
    file_guard: Option<WorkerGuard>,
}

/// Install the global `tracing` subscriber for this process: env filter, log
/// file, stderr and — with `otlp` — span and log export. Logs the effective
/// telemetry settings and any warnings from `resolved` once it's up.
///
/// `version` is the binary's version (`env!("CARGO_PKG_VERSION")`), recorded
/// as `service.version`.
pub fn init(resolved: &Resolved, version: &str) -> Telemetry {
    let mut problems = Vec::new();
    // `Targets` (a `target=level` list, e.g. `info,tao=warn`) rather than
    // `EnvFilter`: no regex engine in every binary. Span/field directives aren't
    // supported.
    let filter = resolved.log_filter.parse::<Targets>().unwrap_or_else(|e| {
        problems.push(format!(
            "invalid log filter {:?} ({e}); using {:?}",
            resolved.log_filter,
            config::DEFAULT_LOG_FILTER
        ));
        config::DEFAULT_LOG_FILTER
            .parse()
            .expect("the default log filter parses")
    });
    let filter = complete_filter(filter);

    let mut layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> = vec![logs::stderr_layer()];
    let mut file_guard = None;
    if let Some(dir) = &resolved.log_dir {
        match logs::file_layer(dir, &resolved.service_name, resolved.log_retention_days) {
            Ok((layer, guard)) => {
                layers.push(layer);
                file_guard = Some(guard);
            }
            Err(e) => problems.push(format!("{e}; logging to stderr only")),
        }
    }

    #[cfg(feature = "otlp")]
    let (providers, exporting) = {
        let providers = match &resolved.otlp {
            Some(otlp) => {
                let (providers, errors) = otlp::Providers::exporting(resolved, otlp, version);
                problems.extend(errors);
                providers
            }
            None => otlp::Providers::local_only(resolved, version),
        };
        layers.extend(providers.layers());
        if let Some(meter) = &providers.meter {
            metrics::install(meter.clone());
        }
        let exporting = providers.exports(resolved.otlp.as_ref());
        (Some(providers), exporting)
    };
    #[cfg(not(feature = "otlp"))]
    let exporting = {
        let _ = version;
        if resolved.otlp.is_some() {
            problems.push("OTLP endpoint configured, but this binary is built without export (feature `otlp`); writing local logs only".into());
        }
        false
    };

    propagation::capture_child_settings(resolved, &|key| std::env::var(key).ok());

    if let Err(e) = tracing_subscriber::registry()
        .with(layers)
        .with(filter)
        .try_init()
    {
        // Another subscriber is already installed (tests, or a second init);
        // keep it rather than failing the process.
        eprintln!("[telemetry] subscriber not installed: {e}");
    }

    for (key, value, source) in &resolved.summary {
        tracing::info!(target: TELEMETRY_TARGET, "config {key} = {value} ({source})");
    }
    for warning in resolved.warnings.iter().chain(&problems) {
        tracing::warn!(target: TELEMETRY_TARGET, "{warning}");
    }

    Telemetry {
        #[cfg(feature = "otlp")]
        providers,
        exporting,
        file_guard,
    }
}

impl Telemetry {
    /// True when any signal is exported (an endpoint was configured and its
    /// exporter built).
    pub fn exporting(&self) -> bool {
        self.exporting
    }

    /// A `process.startup` span. When this process was spawned with
    /// `TRACEPARENT`, the span is a child of the spawning span, so the launch
    /// appears as one trace across processes. Later spans are new roots.
    pub fn startup_span(&self) -> tracing::Span {
        let span =
            tracing::info_span!(target: SPAN_TARGET, "process.startup", pid = std::process::id());
        #[cfg(feature = "otlp")]
        if let Some(parent) = propagation::startup_parent() {
            use tracing_opentelemetry::OpenTelemetrySpanExt;
            let _ = span.set_parent(propagation::remote_context(parent));
        }
        span
    }

    /// Log any panic at `error` and flush exporters (capped at
    /// [`FLUSH_CAP`]) before the previous hook runs.
    pub fn install_panic_hook(&self) {
        #[cfg(feature = "otlp")]
        let providers = self.providers.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            let location = info
                .location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_default();
            tracing::error!(target: "panic", location = %location, "panic: {payload}");
            // Print the panic first; the flush may take up to FLUSH_CAP.
            previous(info);
            #[cfg(feature = "otlp")]
            if let Some(p) = &providers {
                run_capped(p.flush_jobs());
            }
        }));
    }

    /// Flush and stop exporters (waiting at most [`FLUSH_CAP`]), then flush
    /// the log file. Idempotent.
    pub fn shutdown(&mut self) {
        #[cfg(feature = "otlp")]
        if let Some(p) = self.providers.take() {
            if !run_capped(p.shutdown_jobs()) {
                tracing::warn!(
                    target: TELEMETRY_TARGET,
                    "export flush took longer than {}s; unsent telemetry dropped",
                    FLUSH_CAP.as_secs()
                );
            }
        }
        // Dropping the guard flushes buffered lines to the file.
        self.file_guard.take();
    }
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Run each job on its own helper thread and wait for all of them, at most
/// [`FLUSH_CAP`] in total. Returns whether every job finished in time; late
/// threads are left to finish (or die with the process).
#[cfg(feature = "otlp")]
fn run_capped(jobs: Vec<otlp::Job>) -> bool {
    let deadline = std::time::Instant::now() + FLUSH_CAP;
    let (tx, rx) = std::sync::mpsc::channel();
    let mut pending = 0;
    for job in jobs {
        let tx = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("ot-telemetry-flush".into())
            .spawn(move || {
                job();
                let _ = tx.send(());
            });
        if spawned.is_ok() {
            pending += 1;
        }
    }
    while pending > 0 {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if rx.recv_timeout(left).is_err() {
            return false;
        }
        pending -= 1;
    }
    true
}

/// Target of telemetry's own log lines.
const TELEMETRY_TARGET: &str = "telemetry";

/// Target for spans that carry traces (`fdc3.request`, `app.launch`,
/// `process.startup`, …): `info_span!(target: ot_telemetry::SPAN_TARGET, …)`.
/// The log filter always enables it (unless it names it), so a quiet
/// `OT_LOG=warn` keeps logs quiet without turning tracing off.
pub const SPAN_TARGET: &str = "ot_spans";

/// `Targets` has no implicit default: `OT_LOG=desktop_agent_lib=debug` alone
/// would silence every other warning and error. Add a `warn` default when none
/// is given, and keep telemetry's own targets — the effective-config lines
/// and [`SPAN_TARGET`] — at `info` unless the filter names them.
fn complete_filter(filter: Targets) -> Targets {
    use tracing_subscriber::filter::LevelFilter;
    let filter = if filter.default_level().is_none() {
        filter.with_default(LevelFilter::WARN)
    } else {
        filter
    };
    [TELEMETRY_TARGET, SPAN_TARGET]
        .into_iter()
        .fold(filter, |filter, own| {
            if filter.iter().any(|(target, _)| target == own) {
                filter
            } else {
                filter.with_target(own, LevelFilter::INFO)
            }
        })
}

/// Random id for this process (`service.instance.id`), without pulling in a
/// UUID crate: std's `RandomState` is seeded from OS randomness.
pub fn instance_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        use std::hash::{BuildHasher, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        format!("{:016x}{:08x}", hasher.finish(), std::process::id())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_filter_adds_a_default_and_keeps_telemetry_visible() {
        use tracing::Level;
        let f = complete_filter("desktop_agent_lib=debug".parse().unwrap());
        assert!(f.would_enable("desktop_agent_lib", &Level::DEBUG));
        assert!(f.would_enable("tao", &Level::WARN), "warn default added");
        assert!(!f.would_enable("tao", &Level::INFO));
        assert!(f.would_enable(TELEMETRY_TARGET, &Level::INFO));
        assert!(
            f.would_enable(SPAN_TARGET, &Level::INFO),
            "tracing on at warn"
        );

        let f = complete_filter("error,telemetry=off".parse().unwrap());
        assert!(
            !f.would_enable(TELEMETRY_TARGET, &Level::ERROR),
            "explicit setting kept"
        );
        assert!(
            !f.would_enable("tao", &Level::WARN),
            "explicit default kept"
        );
    }

    #[test]
    fn instance_id_is_stable_within_a_process() {
        assert_eq!(instance_id(), instance_id());
        assert_eq!(instance_id().len(), 24);
    }

    #[cfg(feature = "otlp")]
    #[test]
    fn shutdown_is_capped_when_the_collector_is_unreachable() {
        // A listener that accepts connections and never answers: exports block
        // until their own timeout, which is longer than FLUSH_CAP.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for s in listener.incoming().flatten() {
                held.push(s);
            }
        });
        let mut cfg = TelemetryConfig::default();
        cfg.otlp.endpoint = Some(format!("http://{addr}"));
        cfg.otlp.timeout_ms = 30_000;
        let resolved = resolve(&cfg, "test", &|_| None);
        let otlp = resolved.otlp.as_ref().unwrap();
        let (providers, errors) = otlp::Providers::exporting(&resolved, otlp, "0.0.0");
        assert!(errors.is_empty(), "{errors:?}");

        // Queue one span so the trace exporter has work to flush.
        {
            use opentelemetry::trace::{Tracer, TracerProvider as _};
            let tracer = providers.tracer.tracer("t");
            tracer.in_span("pending", |_| {});
        }

        let mut telemetry = Telemetry {
            providers: Some(providers),
            exporting: true,
            file_guard: None,
        };
        let started = std::time::Instant::now();
        telemetry.shutdown();
        assert!(
            // Generous margin: CI runners can be slow to schedule threads.
            started.elapsed() < FLUSH_CAP + Duration::from_secs(2),
            "shutdown took {:?}",
            started.elapsed()
        );
        telemetry.shutdown(); // idempotent
    }

    /// Minimal HTTP/1.1 responder: answers every request `200` and reports
    /// each request path on the returned channel.
    #[cfg(feature = "otlp")]
    fn fake_collector() -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut stream = stream;
                    loop {
                        let mut request_line = String::new();
                        if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                            return;
                        }
                        let path = request_line
                            .split_whitespace()
                            .nth(1)
                            .unwrap_or("")
                            .to_string();
                        let mut len = 0usize;
                        loop {
                            let mut line = String::new();
                            reader.read_line(&mut line).unwrap();
                            if line == "\r\n" {
                                break;
                            }
                            if let Some(v) =
                                line.to_ascii_lowercase().strip_prefix("content-length:")
                            {
                                len = v.trim().parse().unwrap_or(0);
                            }
                        }
                        let mut body = vec![0; len];
                        reader.read_exact(&mut body).unwrap();
                        let _ = tx.send(path);
                        stream
                            .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: application/x-protobuf\r\ncontent-length: 0\r\n\r\n")
                            .unwrap();
                    }
                });
            }
        });
        (url, rx)
    }

    #[cfg(feature = "otlp")]
    #[test]
    fn exports_reach_the_collector() {
        let (url, requests) = fake_collector();
        let mut cfg = TelemetryConfig::default();
        cfg.otlp.endpoint = Some(url);
        let resolved = resolve(&cfg, "test", &|_| None);
        let otlp = resolved.otlp.as_ref().unwrap();
        let (providers, errors) = otlp::Providers::exporting(&resolved, otlp, "0.0.0");
        assert!(errors.is_empty(), "{errors:?}");
        {
            use opentelemetry::trace::{Tracer, TracerProvider as _};
            providers.tracer.tracer("t").in_span("exported", |_| {});
        }
        let result = providers.tracer.force_flush();
        assert!(result.is_ok(), "{result:?}");
        let path = requests
            .recv_timeout(Duration::from_secs(5))
            .expect("the trace export reached the collector");
        assert_eq!(path, "/v1/traces");
    }

    #[cfg(feature = "otlp")]
    #[test]
    fn log_events_are_exported_on_shutdown_within_the_cap() {
        let (url, requests) = fake_collector();
        let mut cfg = TelemetryConfig::default();
        cfg.otlp.endpoint = Some(url);
        let resolved = resolve(&cfg, "test", &|_| None);
        let otlp = resolved.otlp.as_ref().unwrap();
        let (providers, errors) = otlp::Providers::exporting(&resolved, otlp, "0.0.0");
        assert!(errors.is_empty(), "{errors:?}");
        let subscriber = tracing_subscriber::registry().with(providers.layers());
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "app", "hello collector");
        });
        let mut telemetry = Telemetry {
            providers: Some(providers),
            exporting: true,
            file_guard: None,
        };
        let started = std::time::Instant::now();
        telemetry.shutdown();
        let elapsed = started.elapsed();
        let paths: Vec<String> = requests.try_iter().collect();
        assert!(paths.iter().any(|p| p == "/v1/logs"), "requests: {paths:?}");
        assert!(elapsed < FLUSH_CAP, "shutdown took {elapsed:?}");
    }

    #[cfg(feature = "otlp")]
    #[test]
    fn spans_have_trace_ids_even_when_not_exporting() {
        use opentelemetry::trace::TraceContextExt;
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        let resolved = resolve(&TelemetryConfig::default(), "test", &|_| None);
        let providers = otlp::Providers::local_only(&resolved, "0.0.0");
        let subscriber = tracing_subscriber::registry().with(providers.layers());
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("work");
            let cx = span.context();
            assert!(cx.span().span_context().is_valid());
            assert!(!cx.span().span_context().is_sampled());
        });
    }
}
