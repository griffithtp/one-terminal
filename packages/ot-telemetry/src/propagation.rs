//! W3C trace context between processes, following OpenTelemetry's environment
//! carrier: a parent sets `TRACEPARENT` (and `TRACESTATE`) on the child it
//! spawns, and the child reads them once at startup.
//!
//! The parser has no OpenTelemetry dependency, so builds without the `otlp`
//! feature (`tauri-webview-host`) can still stamp the parent's trace id on
//! their log lines.

use std::process::Command;
use std::sync::OnceLock;

use crate::config::Resolved;

pub const TRACEPARENT: &str = "TRACEPARENT";
pub const TRACESTATE: &str = "TRACESTATE";

/// A parsed `traceparent` value (version `00`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceParent {
    /// 32 lowercase hex characters, not all zero.
    pub trace_id: String,
    /// 16 lowercase hex characters, not all zero.
    pub span_id: String,
    pub sampled: bool,
}

impl TraceParent {
    /// Parse `00-<trace-id>-<span-id>-<flags>`. Anything else — other
    /// versions, wrong lengths, uppercase, all-zero ids — is rejected, as the
    /// W3C spec requires.
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.trim().split('-');
        let (version, trace_id, span_id, flags) =
            (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() || version != "00" {
            return None;
        }
        let valid = |s: &str, len: usize| {
            s.len() == len
                && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                && s.bytes().any(|b| b != b'0')
        };
        if !valid(trace_id, 32) || !valid(span_id, 16) || flags.len() != 2 {
            return None;
        }
        let flags = u8::from_str_radix(flags, 16).ok()?;
        Some(Self {
            trace_id: trace_id.to_string(),
            span_id: span_id.to_string(),
            sampled: flags & 0x01 == 0x01,
        })
    }

    pub fn header_value(&self) -> String {
        format!(
            "00-{}-{}-{}",
            self.trace_id,
            self.span_id,
            if self.sampled { "01" } else { "00" }
        )
    }
}

/// The `TRACEPARENT` this process was started with, read once.
pub fn startup_parent() -> Option<&'static TraceParent> {
    static PARENT: OnceLock<Option<TraceParent>> = OnceLock::new();
    PARENT
        .get_or_init(|| {
            std::env::var(TRACEPARENT)
                .ok()
                .and_then(|v| TraceParent::parse(&v))
        })
        .as_ref()
}

/// How much of this process's telemetry setup a child process receives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildKind {
    /// A framework process that exports itself (the Terminal): trace context,
    /// log settings and the export settings from this process's config file,
    /// including OTLP headers.
    Exporting,
    /// A framework process that only writes local logs (`tauri-webview-host`,
    /// the Electron host): trace context and log settings. Export variables
    /// that can carry credentials are removed — it never exports.
    LogsOnly,
    /// Third-party code (engine plugin binaries): trace context only, with the
    /// same credential-bearing variables removed.
    Untrusted,
}

/// `OTEL_EXPORTER_OTLP_*` settings that may carry credentials (headers, URLs
/// with userinfo or API keys, TLS key paths) in their general and per-signal
/// forms.
const CREDENTIAL_SETTINGS: [&str; 5] = [
    "HEADERS",
    "ENDPOINT",
    "CERTIFICATE",
    "CLIENT_KEY",
    "CLIENT_CERTIFICATE",
];
const SIGNAL_PREFIXES: [&str; 4] = ["", "TRACES_", "METRICS_", "LOGS_"];
const SIGNALS: [&str; 3] = ["TRACES", "METRICS", "LOGS"];

/// Every variable a non-exporting child must not inherit.
fn credential_vars() -> impl Iterator<Item = String> {
    SIGNAL_PREFIXES
        .iter()
        .flat_map(|prefix| {
            CREDENTIAL_SETTINGS
                .iter()
                .map(move |setting| format!("OTEL_EXPORTER_OTLP_{prefix}{setting}"))
        })
        .chain(std::iter::once("OT_OTLP_ENDPOINT".to_string()))
}

/// Settings children receive, captured by [`crate::init`]: environment
/// variables already reach a child by inheritance, but settings read from this
/// process's config file don't.
#[derive(Debug, Default)]
pub(crate) struct ChildSettings {
    /// `OsString` values: the log directory must reach the child unchanged,
    /// whatever its encoding.
    logs: Vec<(String, std::ffi::OsString)>,
    export: Vec<(String, String)>,
}

static CHILD_SETTINGS: OnceLock<ChildSettings> = OnceLock::new();

/// Record the settings to pass to children (called once from `init`).
pub(crate) fn capture_child_settings(resolved: &Resolved, var: &dyn Fn(&str) -> Option<String>) {
    let _ = CHILD_SETTINGS.set(child_settings(resolved, var));
}

fn child_settings(resolved: &Resolved, var: &dyn Fn(&str) -> Option<String>) -> ChildSettings {
    let mut logs = vec![("OT_LOG".to_string(), resolved.log_filter.clone().into())];
    if let Some(dir) = &resolved.log_dir {
        logs.push(("OT_LOG_DIR".to_string(), dir.as_os_str().to_owned()));
    }
    let mut export = Vec::new();
    if let Some(otlp) = &resolved.otlp {
        for (signal, url) in SIGNALS.iter().zip([
            &otlp.traces_endpoint,
            &otlp.metrics_endpoint,
            &otlp.logs_endpoint,
        ]) {
            if let Some(url) = url {
                export.push((format!("OTEL_EXPORTER_OTLP_{signal}_ENDPOINT"), url.clone()));
            }
        }
        if !otlp.headers.0.is_empty() {
            let config = otlp
                .headers
                .0
                .iter()
                .map(|(k, v)| format!("{k}={}", encode_header_value(v)))
                .collect::<Vec<_>>()
                .join(",");
            // Config first, inherited value last: the exporter keeps the last
            // duplicate key, and in this process env headers win over config.
            // Per-signal variables shadow the general one, so merge into each
            // that is set.
            for prefix in SIGNAL_PREFIXES {
                let name = format!("OTEL_EXPORTER_OTLP_{prefix}HEADERS");
                let inherited = var(&name).filter(|h| !h.trim().is_empty());
                if prefix.is_empty() || inherited.is_some() {
                    let merged = match inherited {
                        Some(env) => format!("{config},{env}"),
                        None => config.clone(),
                    };
                    export.push((name, merged));
                }
            }
        }
        export.push((
            "OTEL_TRACES_SAMPLER".to_string(),
            "parentbased_traceidratio".to_string(),
        ));
        export.push((
            "OTEL_TRACES_SAMPLER_ARG".to_string(),
            otlp.sample_ratio.to_string(),
        ));
        export.push((
            "OTEL_EXPORTER_OTLP_TIMEOUT".to_string(),
            otlp.timeout.as_millis().to_string(),
        ));
        export.push((
            "OTEL_METRIC_EXPORT_INTERVAL".to_string(),
            otlp.metrics_interval.as_millis().to_string(),
        ));
    }
    ChildSettings { logs, export }
}

/// Percent-encode the characters that would break `key=value,key=value` (the
/// `OTEL_EXPORTER_OTLP_HEADERS` format). Keys aren't decoded by the exporter,
/// and header names can't contain these characters anyway.
fn encode_header_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '%' => out.push_str("%25"),
            ',' => out.push_str("%2C"),
            '=' => out.push_str("%3D"),
            ' ' => out.push_str("%20"),
            c => out.push(c),
        }
    }
    out
}

/// Prepare `cmd`'s environment for a child process of the given `kind`:
/// - `TRACEPARENT` set to the current span, so the child's startup joins this
///   trace (or removed, outside a span, so it doesn't inherit ours);
/// - `OTEL_SERVICE_NAME` removed — it would rename every child — and the
///   widget endpoint variables, which name one window;
/// - log and export settings per [`ChildKind`].
pub fn inject_env(cmd: &mut Command, kind: ChildKind) {
    apply(cmd, kind, current(), CHILD_SETTINGS.get());
}

fn apply(
    cmd: &mut Command,
    kind: ChildKind,
    parent: Option<TraceParent>,
    settings: Option<&ChildSettings>,
) {
    // A widget token names one window: a child never inherits its parent's
    // (`ingress::Ingress::configure_host` sets a host's own).
    cmd.env_remove(TRACEPARENT)
        .env_remove(TRACESTATE)
        .env_remove("OTEL_SERVICE_NAME")
        .env_remove(crate::widget::URL_ENV)
        .env_remove(crate::widget::TOKEN_ENV);
    if let Some(parent) = parent {
        cmd.env(TRACEPARENT, parent.header_value());
    }
    if kind != ChildKind::Exporting {
        for var in credential_vars() {
            cmd.env_remove(var);
        }
    }
    if kind == ChildKind::Untrusted {
        return;
    }
    if let Some(settings) = settings {
        cmd.envs(settings.logs.iter().map(|(k, v)| (k, v)));
        if kind == ChildKind::Exporting {
            cmd.envs(settings.export.iter().map(|(k, v)| (k, v)));
        }
    }
}

/// The current tracing span as a `TraceParent`, when it carries a valid
/// OpenTelemetry context.
#[cfg(feature = "otlp")]
fn current() -> Option<TraceParent> {
    use opentelemetry::trace::TraceContextExt;
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let cx = tracing::Span::current().context();
    let span = cx.span();
    let sc = span.span_context();
    sc.is_valid().then(|| TraceParent {
        trace_id: sc.trace_id().to_string(),
        span_id: sc.span_id().to_string(),
        sampled: sc.is_sampled(),
    })
}

#[cfg(not(feature = "otlp"))]
fn current() -> Option<TraceParent> {
    None
}

/// An OpenTelemetry context whose remote parent is `parent`.
#[cfg(feature = "otlp")]
pub fn remote_context(parent: &TraceParent) -> opentelemetry::Context {
    use opentelemetry::trace::{
        SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState,
    };

    let flags = if parent.sampled {
        TraceFlags::SAMPLED
    } else {
        TraceFlags::default()
    };
    let sc = SpanContext::new(
        TraceId::from_hex(&parent.trace_id).unwrap_or(TraceId::INVALID),
        SpanId::from_hex(&parent.span_id).unwrap_or(SpanId::INVALID),
        flags,
        true,
        TraceState::default(),
    );
    opentelemetry::Context::new().with_remote_span_context(sc)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    #[test]
    fn parses_and_round_trips() {
        let tp = TraceParent::parse(VALID).unwrap();
        assert_eq!(tp.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(tp.span_id, "00f067aa0ba902b7");
        assert!(tp.sampled);
        assert_eq!(tp.header_value(), VALID);
    }

    #[test]
    fn rejects_invalid_values() {
        for bad in [
            "",
            "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e473-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-extra",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-zz",
        ] {
            assert_eq!(TraceParent::parse(bad), None, "{bad:?}");
        }
    }

    fn env_of(cmd: &Command) -> Vec<(String, Option<String>)> {
        cmd.get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }

    #[test]
    fn inject_env_clears_inherited_variables_outside_a_span() {
        // A command that is configured but never spawned.
        let mut cmd = Command::new("child");
        cmd.env(TRACEPARENT, VALID).env("OTEL_SERVICE_NAME", "x");
        inject_env(&mut cmd, ChildKind::Untrusted);
        let env = env_of(&cmd);
        assert!(env.contains(&(TRACEPARENT.to_string(), None)));
        assert!(env.contains(&("OTEL_SERVICE_NAME".to_string(), None)));
        assert!(env.contains(&("OTEL_EXPORTER_OTLP_HEADERS".to_string(), None)));
        assert!(env.contains(&(crate::widget::TOKEN_ENV.to_string(), None)));
    }

    fn resolved_with_export() -> Resolved {
        let mut cfg = crate::TelemetryConfig::default();
        cfg.otlp.endpoint = Some("https://collector.example".into());
        cfg.otlp
            .headers
            .0
            .insert("x-api-key".into(), "k=1, two".into());
        cfg.log_level = Some("debug".into());
        crate::resolve(&cfg, "parent", &|_| None)
    }

    fn find<'a>(list: &'a [(String, String)], key: &str) -> Option<&'a str> {
        list.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    #[test]
    fn child_settings_carry_config_export_settings() {
        let env = |k: &str| match k {
            "OTEL_EXPORTER_OTLP_HEADERS" => Some("x-api-key=env".to_string()),
            "OTEL_EXPORTER_OTLP_TRACES_HEADERS" => Some("t=1".to_string()),
            _ => None,
        };
        let s = child_settings(&resolved_with_export(), &env);
        let log = s
            .logs
            .iter()
            .find(|(k, _)| k == "OT_LOG")
            .map(|(_, v)| v.clone());
        assert_eq!(log, Some("debug".into()));
        assert_eq!(
            find(&s.export, "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT"),
            Some("https://collector.example/v1/traces")
        );
        assert_eq!(
            find(&s.export, "OTEL_EXPORTER_OTLP_HEADERS"),
            Some("x-api-key=k%3D1%2C%20two,x-api-key=env"),
            "config first so the inherited env value wins on a duplicate key"
        );
        assert_eq!(
            find(&s.export, "OTEL_EXPORTER_OTLP_TRACES_HEADERS"),
            Some("x-api-key=k%3D1%2C%20two,t=1"),
            "config merged into a per-signal variable that would shadow it"
        );
        assert_eq!(find(&s.export, "OTEL_EXPORTER_OTLP_LOGS_HEADERS"), None);
    }

    #[test]
    fn export_off_passes_no_export_settings() {
        let resolved = crate::resolve(&crate::TelemetryConfig::default(), "p", &|_| None);
        assert!(child_settings(&resolved, &|_| None).export.is_empty());
    }

    #[test]
    fn each_child_kind_gets_exactly_its_settings() {
        let settings = child_settings(&resolved_with_export(), &|_| None);
        let parent = TraceParent::parse(VALID);
        let run = |kind| {
            let mut cmd = Command::new("child");
            cmd.env("OT_OTLP_ENDPOINT", "https://u:p@c")
                .env("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT", "https://c/?key=1")
                .env("OTEL_EXPORTER_OTLP_CLIENT_KEY", "/k.pem");
            apply(&mut cmd, kind, parent.clone(), Some(&settings));
            env_of(&cmd)
        };
        let has = |env: &[(String, Option<String>)], k: &str| {
            env.iter().any(|(key, v)| key == k && v.is_some())
        };
        let removed = |env: &[(String, Option<String>)], k: &str| {
            env.iter().any(|(key, v)| key == k && v.is_none())
        };

        let exporting = run(ChildKind::Exporting);
        assert!(has(&exporting, TRACEPARENT));
        assert!(has(&exporting, "OT_LOG"));
        assert!(has(&exporting, "OTEL_EXPORTER_OTLP_HEADERS"));

        for kind in [ChildKind::LogsOnly, ChildKind::Untrusted] {
            let env = run(kind);
            assert!(has(&env, TRACEPARENT), "{kind:?}");
            assert!(!has(&env, "OTEL_EXPORTER_OTLP_HEADERS"), "{kind:?}");
            for var in [
                "OT_OTLP_ENDPOINT",
                "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
                "OTEL_EXPORTER_OTLP_CLIENT_KEY",
                "OTEL_EXPORTER_OTLP_LOGS_HEADERS",
            ] {
                assert!(removed(&env, var), "{kind:?} keeps {var}");
            }
        }
        assert!(has(&run(ChildKind::LogsOnly), "OT_LOG"));
        assert!(
            !has(&run(ChildKind::Untrusted), "OT_LOG"),
            "untrusted: trace context only"
        );
    }

    #[cfg(feature = "otlp")]
    #[test]
    fn inject_env_round_trips_through_a_span() {
        use opentelemetry::trace::TracerProvider as _;
        use tracing_subscriber::layer::SubscriberExt;

        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("spawn");
            let _entered = span.enter();
            let mut cmd = Command::new("child");
            inject_env(&mut cmd, ChildKind::LogsOnly);
            let value = cmd
                .get_envs()
                .find(|(k, _)| *k == TRACEPARENT)
                .and_then(|(_, v)| v)
                .map(|v| v.to_string_lossy().into_owned())
                .expect("TRACEPARENT set inside a span");
            assert!(
                TraceParent::parse(&value).is_some(),
                "valid traceparent: {value}"
            );
        });
    }

    #[cfg(feature = "otlp")]
    #[test]
    fn remote_context_carries_the_parent_ids() {
        use opentelemetry::trace::TraceContextExt;
        let tp = TraceParent::parse(VALID).unwrap();
        let cx = remote_context(&tp);
        let span = cx.span();
        let sc = span.span_context();
        assert!(sc.is_remote() && sc.is_valid());
        assert_eq!(sc.trace_id().to_string(), tp.trace_id);
    }
}
