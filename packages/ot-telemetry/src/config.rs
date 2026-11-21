//! The `telemetry` config block and [`resolve`], which applies environment
//! overrides and the opt-in rule. `resolve` is pure — the environment is
//! passed in — so precedence is unit-tested without touching process env.
//!
//! Opt-in: nothing is exported unless an endpoint comes from `OT_OTLP_ENDPOINT`,
//! a standard `OTEL_EXPORTER_OTLP_*ENDPOINT` variable or the config file, and
//! `OTEL_SDK_DISABLED` is not `true`. Local log files are always written.

use crate::redact::{self, Redacted};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

/// Log filter used when neither `OT_LOG`, `RUST_LOG` nor `telemetry.logLevel`
/// is set. tao and wry log every window event at `info`.
pub const DEFAULT_LOG_FILTER: &str = "info,tao=warn,wry=warn";
/// Daily log files kept per service.
pub const DEFAULT_LOG_RETENTION_DAYS: usize = 7;
/// The only OTLP protocol supported: HTTP with protobuf bodies (no gRPC, so no
/// tonic/protoc in the build).
pub const PROTOCOL_HTTP_PROTOBUF: &str = "http/protobuf";

// ── Config file shape ─────────────────────────────────────────────────────────

/// `telemetry` in `agent.config.json` / `terminal.config.json`. Every key is
/// optional; defaults are compiled in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TelemetryConfig {
    /// Log filter: a default level plus `target=level` pairs, e.g. `"info"` or
    /// `"info,desktop_agent_lib::fdc3_bus=debug"`.
    pub log_level: Option<String>,
    /// Directory for log files; `None` uses [`crate::paths::default_log_dir`].
    pub log_dir: Option<PathBuf>,
    pub log_retention_days: usize,
    /// `service.namespace` resource attribute shared by every process.
    pub service_namespace: String,
    /// Extra resource attributes (`deployment.environment`, …). Merged under
    /// `OTEL_RESOURCE_ATTRIBUTES`.
    pub resource_attributes: BTreeMap<String, String>,
    pub otlp: OtlpConfig,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            log_level: None,
            log_dir: None,
            log_retention_days: DEFAULT_LOG_RETENTION_DAYS,
            service_namespace: "one-terminal".to_string(),
            resource_attributes: BTreeMap::new(),
            otlp: OtlpConfig::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OtlpConfig {
    /// Collector base URL, e.g. `http://localhost:4318`. `None` = no export.
    pub endpoint: Option<String>,
    pub protocol: String,
    /// Sent with every export request (auth tokens). Never logged.
    pub headers: Redacted<BTreeMap<String, String>>,
    pub timeout_ms: u64,
    /// Fraction of new traces recorded (parent-based: a sampled parent from
    /// another process is always followed).
    pub sample_ratio: f64,
    pub metrics_interval_ms: u64,
    pub signals: Signals,
}

impl Default for OtlpConfig {
    fn default() -> Self {
        Self {
            endpoint: None,
            protocol: PROTOCOL_HTTP_PROTOBUF.to_string(),
            headers: Redacted::default(),
            timeout_ms: 10_000,
            sample_ratio: 1.0,
            metrics_interval_ms: 60_000,
            signals: Signals::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Signals {
    pub traces: bool,
    pub metrics: bool,
    pub logs: bool,
}

impl Default for Signals {
    fn default() -> Self {
        Self {
            traces: true,
            metrics: true,
            logs: true,
        }
    }
}

// ── Resolved settings ─────────────────────────────────────────────────────────

/// Effective settings for one process, after environment overrides.
#[derive(Clone, Debug)]
pub struct Resolved {
    pub service_name: String,
    pub service_namespace: String,
    pub resource_attributes: BTreeMap<String, String>,
    pub log_filter: String,
    pub log_dir: Option<PathBuf>,
    pub log_retention_days: usize,
    /// `None` = export off.
    pub otlp: Option<ResolvedOtlp>,
    /// Problems found while resolving; logged once logging is up.
    pub warnings: Vec<String>,
    /// `(setting, value, source)` rows for the startup log. Values are
    /// already redacted.
    pub summary: Vec<(String, String, String)>,
}

#[derive(Clone)]
pub struct ResolvedOtlp {
    /// Full per-signal URLs (`…/v1/traces`); `None` = that signal is off.
    pub traces_endpoint: Option<String>,
    pub metrics_endpoint: Option<String>,
    pub logs_endpoint: Option<String>,
    /// Headers from the config file. `OTEL_EXPORTER_OTLP_HEADERS` is applied
    /// by the exporter itself.
    pub headers: Redacted<BTreeMap<String, String>>,
    pub timeout: Duration,
    pub sample_ratio: f64,
    pub metrics_interval: Duration,
}

/// Endpoints may embed credentials (`https://user:token@…`, `?api-key=…`), so
/// `Debug` shows them redacted.
impl std::fmt::Debug for ResolvedOtlp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let url = |u: &Option<String>| u.as_deref().map(redact::url);
        f.debug_struct("ResolvedOtlp")
            .field("traces_endpoint", &url(&self.traces_endpoint))
            .field("metrics_endpoint", &url(&self.metrics_endpoint))
            .field("logs_endpoint", &url(&self.logs_endpoint))
            .field("headers", &self.headers)
            .field("timeout", &self.timeout)
            .field("sample_ratio", &self.sample_ratio)
            .field("metrics_interval", &self.metrics_interval)
            .finish()
    }
}

/// Apply environment overrides to `cfg` for the process `service`.
///
/// Endpoint precedence, per signal: `OT_OTLP_ENDPOINT` →
/// `OTEL_EXPORTER_OTLP_<SIGNAL>_ENDPOINT` (used as-is) →
/// `OTEL_EXPORTER_OTLP_ENDPOINT` → `telemetry.otlp.endpoint`. Base URLs get the
/// signal path (`/v1/traces`) appended.
pub fn resolve(
    cfg: &TelemetryConfig,
    service: &str,
    var: &dyn Fn(&str) -> Option<String>,
) -> Resolved {
    let var = |key: &str| var(key).filter(|v| !v.trim().is_empty());
    let mut warnings = Vec::new();
    let mut summary = Vec::new();
    let mut row = |key: &str, value: String, source: &str| {
        summary.push((key.to_string(), value, source.to_string()));
    };

    let (service_name, source) = match var("OTEL_SERVICE_NAME") {
        Some(name) => (name, "env OTEL_SERVICE_NAME"),
        None => (service.to_string(), "default"),
    };
    row("telemetry.serviceName", service_name.clone(), source);

    let (log_filter, source) = if let Some(v) = var("OT_LOG") {
        (v, "env OT_LOG")
    } else if let Some(v) = var("RUST_LOG") {
        (v, "env RUST_LOG")
    } else if let Some(v) = cfg.log_level.clone().filter(|v| !v.trim().is_empty()) {
        (v, "config")
    } else {
        (DEFAULT_LOG_FILTER.to_string(), "default")
    };
    row("telemetry.logLevel", log_filter.clone(), source);

    let (log_dir, source) = if let Some(v) = var("OT_LOG_DIR") {
        (Some(PathBuf::from(v)), "env OT_LOG_DIR")
    } else if let Some(v) = cfg.log_dir.clone() {
        (Some(v), "config")
    } else {
        (crate::paths::default_log_dir(&var), "default")
    };
    row(
        "telemetry.logDir",
        log_dir
            .as_ref()
            .map_or("none (stderr only)".to_string(), |d| {
                d.display().to_string()
            }),
        source,
    );

    let log_retention_days = cfg.log_retention_days.max(1);
    let otlp = resolve_otlp(cfg, &var, &mut warnings, &mut row);

    Resolved {
        service_name,
        service_namespace: cfg.service_namespace.clone(),
        resource_attributes: cfg.resource_attributes.clone(),
        log_filter,
        log_dir,
        log_retention_days,
        otlp,
        warnings,
        summary,
    }
}

fn resolve_otlp(
    cfg: &TelemetryConfig,
    var: &dyn Fn(&str) -> Option<String>,
    warnings: &mut Vec<String>,
    row: &mut dyn FnMut(&str, String, &str),
) -> Option<ResolvedOtlp> {
    if var("OTEL_SDK_DISABLED").is_some_and(|v| v.trim().eq_ignore_ascii_case("true")) {
        row("telemetry.otlp", "off".into(), "env OTEL_SDK_DISABLED");
        return None;
    }

    let protocol = var("OTEL_EXPORTER_OTLP_PROTOCOL").unwrap_or_else(|| cfg.otlp.protocol.clone());
    if protocol != PROTOCOL_HTTP_PROTOBUF {
        warnings.push(format!(
            "OTLP protocol {protocol:?} is not supported; using {PROTOCOL_HTTP_PROTOBUF}"
        ));
    }

    let signals = cfg.otlp.signals;
    let mut endpoint_for = |enabled: bool, signal: &str, path: &str| -> Option<String> {
        if !enabled {
            return None;
        }
        let per_signal = format!("OTEL_EXPORTER_OTLP_{}_ENDPOINT", signal.to_uppercase());
        let (url, source) = if let Some(base) = var("OT_OTLP_ENDPOINT") {
            (
                join_signal_path(&base, path),
                "env OT_OTLP_ENDPOINT".to_string(),
            )
        } else if let Some(full) = var(&per_signal) {
            (full, format!("env {per_signal}"))
        } else if let Some(base) = var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            (
                join_signal_path(&base, path),
                "env OTEL_EXPORTER_OTLP_ENDPOINT".to_string(),
            )
        } else if let Some(base) = cfg.otlp.endpoint.clone().filter(|v| !v.trim().is_empty()) {
            (join_signal_path(&base, path), "config".to_string())
        } else {
            return None;
        };
        match check_endpoint(&url) {
            Ok(note) => {
                warnings.extend(note);
                row(
                    &format!("telemetry.otlp.{signal}"),
                    redact::url(&url),
                    &source,
                );
                Some(url)
            }
            Err(e) => {
                warnings.push(format!("OTLP {signal} endpoint ignored: {e}"));
                None
            }
        }
    };

    let traces_endpoint = endpoint_for(signals.traces, "traces", "/v1/traces");
    let metrics_endpoint = endpoint_for(signals.metrics, "metrics", "/v1/metrics");
    let logs_endpoint = endpoint_for(signals.logs, "logs", "/v1/logs");

    if traces_endpoint.is_none() && metrics_endpoint.is_none() && logs_endpoint.is_none() {
        row("telemetry.otlp", "off".into(), "default");
        return None;
    }

    let ratio_arg = match var("OTEL_TRACES_SAMPLER_ARG") {
        Some(v) => match v.trim().parse::<f64>() {
            Ok(r) if (0.0..=1.0).contains(&r) => r,
            _ => {
                warnings.push(format!(
                    "ignoring OTEL_TRACES_SAMPLER_ARG={v:?} (expected a number from 0 to 1)"
                ));
                cfg.otlp.sample_ratio
            }
        },
        None => cfg.otlp.sample_ratio,
    };
    // Sampling is always parent-based (a sampled parent process is followed);
    // the standard sampler names map onto the ratio.
    let sample_ratio = match var("OTEL_TRACES_SAMPLER").as_deref().map(str::trim) {
        None | Some("traceidratio" | "parentbased_traceidratio") => ratio_arg,
        Some("always_on" | "parentbased_always_on") => 1.0,
        Some("always_off" | "parentbased_always_off") => 0.0,
        Some(other) => {
            warnings.push(format!(
                "ignoring OTEL_TRACES_SAMPLER={other:?} (supported: always_on, always_off, traceidratio and their parentbased_ forms)"
            ));
            ratio_arg
        }
    }
    .clamp(0.0, 1.0);

    let timeout_ms =
        env_millis(var, "OTEL_EXPORTER_OTLP_TIMEOUT", warnings).unwrap_or(cfg.otlp.timeout_ms);
    let metrics_interval_ms = env_millis(var, "OTEL_METRIC_EXPORT_INTERVAL", warnings)
        .unwrap_or(cfg.otlp.metrics_interval_ms);

    let headers = cfg.otlp.headers.clone();
    if !headers.0.is_empty() {
        row(
            "telemetry.otlp.headers",
            format!("{} set (redacted)", headers.0.len()),
            "config",
        );
    }

    Some(ResolvedOtlp {
        traces_endpoint,
        metrics_endpoint,
        logs_endpoint,
        headers,
        timeout: Duration::from_millis(timeout_ms.max(1)),
        sample_ratio,
        metrics_interval: Duration::from_millis(metrics_interval_ms.max(MIN_METRICS_INTERVAL_MS)),
    })
}

/// Shortest metrics export interval; anything lower would flood the collector.
const MIN_METRICS_INTERVAL_MS: u64 = 1_000;

/// A millisecond count from the environment, warning on garbage.
fn env_millis(
    var: &dyn Fn(&str) -> Option<String>,
    key: &str,
    warnings: &mut Vec<String>,
) -> Option<u64> {
    let raw = var(key)?;
    match raw.trim().parse() {
        Ok(ms) => Some(ms),
        Err(_) => {
            warnings.push(format!("ignoring {key}={raw:?} (expected milliseconds)"));
            None
        }
    }
}

/// `base` + `path`, without doubling the slash.
fn join_signal_path(base: &str, path: &str) -> String {
    format!("{}{path}", base.trim_end_matches('/'))
}

/// Accept only `http(s)` URLs. Returns a warning for plain `http` to a host
/// other than loopback (credentials and telemetry would travel unencrypted).
fn check_endpoint(url: &str) -> Result<Option<String>, String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| format!("{} is not a URL", redact::url(url)))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if let Some(v6) = host_port.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    if host.is_empty() {
        return Err(format!("{} has no host", redact::url(url)));
    }
    match scheme {
        "https" => Ok(None),
        "http" => {
            let loopback = host == "localhost"
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback());
            Ok((!loopback).then(|| {
                format!(
                    "OTLP endpoint {} uses plain http to a non-loopback host; telemetry is sent unencrypted",
                    redact::url(url)
                )
            }))
        }
        other => Err(format!("unsupported scheme {other:?} (use http or https)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |key| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    fn with_endpoint(endpoint: &str) -> TelemetryConfig {
        let mut cfg = TelemetryConfig::default();
        cfg.otlp.endpoint = Some(endpoint.to_string());
        cfg
    }

    #[test]
    fn export_is_off_by_default() {
        let r = resolve(&TelemetryConfig::default(), "desktop-agent", &env_with(&[]));
        assert!(r.otlp.is_none());
        assert!(r.warnings.is_empty());
        assert_eq!(r.service_name, "desktop-agent");
        assert_eq!(r.log_filter, DEFAULT_LOG_FILTER);
    }

    #[test]
    fn config_endpoint_gets_signal_paths() {
        let r = resolve(
            &with_endpoint("http://localhost:4318/"),
            "x",
            &env_with(&[]),
        );
        let otlp = r.otlp.unwrap();
        assert_eq!(
            otlp.traces_endpoint.as_deref(),
            Some("http://localhost:4318/v1/traces")
        );
        assert_eq!(
            otlp.metrics_endpoint.as_deref(),
            Some("http://localhost:4318/v1/metrics")
        );
        assert_eq!(
            otlp.logs_endpoint.as_deref(),
            Some("http://localhost:4318/v1/logs")
        );
    }

    #[test]
    fn endpoint_precedence() {
        let cfg = with_endpoint("http://config:4318");
        let all = env_with(&[
            ("OT_OTLP_ENDPOINT", "http://ot:4318"),
            (
                "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
                "http://signal:4318/custom",
            ),
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://generic:4318"),
        ]);
        let otlp = resolve(&cfg, "x", &all).otlp.unwrap();
        assert_eq!(
            otlp.traces_endpoint.as_deref(),
            Some("http://ot:4318/v1/traces")
        );

        let no_ot = env_with(&[
            (
                "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
                "http://signal:4318/custom",
            ),
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://generic:4318"),
        ]);
        let otlp = resolve(&cfg, "x", &no_ot).otlp.unwrap();
        assert_eq!(
            otlp.traces_endpoint.as_deref(),
            Some("http://signal:4318/custom")
        );
        assert_eq!(
            otlp.logs_endpoint.as_deref(),
            Some("http://generic:4318/v1/logs")
        );
    }

    #[test]
    fn sdk_disabled_wins_over_every_endpoint() {
        let env = env_with(&[
            ("OT_OTLP_ENDPOINT", "http://ot:4318"),
            ("OTEL_SDK_DISABLED", "TRUE"),
        ]);
        assert!(resolve(&with_endpoint("http://c:4318"), "x", &env)
            .otlp
            .is_none());
    }

    #[test]
    fn disabled_signals_get_no_endpoint() {
        let mut cfg = with_endpoint("http://localhost:4318");
        cfg.otlp.signals = Signals {
            traces: true,
            metrics: false,
            logs: false,
        };
        let otlp = resolve(&cfg, "x", &env_with(&[])).otlp.unwrap();
        assert!(otlp.traces_endpoint.is_some());
        assert!(otlp.metrics_endpoint.is_none() && otlp.logs_endpoint.is_none());
    }

    #[test]
    fn rejects_bad_schemes_and_warns_on_plain_http_to_remote() {
        let r = resolve(&with_endpoint("ftp://collector"), "x", &env_with(&[]));
        assert!(r.otlp.is_none());
        assert!(r.warnings.iter().any(|w| w.contains("unsupported scheme")));

        let r = resolve(
            &with_endpoint("http://collector.example:4318"),
            "x",
            &env_with(&[]),
        );
        assert!(r.otlp.is_some());
        assert!(r.warnings.iter().any(|w| w.contains("unencrypted")));

        let r = resolve(&with_endpoint("http://127.0.0.1:4318"), "x", &env_with(&[]));
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn loopback_check_parses_ip_addresses() {
        let r = resolve(
            &with_endpoint("http://127.evil.example:4318"),
            "x",
            &env_with(&[]),
        );
        assert!(r.warnings.iter().any(|w| w.contains("unencrypted")));
        for ok in [
            "http://127.0.0.2:4318",
            "http://[::1]:4318",
            "http://localhost:4318",
        ] {
            let r = resolve(&with_endpoint(ok), "x", &env_with(&[]));
            assert!(r.warnings.is_empty(), "{ok}: {:?}", r.warnings);
        }
    }

    #[test]
    fn standard_sampler_timeout_and_interval_vars() {
        let otlp = |pairs: &[(&str, &str)]| {
            resolve(
                &with_endpoint("http://localhost:4318"),
                "x",
                &env_with(pairs),
            )
            .otlp
            .unwrap()
        };
        assert!((otlp(&[("OTEL_TRACES_SAMPLER", "always_off")]).sample_ratio).abs() < f64::EPSILON);
        let o = otlp(&[
            ("OTEL_TRACES_SAMPLER", "parentbased_traceidratio"),
            ("OTEL_TRACES_SAMPLER_ARG", "0.5"),
            ("OTEL_EXPORTER_OTLP_TIMEOUT", "2500"),
            ("OTEL_METRIC_EXPORT_INTERVAL", "15000"),
        ]);
        assert!((o.sample_ratio - 0.5).abs() < f64::EPSILON);
        assert_eq!(o.timeout, Duration::from_millis(2500));
        assert_eq!(o.metrics_interval, Duration::from_millis(15000));

        let r = resolve(
            &with_endpoint("http://localhost:4318"),
            "x",
            &env_with(&[("OTEL_TRACES_SAMPLER", "jaeger_remote")]),
        );
        assert!(r.warnings.iter().any(|w| w.contains("OTEL_TRACES_SAMPLER")));
    }

    #[test]
    fn unsupported_protocol_warns() {
        let env = env_with(&[("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc")]);
        let r = resolve(&with_endpoint("http://localhost:4318"), "x", &env);
        assert!(r.warnings.iter().any(|w| w.contains("\"grpc\"")));
    }

    #[test]
    fn default_log_filter_parses() {
        assert!(DEFAULT_LOG_FILTER
            .parse::<tracing_subscriber::filter::Targets>()
            .is_ok());
    }

    #[test]
    fn log_filter_precedence() {
        let cfg = TelemetryConfig {
            log_level: Some("warn".into()),
            ..TelemetryConfig::default()
        };
        assert_eq!(resolve(&cfg, "x", &env_with(&[])).log_filter, "warn");
        let env = env_with(&[("RUST_LOG", "debug")]);
        assert_eq!(resolve(&cfg, "x", &env).log_filter, "debug");
        let env = env_with(&[("RUST_LOG", "debug"), ("OT_LOG", "trace")]);
        assert_eq!(resolve(&cfg, "x", &env).log_filter, "trace");
    }

    #[test]
    fn log_dir_env_overrides_config() {
        let cfg = TelemetryConfig {
            log_dir: Some(PathBuf::from("/from/config")),
            ..TelemetryConfig::default()
        };
        assert_eq!(
            resolve(&cfg, "x", &env_with(&[])).log_dir,
            Some(PathBuf::from("/from/config"))
        );
        let env = env_with(&[("OT_LOG_DIR", "/from/env")]);
        assert_eq!(
            resolve(&cfg, "x", &env).log_dir,
            Some(PathBuf::from("/from/env"))
        );
    }

    #[test]
    fn sampler_arg_is_validated() {
        let env = env_with(&[("OTEL_TRACES_SAMPLER_ARG", "0.25")]);
        let otlp = resolve(&with_endpoint("http://localhost:4318"), "x", &env)
            .otlp
            .unwrap();
        assert!((otlp.sample_ratio - 0.25).abs() < f64::EPSILON);

        let env = env_with(&[("OTEL_TRACES_SAMPLER_ARG", "7")]);
        let r = resolve(&with_endpoint("http://localhost:4318"), "x", &env);
        assert!((r.otlp.unwrap().sample_ratio - 1.0).abs() < f64::EPSILON);
        assert!(r
            .warnings
            .iter()
            .any(|w| w.contains("OTEL_TRACES_SAMPLER_ARG")));
    }

    #[test]
    fn summary_never_contains_secrets() {
        let mut cfg = with_endpoint("https://user:pw@collector.example/otlp?token=abc");
        cfg.otlp
            .headers
            .0
            .insert("authorization".into(), "Bearer s3cret".into());
        let r = resolve(&cfg, "x", &env_with(&[]));
        let text = format!("{:?} {:?}", r.summary, r.otlp);
        // Fixture values only; the failure message stays generic so a failing
        // run doesn't print them (and CodeQL doesn't flag it as logging).
        for (i, fixture) in ["pw", "token=abc", "s3cret"].iter().enumerate() {
            assert!(
                !text.contains(fixture),
                "fixture #{i} leaked into the summary"
            );
        }
    }

    #[test]
    fn service_name_env_override() {
        let env = env_with(&[("OTEL_SERVICE_NAME", "custom")]);
        assert_eq!(
            resolve(&TelemetryConfig::default(), "x", &env).service_name,
            "custom"
        );
    }

    #[test]
    fn partial_config_json_fills_defaults() {
        let cfg: TelemetryConfig =
            serde_json::from_str(r#"{"otlp":{"endpoint":"http://localhost:4318"}}"#).unwrap();
        assert_eq!(cfg.otlp.timeout_ms, 10_000);
        assert_eq!(cfg.log_retention_days, DEFAULT_LOG_RETENTION_DAYS);
        assert!(cfg.otlp.signals.traces);
    }
}
