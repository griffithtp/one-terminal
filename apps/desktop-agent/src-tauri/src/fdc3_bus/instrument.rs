//! Spans and metrics for the FDC3 bus. Observes frames from the outside: the
//! broker knows nothing about telemetry.
//!
//! - Every request (except heartbeat acknowledgements) runs in a root
//!   `fdc3.request` span named after its message type.
//! - A request without an FDC3 `traceId` gets the span's trace id stamped in
//!   (`payload.metadata.traceId`), and the broker delivers it to receivers as
//!   `ContextMetadata.traceId`, so contexts and spans correlate.
//! - An app-supplied `traceId` passes through unchanged and is recorded as the
//!   `fdc3.trace_id` attribute. It is never adopted as the span's parent: any
//!   local page can connect to the bus, and a chosen id would let it bypass
//!   sampling or graft spans into another app's trace.

use crate::broker::Outcome;
use ot_telemetry::metrics::{Counter, Histogram, KeyValue, Meter, UpDownCounter};
use serde_json::Value;
use std::time::Instant;
use tracing::Span;

/// Message type attribute and span name for types the broker doesn't handle:
/// keeps both low-cardinality whatever a peer sends.
const OTHER: &str = "other";

/// Request types the broker handles (see `Broker::handle`). Anything else is
/// recorded as [`OTHER`].
const KNOWN_REQUESTS: [&str; 29] = [
    "addContextListenerRequest",
    "addEventListenerRequest",
    "addIntentListenerRequest",
    "broadcastRequest",
    "clearContextRequest",
    "closeRequest",
    "contextListenerUnsubscribeRequest",
    "createPrivateChannelRequest",
    "eventListenerUnsubscribeRequest",
    "findInstancesRequest",
    "findIntentRequest",
    "findIntentsByContextRequest",
    "getAppMetadataRequest",
    "getCurrentChannelRequest",
    "getCurrentContextRequest",
    "getInfoRequest",
    "getOrCreateChannelRequest",
    "getUserChannelsRequest",
    "heartbeatAcknowledgementRequest",
    "intentListenerUnsubscribeRequest",
    "intentResultRequest",
    "joinUserChannelRequest",
    "leaveCurrentChannelRequest",
    "openRequest",
    "privateChannelAddEventListenerRequest",
    "privateChannelDisconnectRequest",
    "privateChannelUnsubscribeEventListenerRequest",
    "raiseIntentForContextRequest",
    "raiseIntentRequest",
];

/// Requests not worth a span or a metric data point.
const UNTRACED: [&str; 1] = ["heartbeatAcknowledgementRequest"];

/// The bus's instruments, built once per server.
pub(super) struct BusMetrics {
    messages: Counter<u64>,
    duration: Histogram<f64>,
    connections: UpDownCounter<i64>,
    handshake_failures: Counter<u64>,
}

impl BusMetrics {
    pub fn new(meter: &Meter) -> Self {
        Self {
            messages: meter
                .u64_counter("ot.fdc3.messages")
                .with_description("FDC3 requests received, by message type")
                .build(),
            duration: meter
                .f64_histogram("ot.fdc3.message.duration")
                .with_unit("s")
                .with_description("Time the broker spent handling one FDC3 request")
                .build(),
            connections: meter
                .i64_up_down_counter("ot.fdc3.connections")
                .with_description("Connected FDC3 apps")
                .build(),
            handshake_failures: meter
                .u64_counter("ot.fdc3.handshake.failures")
                .with_description("Connections closed before a valid WCP4 handshake")
                .build(),
        }
    }

    pub fn connected(&self) {
        self.connections.add(1, &[]);
    }

    pub fn disconnected(&self) {
        self.connections.add(-1, &[]);
    }

    /// A connection that ended before a valid WCP4 handshake.
    pub fn handshake_failed(&self, reason: HandshakeEnd) {
        self.handshake_failures
            .add(1, &[KeyValue::new("reason", reason.as_str())]);
    }

    /// Record one handled request that started at `started`.
    pub fn handled(&self, label: &'static str, outcome: Outcome, started: Instant) {
        let message_type = KeyValue::new("fdc3.message.type", label);
        self.messages.add(
            1,
            &[
                message_type.clone(),
                KeyValue::new("outcome", outcome.as_str()),
            ],
        );
        self.duration
            .record(started.elapsed().as_secs_f64(), &[message_type]);
    }
}

/// How a connection's handshake phase ended without a valid WCP4.
#[derive(Clone, Copy, Debug)]
pub(super) enum HandshakeEnd {
    /// The peer closed (or the socket dropped) first.
    Closed,
    /// A non-text frame or a socket error.
    Invalid,
}

impl HandshakeEnd {
    fn as_str(self) -> &'static str {
        match self {
            HandshakeEnd::Closed => "closed",
            HandshakeEnd::Invalid => "invalid",
        }
    }
}

/// A root span for one connection's WCP4/WCP5 handshake. `app_id` is the
/// self-asserted `?appId=`, clipped. `ot.instance_id` and `outcome` are
/// recorded when the handshake ends.
pub(super) fn connect_span(app_id: &str) -> Span {
    tracing::info_span!(
        target: ot_telemetry::SPAN_TARGET,
        parent: None,
        "fdc3.connect",
        otel.kind = "server",
        ot.app_id = %crate::broker::clip_untrusted(app_id),
        ot.instance_id = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
}

/// Record how the handshake in `span` ended.
pub(super) fn handshake_ended(span: &Span, outcome: &str) {
    span.record("outcome", outcome);
}

/// The low-cardinality label for a message `type`, or `None` for types that
/// get no span or metric.
pub(super) fn message_label(msg_type: &str) -> Option<&'static str> {
    if UNTRACED.contains(&msg_type) {
        return None;
    }
    Some(
        KNOWN_REQUESTS
            .iter()
            .find(|known| **known == msg_type)
            .copied()
            .unwrap_or(OTHER),
    )
}

/// A root span for one request. `parent: None`: requests aren't children of
/// the hours-long connection.
pub(super) fn request_span(label: &'static str, msg: &Value, instance_id: &str) -> Span {
    let request_uuid = msg["meta"]["requestUuid"].as_str().unwrap_or_default();
    tracing::info_span!(
        target: ot_telemetry::SPAN_TARGET,
        parent: None,
        "fdc3.request",
        otel.name = label,
        otel.kind = "server",
        fdc3.message.type = label,
        fdc3.request_uuid = %crate::broker::clip_untrusted(request_uuid),
        ot.instance_id = %instance_id,
        fdc3.trace_id = tracing::field::Empty,
    )
}

/// Correlate the request's FDC3 trace id with `span` (see the module docs).
/// Leaves `msg` untouched when its payload isn't an object or the span has no
/// trace id (no telemetry subscriber, e.g. in broker tests).
pub(super) fn link_trace(span: &Span, msg: &mut Value) {
    // An app-supplied trace id passes through unchanged (FDC3) and is only
    // recorded — never joined (see the module docs).
    if let Some(supplied) = msg["payload"]["metadata"]["traceId"].as_str() {
        span.record(
            "fdc3.trace_id",
            crate::broker::clip_untrusted(supplied).as_ref(),
        );
        return;
    }
    let Some(trace_id) = ot_telemetry::trace::trace_id(span) else {
        return;
    };
    let Some(payload) = msg.get_mut("payload").and_then(Value::as_object_mut) else {
        return;
    };
    let metadata = payload
        .entry("metadata")
        .or_insert_with(|| Value::Object(Default::default()));
    if metadata.is_null() {
        *metadata = Value::Object(Default::default());
    }
    if let Some(metadata) = metadata.as_object_mut() {
        metadata.insert("traceId".into(), Value::String(trace_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ot_telemetry::testing::Capture;
    use serde_json::json;

    #[test]
    fn connect_span_records_identity_and_outcome() {
        let capture = Capture::new();
        capture.run(|| {
            let span = connect_span(&"a".repeat(10_000));
            span.record("ot.instance_id", "inst-1");
            handshake_ended(&span, "ok");
        });
        let span = &capture.spans()[0];
        assert_eq!(span.name, "fdc3.connect");
        assert_eq!(span.attributes["outcome"], "ok");
        assert_eq!(span.attributes["ot.instance_id"], "inst-1");
        assert!(span.attributes["ot.app_id"].len() < 200, "app id clipped");
    }

    #[test]
    fn labels_are_low_cardinality() {
        assert_eq!(message_label("broadcastRequest"), Some("broadcastRequest"));
        assert_eq!(message_label("x".repeat(10_000).as_str()), Some(OTHER));
        assert_eq!(message_label("heartbeatAcknowledgementRequest"), None);
    }

    #[test]
    fn stamps_the_span_trace_id_into_the_request() {
        let capture = Capture::new();
        let mut msg = json!({
            "type": "broadcastRequest",
            "meta": { "requestUuid": "r1" },
            "payload": { "channelId": "Red", "context": { "type": "fdc3.instrument" } },
        });
        let trace_id = capture.run(|| {
            let span = request_span("broadcastRequest", &msg, "inst-1");
            link_trace(&span, &mut msg);
            ot_telemetry::trace::trace_id(&span).unwrap()
        });
        assert_eq!(msg["payload"]["metadata"]["traceId"], trace_id);
        let spans = capture.spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "broadcastRequest");
        assert_eq!(spans[0].trace_id, trace_id);
        assert_eq!(spans[0].attributes["fdc3.request_uuid"], "r1");
    }

    #[test]
    fn records_but_never_joins_an_app_supplied_trace_id() {
        let capture = Capture::new();
        // A peer-chosen id: joining it would let the peer pick the sampling
        // decision and graft spans into another app's trace.
        let chosen = "ffffffffffffffff0000000000000000";
        let mut msg = json!({
            "type": "raiseIntentRequest",
            "payload": { "metadata": { "traceId": chosen } },
        });
        capture.run(|| {
            let span = request_span("raiseIntentRequest", &msg, "inst-1");
            link_trace(&span, &mut msg);
        });
        assert_eq!(
            msg["payload"]["metadata"]["traceId"], chosen,
            "app value kept"
        );
        let span = &capture.spans()[0];
        assert_ne!(span.trace_id, chosen, "own root trace");
        assert!(!span.parent_is_remote);
        assert_eq!(span.attributes["fdc3.trace_id"], chosen);
    }

    #[test]
    fn keeps_an_app_trace_id_it_cannot_join() {
        let capture = Capture::new();
        let mut msg = json!({ "payload": { "metadata": { "traceId": "trace-1" } } });
        capture.run(|| {
            let span = request_span("broadcastRequest", &msg, "inst-1");
            link_trace(&span, &mut msg);
        });
        assert_eq!(msg["payload"]["metadata"]["traceId"], "trace-1");
        assert!(!capture.spans()[0].parent_is_remote, "not joined");
    }

    #[test]
    fn no_subscriber_leaves_the_request_alone() {
        let mut msg = json!({ "type": "broadcastRequest", "payload": {} });
        let before = msg.clone();
        let span = request_span("broadcastRequest", &msg, "inst-1");
        link_trace(&span, &mut msg);
        assert_eq!(msg, before);
    }

    #[test]
    fn records_message_metrics() {
        let capture = Capture::new();
        let metrics = BusMetrics::new(&capture.meter("desktop-agent"));
        let sum = |attrs: &[(&str, &str)]| capture.sum("ot.fdc3.messages", attrs);
        metrics.connected();
        metrics.handled("broadcastRequest", Outcome::Ok, Instant::now());
        metrics.handled("broadcastRequest", Outcome::Error, Instant::now());
        metrics.handled(OTHER, Outcome::Unhandled, Instant::now());
        metrics.handshake_failed(HandshakeEnd::Closed);
        assert!((sum(&[("fdc3.message.type", "broadcastRequest")]) - 2.0).abs() < f64::EPSILON);
        assert!(
            (sum(&[
                ("fdc3.message.type", "broadcastRequest"),
                ("outcome", "error")
            ]) - 1.0)
                .abs()
                < f64::EPSILON
        );
        assert!((sum(&[("outcome", "unhandled")]) - 1.0).abs() < f64::EPSILON);
        assert_eq!(
            capture.histogram_count("ot.fdc3.message.duration", &[("fdc3.message.type", OTHER)]),
            1
        );
        assert!((capture.sum("ot.fdc3.connections", &[]) - 1.0).abs() < f64::EPSILON);
        assert!(
            (capture.sum("ot.fdc3.handshake.failures", &[("reason", "closed")]) - 1.0).abs()
                < f64::EPSILON
        );
    }
}
