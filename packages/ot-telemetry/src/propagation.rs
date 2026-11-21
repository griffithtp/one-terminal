//! W3C trace context between processes, following OpenTelemetry's environment
//! carrier: a parent sets `TRACEPARENT` (and `TRACESTATE`) on the child it
//! spawns, and the child reads them once at startup.
//!
//! The parser has no OpenTelemetry dependency, so builds without the `otlp`
//! feature (`tauri-webview-host`) can still stamp the parent's trace id on
//! their log lines.

use std::process::Command;
use std::sync::OnceLock;

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

/// Prepare `cmd`'s environment for a child process: set `TRACEPARENT` to the
/// current span (so the child's startup joins this trace) and clear the
/// variables a child must not inherit verbatim — our own `TRACEPARENT`/
/// `TRACESTATE` and `OTEL_SERVICE_NAME`, which would rename every child.
pub fn inject_env(cmd: &mut Command) {
    cmd.env_remove(TRACEPARENT)
        .env_remove(TRACESTATE)
        .env_remove("OTEL_SERVICE_NAME");
    if let Some(parent) = current() {
        cmd.env(TRACEPARENT, parent.header_value());
    }
}

/// The same variables as [`inject_env`], for spawn helpers that take an env
/// list instead of a `Command` (`ot_core::electron_host`).
pub fn child_env() -> Vec<(String, String)> {
    current()
        .map(|p| vec![(TRACEPARENT.to_string(), p.header_value())])
        .unwrap_or_default()
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

    #[test]
    fn inject_env_clears_inherited_variables_outside_a_span() {
        let mut cmd = Command::new("true");
        cmd.env(TRACEPARENT, VALID).env("OTEL_SERVICE_NAME", "x");
        inject_env(&mut cmd);
        let set: Vec<_> = cmd.get_envs().collect();
        assert!(set.contains(&(std::ffi::OsStr::new(TRACEPARENT), None)));
        assert!(set.contains(&(std::ffi::OsStr::new("OTEL_SERVICE_NAME"), None)));
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
            let mut cmd = Command::new("true");
            inject_env(&mut cmd);
            let value = cmd
                .get_envs()
                .find(|(k, _)| *k == TRACEPARENT)
                .and_then(|(_, v)| v)
                .map(|v| v.to_string_lossy().into_owned())
                .expect("TRACEPARENT set inside a span");
            let tp = TraceParent::parse(&value).expect("valid traceparent");
            assert_eq!(
                child_env(),
                vec![(TRACEPARENT.to_string(), tp.header_value())]
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
