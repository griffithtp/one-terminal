//! Trace-context helpers for application code (feature `otlp`), so callers
//! work with plain trace-id strings and never touch OpenTelemetry types.
//!
//! Externally supplied ids (an FDC3 `traceId` from a bus peer) are never
//! adopted as a span parent: a peer could pick ids to bypass sampling or graft
//! spans into other apps' traces. Record them as an attribute instead.

use opentelemetry::trace::TraceContextExt;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// The trace id of `span` as 32 lowercase hex characters, when it carries a
/// valid OpenTelemetry context (always, once `init` has run — even with export
/// off, spans get ids).
pub fn trace_id(span: &tracing::Span) -> Option<String> {
    let cx = span.context();
    let otel_span = cx.span();
    let sc = otel_span.span_context();
    sc.is_valid().then(|| sc.trace_id().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_id_is_32_hex_inside_an_otel_span() {
        use opentelemetry::trace::TracerProvider as _;
        use tracing_subscriber::layer::SubscriberExt;
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("t")));
        tracing::subscriber::with_default(subscriber, || {
            let id = trace_id(&tracing::info_span!("x")).unwrap();
            assert_eq!(id.len(), 32);
            assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
        });
    }

    #[test]
    fn no_subscriber_gives_no_trace_id() {
        assert_eq!(trace_id(&tracing::info_span!("x")), None);
    }
}
