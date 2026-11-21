//! Local log output, always on: JSON lines in a daily file per service, plus
//! human-readable lines on stderr.
//!
//! Several processes may append to the same file (every Terminal process is
//! `one-terminal`); each line carries `pid`, and files are never renamed
//! (dated names), so processes don't race on rotation.

use serde_json::{Map, Value};
use std::fmt;
use std::io::IsTerminal;
use std::path::Path;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_log::NormalizeEvent;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::{FormatTime, SystemTime};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

/// Lines queued for the file-writer thread before new ones are dropped.
/// tracing-appender's default (128,000) preallocates ~4 MB per process; our
/// volume is a few lines per second at most, so a burst of 4,096 is ample.
const BUFFERED_LINES: usize = 4_096;

/// Open `<dir>/<service>.YYYY-MM-DD.log` for appending, keeping `retention`
/// files. Returns the non-blocking writer's guard — dropping it flushes.
pub(crate) fn file_layer<S>(
    dir: &Path,
    service: &str,
    retention: usize,
) -> Result<(Box<dyn Layer<S> + Send + Sync>, WorkerGuard), String>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    create_private_dir(dir).map_err(|e| format!("create log dir {}: {e}", dir.display()))?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(file_stem(service))
        .filename_suffix("log")
        .max_log_files(retention)
        .build(dir)
        .map_err(|e| format!("open log file in {}: {e}", dir.display()))?;
    let (writer, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .buffered_lines_limit(BUFFERED_LINES)
        .lossy(true)
        .finish(appender);
    let layer = tracing_subscriber::fmt::layer()
        .event_format(JsonLines)
        .with_writer(writer)
        .with_ansi(false);
    Ok((Box::new(layer), guard))
}

/// `service` made safe as a file name: `OTEL_SERVICE_NAME` is user-supplied,
/// and `:`, `\\`, `/` or a trailing dot would fail on Windows or leave the log
/// directory. Only the file name is sanitised; `service.name` keeps the value.
fn file_stem(service: &str) -> String {
    let stem: String = service
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = stem.trim_matches('.');
    if stem.is_empty() {
        "service".to_string()
    } else {
        stem.to_string()
    }
}

/// Create `dir` (and parents). On Unix the leaf is owner-only (`0700`) —
/// logs carry app ids, paths and URLs. On Windows `%LOCALAPPDATA%`'s ACL
/// already limits access to the user, so no extra step.
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Compact text on stderr, coloured when stderr is a terminal.
pub(crate) fn stderr_layer<S>() -> Box<dyn Layer<S> + Send + Sync>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    Box::new(
        tracing_subscriber::fmt::layer()
            .compact()
            .with_writer(std::io::stderr)
            .with_ansi(std::io::stderr().is_terminal()),
    )
}

/// One JSON object per line:
/// `{"timestamp","level","target","pid","message",<fields>…,"span","trace_id","span_id"}`.
///
/// `trace_id`/`span_id` come from the active OpenTelemetry context (builds
/// with `otlp`). Without it — or outside any span — the `TRACEPARENT` this
/// process was started with is used, as `trace_id` + `parent_span_id`, so a
/// child's log lines still link to the span that launched it.
pub(crate) struct JsonLines;

impl<S, N> FormatEvent<S, N> for JsonLines
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        // Records from the `log` crate (tao, wry, tauri) arrive with target
        // "log"; restore their real target and drop the `log.*` fields.
        let normalized = event.normalized_metadata();
        let meta = normalized.as_ref().unwrap_or_else(|| event.metadata());
        let mut obj = Map::new();

        let mut ts = String::new();
        SystemTime.format_time(&mut Writer::new(&mut ts))?;
        obj.insert("timestamp".into(), Value::String(ts));
        obj.insert("level".into(), Value::String(meta.level().as_str().into()));
        obj.insert("target".into(), Value::String(meta.target().into()));
        obj.insert("pid".into(), Value::from(std::process::id()));

        let mut fields = FieldMap(Map::new());
        event.record(&mut fields);
        let message = fields
            .0
            .remove("message")
            .unwrap_or(Value::String(String::new()));
        obj.insert("message".into(), message);
        for (k, v) in fields.0 {
            obj.entry(k).or_insert(v);
        }

        if let Some(span) = ctx.lookup_current() {
            obj.insert("span".into(), Value::String(span.name().into()));
        }
        insert_trace_ids(&mut obj);

        let line = serde_json::to_string(&obj).map_err(|_| fmt::Error)?;
        writeln!(writer, "{line}")
    }
}

#[cfg(feature = "otlp")]
fn insert_trace_ids(obj: &mut Map<String, Value>) {
    use opentelemetry::trace::TraceContextExt;
    // tracing-opentelemetry activates the entered span's context, so the
    // current OpenTelemetry context is the event's span.
    let cx = opentelemetry::Context::current();
    let span = cx.span();
    let sc = span.span_context();
    if sc.is_valid() {
        obj.insert("trace_id".into(), Value::String(sc.trace_id().to_string()));
        obj.insert("span_id".into(), Value::String(sc.span_id().to_string()));
    } else {
        insert_startup_parent(obj);
    }
}

#[cfg(not(feature = "otlp"))]
fn insert_trace_ids(obj: &mut Map<String, Value>) {
    insert_startup_parent(obj);
}

fn insert_startup_parent(obj: &mut Map<String, Value>) {
    if let Some(parent) = crate::propagation::startup_parent() {
        obj.insert("trace_id".into(), Value::String(parent.trace_id.clone()));
        obj.insert(
            "parent_span_id".into(),
            Value::String(parent.span_id.clone()),
        );
    }
}

/// Collects an event's fields as JSON values, skipping the `log.*` fields
/// `tracing-log` adds to records bridged from the `log` crate.
struct FieldMap(Map<String, Value>);

impl FieldMap {
    fn insert(&mut self, field: &Field, value: Value) {
        if !field.name().starts_with("log.") {
            self.0.insert(field.name().into(), value);
        }
    }
}

impl Visit for FieldMap {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, Value::String(value.into()));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, Value::Bool(value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, Value::from(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field, Value::from(value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.insert(field, Value::from(value));
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.insert(field, Value::String(value.to_string()));
    }
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert(field, Value::String(format!("{value:?}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt;

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);

    impl Write for Buf {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn capture(f: impl FnOnce()) -> Vec<Value> {
        let buf = Buf::default();
        let out = buf.clone();
        let layer = tracing_subscriber::fmt::layer()
            .event_format(JsonLines)
            .with_writer(move || out.clone());
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), f);
        let text = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        text.lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn writes_one_json_object_per_event() {
        let lines = capture(|| {
            tracing::warn!(target: "wm", attempts = 3, ok = false, "popout failed: {}", "boom");
        });
        assert_eq!(lines.len(), 1);
        let l = &lines[0];
        assert_eq!(l["level"], "WARN");
        assert_eq!(l["target"], "wm");
        assert_eq!(l["message"], "popout failed: boom");
        assert_eq!(l["attempts"], 3);
        assert_eq!(l["ok"], false);
        assert_eq!(l["pid"], std::process::id());
        assert!(l["timestamp"].as_str().unwrap().ends_with('Z'));
    }

    #[test]
    fn file_stem_is_safe_on_every_os() {
        assert_eq!(file_stem("desktop-agent"), "desktop-agent");
        assert_eq!(file_stem("my:svc/a\\b "), "my_svc_a_b_");
        assert_eq!(file_stem("../x."), "_x");
        assert_eq!(file_stem(""), "service");
    }

    #[test]
    fn log_crate_records_keep_their_real_target() {
        let _ = tracing_log::LogTracer::init();
        let lines = capture(|| log::warn!(target: "tao::event", "from the log crate"));
        let line = lines
            .iter()
            .find(|l| l["message"] == "from the log crate")
            .expect("log record bridged");
        assert_eq!(line["target"], "tao::event");
        assert!(
            line.as_object()
                .unwrap()
                .keys()
                .all(|k| !k.starts_with("log.")),
            "{line}"
        );
    }

    #[test]
    fn file_layer_writes_dated_json_lines() {
        let dir = std::env::temp_dir().join(format!("ot-telemetry-logs-{}", std::process::id()));
        let (layer, guard) =
            file_layer::<tracing_subscriber::Registry>(&dir, "test:svc", 2).unwrap();
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
            tracing::info!(target: "app", "to disk");
        });
        drop(guard); // flush
        let files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
        let name = &files[0];
        assert!(
            name.starts_with("test_svc.") && name.ends_with(".log"),
            "{name}"
        );
        let text = std::fs::read_to_string(dir.join(name)).unwrap();
        let line: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(line["message"], "to disk");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn records_the_current_span_name() {
        let lines = capture(|| {
            let span = tracing::info_span!("fdc3.request");
            let _e = span.enter();
            tracing::info!("handled");
        });
        assert_eq!(lines[0]["span"], "fdc3.request");
    }

    #[cfg(feature = "otlp")]
    #[test]
    fn includes_trace_ids_inside_an_otel_span() {
        use opentelemetry::trace::TracerProvider as _;
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let buf = Buf::default();
        let out = buf.clone();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("t")))
            .with(
                tracing_subscriber::fmt::layer()
                    .event_format(JsonLines)
                    .with_writer(move || out.clone()),
            );
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("work");
            let _e = span.enter();
            tracing::info!("inside");
        });
        let text = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        let line: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(line["trace_id"].as_str().unwrap().len(), 32);
        assert_eq!(line["span_id"].as_str().unwrap().len(), 16);
    }
}
