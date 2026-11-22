//! Log records relayed from browser code — the Terminal and Desktop Agent UIs
//! (over IPC) and, later, widgets (over the local telemetry endpoint). This is
//! the only copy of the rules that make such input safe to log:
//!
//! - **Identity comes from the caller**, never the payload: the IPC command
//!   passes the webview label, the endpoint the verified token's claims.
//! - **Bounded:** at most [`MAX_RECORDS`] per batch; messages, stacks and
//!   attributes truncated; attribute keys checked; control characters removed.
//! - **Rate-limited** per sender with a token bucket.
//! - **Accounted:** dropped records are counted (`ot.telemetry.relay.dropped`)
//!   and warned about at most once per sender per minute.
//!
//! A record's `traceId` (for a widget, the FDC3 `metadata.traceId` its handler
//! received) is recorded as `fdc3.trace_id` — never adopted as a span parent,
//! since the sender chooses it.
//!
//! Records are emitted as `tracing` events with target `ot_relay::ui` or
//! `ot_relay::widget`, so they reach the log file and (when exporting) OTLP
//! logs like any other event.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Records accepted per batch; the rest are dropped.
pub const MAX_RECORDS: usize = 20;
/// Longest message kept, in characters.
pub const MAX_MESSAGE_CHARS: usize = 2_048;
/// Longest stack trace kept, in characters.
pub const MAX_STACK_CHARS: usize = 8_192;
/// Longest error name, client timestamp or trace id kept, in characters.
pub const MAX_SHORT_CHARS: usize = 128;
/// Attributes kept per record; the rest are dropped.
pub const MAX_ATTRIBUTES: usize = 8;
/// Longest attribute key / string value, in characters.
pub const MAX_ATTRIBUTE_KEY_CHARS: usize = 64;
pub const MAX_ATTRIBUTE_VALUE_CHARS: usize = 256;
/// Token bucket per sender: sustained rate and burst.
pub const RECORDS_PER_SECOND: f64 = 10.0;
pub const BURST: f64 = 50.0;
/// Senders tracked at once; beyond this the oldest buckets are forgotten.
const MAX_SENDERS: usize = 1_024;
/// At most one "records dropped" warning per sender per this interval.
const WARN_INTERVAL: Duration = Duration::from_secs(60);

// ── Wire types ────────────────────────────────────────────────────────────────

/// What browser code sends: `{ "records": [ … ] }`.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordBatch {
    pub records: Vec<Record>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub level: Level,
    pub message: String,
    /// Client clock, ISO 8601; informational only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// An FDC3 `metadata.traceId` the record relates to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<BTreeMap<String, AttributeValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum AttributeValue {
    Bool(bool),
    Number(f64),
    String(String),
    /// `null`, arrays, objects: dropped by the relay, so one bad attribute
    /// doesn't reject the whole batch.
    #[serde(skip_serializing)]
    Unsupported(serde::de::IgnoredAny),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

// ── Sender identity ───────────────────────────────────────────────────────────

/// Where relayed records come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    /// A framework UI webview (Terminal chrome/overlay/popout, Desktop Agent
    /// dashboard).
    Ui,
    /// A widget page.
    Widget,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Ui => "ui",
            Origin::Widget => "widget",
        }
    }
}

/// Who sent a batch, established by the caller (webview label, token claims)
/// — never read from the payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SenderIdentity {
    pub origin: Origin,
    /// Window or webview label.
    pub window: String,
    pub app_id: Option<String>,
    pub instance_id: Option<String>,
}

impl SenderIdentity {
    /// A framework UI webview.
    pub fn ui(window: impl Into<String>) -> Self {
        Self {
            origin: Origin::Ui,
            window: window.into(),
            app_id: None,
            instance_id: None,
        }
    }

    /// Bucket key. A tuple, not a joined string, so distinct identities can't
    /// collide (`"a|b" + ""` vs `"a" + "b|"`).
    fn key(&self) -> SenderKey {
        (
            self.origin,
            self.window.clone(),
            self.instance_id.clone().unwrap_or_default(),
        )
    }
}

// ── Relay ─────────────────────────────────────────────────────────────────────

/// Why a record was dropped (the `reason` attribute of the drop counter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropReason {
    /// More than [`MAX_RECORDS`] in one batch.
    BatchTooLarge,
    /// The sender's token bucket was empty.
    RateLimited,
}

impl DropReason {
    pub fn as_str(self) -> &'static str {
        match self {
            DropReason::BatchTooLarge => "batch_too_large",
            DropReason::RateLimited => "rate_limited",
        }
    }
}

/// What [`Relay::ingest`] did with a batch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IngestOutcome {
    pub accepted: usize,
    pub dropped: usize,
}

type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;
type SenderKey = (Origin, String, String);

struct Bucket {
    tokens: f64,
    refilled: Instant,
    last_warned: Option<Instant>,
    last_used: Instant,
}

/// Validates, limits and emits relayed records. One per process; share it.
pub struct Relay {
    clock: Clock,
    buckets: Mutex<HashMap<SenderKey, Bucket>>,
    #[cfg(feature = "otlp")]
    dropped: crate::metrics::Counter<u64>,
}

impl Default for Relay {
    fn default() -> Self {
        Self::new()
    }
}

impl Relay {
    pub fn new() -> Self {
        Self::with_clock(Arc::new(Instant::now))
    }

    /// A relay reading time from `clock` (tests advance it by hand).
    pub fn with_clock(clock: Clock) -> Self {
        Self {
            clock,
            buckets: Mutex::new(HashMap::new()),
            #[cfg(feature = "otlp")]
            dropped: drop_counter(&crate::metrics::meter("ot-telemetry")),
        }
    }

    /// A relay recording its drop counter on `meter` (tests).
    #[cfg(feature = "otlp")]
    pub fn with_meter(meter: &crate::metrics::Meter) -> Self {
        Self {
            dropped: drop_counter(meter),
            ..Self::new()
        }
    }

    /// Emit the records of `batch` from `sender` that the limits allow.
    pub fn ingest(&self, sender: &SenderIdentity, batch: RecordBatch) -> IngestOutcome {
        let mut outcome = IngestOutcome::default();
        let mut records = batch.records;
        if records.is_empty() {
            return outcome;
        }
        if records.len() > MAX_RECORDS {
            let extra = records.len() - MAX_RECORDS;
            records.truncate(MAX_RECORDS);
            self.count_dropped(sender, DropReason::BatchTooLarge, extra);
            outcome.dropped += extra;
        }
        let over_batch = outcome.dropped > 0;
        let (allowed, warn_due) = self.take_tokens(sender, records.len(), over_batch);
        if allowed < records.len() {
            let extra = records.len() - allowed;
            records.truncate(allowed);
            self.count_dropped(sender, DropReason::RateLimited, extra);
            outcome.dropped += extra;
        }
        for record in records {
            emit(sender, &sanitize(record));
            outcome.accepted += 1;
        }
        if warn_due && outcome.dropped > 0 {
            let origin = sender.origin.as_str();
            let window = clip(&sender.window, MAX_SHORT_CHARS);
            let dropped = outcome.dropped;
            // `event!` rather than `warn!`: `warn!(target: …, ot.origin = …)`
            // trips a macro-parsing ambiguity on the dotted field name.
            tracing::event!(
                target: "telemetry",
                tracing::Level::WARN,
                ot.origin = origin,
                ot.window = %window,
                "dropped {dropped} relayed log record(s): over the batch or rate limit"
            );
        }
        outcome
    }

    /// Take up to `wanted` tokens from the sender's bucket, in one critical
    /// section with the warning throttle. Returns the tokens granted and
    /// whether a drop warning is due (only consumed when something is, or
    /// will be, dropped: `already_dropping`, or fewer tokens than wanted).
    fn take_tokens(
        &self,
        sender: &SenderIdentity,
        wanted: usize,
        already_dropping: bool,
    ) -> (usize, bool) {
        let now = (self.clock)();
        let key = sender.key();
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if buckets.len() >= MAX_SENDERS && !buckets.contains_key(&key) {
            // Forget the least recently used sender: bounded memory whatever
            // the number of senders. (Senders are webview labels or verified
            // token identities, so a peer can't mint keys to cycle buckets.)
            if let Some(oldest) = buckets
                .iter()
                .min_by_key(|(_, b)| b.last_used)
                .map(|(k, _)| k.clone())
            {
                buckets.remove(&oldest);
            }
        }
        let bucket = buckets.entry(key).or_insert(Bucket {
            tokens: BURST,
            refilled: now,
            last_warned: None,
            last_used: now,
        });
        let elapsed = now.saturating_duration_since(bucket.refilled).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * RECORDS_PER_SECOND).min(BURST);
        bucket.refilled = now;
        bucket.last_used = now;
        // Whole tokens only; at most `wanted` (small: ≤ MAX_RECORDS).
        let granted = bucket.tokens.floor().clamp(0.0, wanted as f64) as usize;
        bucket.tokens -= granted as f64;
        let dropping = already_dropping || granted < wanted;
        let warn_due = dropping
            && bucket
                .last_warned
                .is_none_or(|t| now.saturating_duration_since(t) >= WARN_INTERVAL);
        if warn_due {
            bucket.last_warned = Some(now);
        }
        (granted, warn_due)
    }

    #[cfg_attr(not(feature = "otlp"), allow(unused_variables))]
    fn count_dropped(&self, sender: &SenderIdentity, reason: DropReason, n: usize) {
        #[cfg(feature = "otlp")]
        self.dropped.add(
            n as u64,
            &[
                crate::metrics::KeyValue::new("reason", reason.as_str()),
                crate::metrics::KeyValue::new("origin", sender.origin.as_str()),
            ],
        );
    }
}

#[cfg(feature = "otlp")]
fn drop_counter(meter: &crate::metrics::Meter) -> crate::metrics::Counter<u64> {
    meter
        .u64_counter("ot.telemetry.relay.dropped")
        .with_description("Relayed log records dropped, by reason and origin")
        .build()
}

// ── Sanitising ────────────────────────────────────────────────────────────────

/// A record cut to the limits, ready to log.
#[derive(Debug, PartialEq)]
struct CleanRecord {
    level: Level,
    message: String,
    timestamp: Option<String>,
    trace_id: Option<String>,
    /// Kept attributes as a JSON object string (tracing fields are static, so
    /// arbitrary keys travel as one field).
    attributes: Option<String>,
    error: Option<(String, String, Option<String>)>,
}

fn sanitize(record: Record) -> CleanRecord {
    let attributes = record.attributes.and_then(|attrs| {
        let kept: serde_json::Map<String, serde_json::Value> = attrs
            .into_iter()
            .filter(|(k, v)| valid_attribute_key(k) && !matches!(v, AttributeValue::Unsupported(_)))
            .take(MAX_ATTRIBUTES)
            .filter_map(|(k, v)| {
                let v = match v {
                    AttributeValue::Bool(b) => serde_json::Value::Bool(b),
                    AttributeValue::Number(n) => serde_json::json!(n),
                    AttributeValue::String(s) => {
                        serde_json::Value::String(clean(&s, MAX_ATTRIBUTE_VALUE_CHARS, false))
                    }
                    AttributeValue::Unsupported(_) => return None,
                };
                Some((k, v))
            })
            .collect();
        (!kept.is_empty()).then(|| serde_json::Value::Object(kept).to_string())
    });
    CleanRecord {
        level: record.level,
        message: clean(&record.message, MAX_MESSAGE_CHARS, false),
        timestamp: record.timestamp.map(|t| clean(&t, MAX_SHORT_CHARS, false)),
        trace_id: record.trace_id.map(|t| clean(&t, MAX_SHORT_CHARS, false)),
        attributes,
        error: record.error.map(|e| {
            (
                clean(&e.name, MAX_SHORT_CHARS, false),
                clean(&e.message, MAX_MESSAGE_CHARS, false),
                e.stack.map(|s| clean(&s, MAX_STACK_CHARS, true)),
            )
        }),
    }
}

/// `^[a-z0-9_.]{1,64}$`
fn valid_attribute_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_ATTRIBUTE_KEY_CHARS
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}

/// `s` without control or invisible formatting characters, with URL
/// credentials and query strings removed, cut to `max` characters. Bounded
/// work: characters past the limit are never visited, however long `s` is.
///
/// With `keep_newlines` (stack traces) `\n` is kept, and every continuation
/// line is indented, so a crafted stack can't start a fake log line in
/// plain-text output (the JSON log file escapes newlines anyway).
fn clean(s: &str, max: usize, keep_newlines: bool) -> String {
    let mut out = String::new();
    let mut kept = 0;
    for c in s.chars() {
        if (c.is_control() && !(keep_newlines && c == '\n')) || is_invisible_format(c) {
            continue;
        }
        if kept == max {
            out.push('…');
            break;
        }
        out.push(c);
        if c == '\n' {
            out.push_str(CONTINUATION_INDENT);
        }
        kept += 1;
    }
    scrub_urls(&out)
}

/// Bidi overrides and isolates, zero-width characters, soft hyphen, BOM and
/// the Unicode line/paragraph separators: invisible characters that can
/// disguise text in log viewers and terminals.
fn is_invisible_format(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
    )
}

/// Replace every `scheme://…` in `s` with [`crate::redact::url`] of it —
/// userinfo, query and fragment removed — so presigned or token-bearing URLs
/// in relayed text never reach the log file or the collector.
fn scrub_urls(s: &str) -> String {
    let is_scheme = |c: char| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.');
    let ends_url = |c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '`' | ','
            )
    };
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(sep) = rest.find("://") {
        let scheme_start = rest[..sep]
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_scheme(*c))
            .last()
            .map_or(sep, |(i, _)| i);
        let end = rest[sep + 3..]
            .find(ends_url)
            .map_or(rest.len(), |i| sep + 3 + i);
        out.push_str(&rest[..scheme_start]);
        if scheme_start == sep {
            out.push_str("://"); // no scheme: not a URL
        } else {
            out.push_str(&crate::redact::url(&rest[scheme_start..end]));
        }
        rest = &rest[if scheme_start == sep { sep + 3 } else { end }..];
    }
    out.push_str(rest);
    out
}

/// Prefix for stack-trace continuation lines (see [`clean`]).
const CONTINUATION_INDENT: &str = "    ";

fn clip(s: &str, max: usize) -> std::borrow::Cow<'_, str> {
    match s.char_indices().nth(max) {
        None => std::borrow::Cow::Borrowed(s),
        Some((cut, _)) => std::borrow::Cow::Owned(format!("{}…", &s[..cut])),
    }
}

// ── Emitting ──────────────────────────────────────────────────────────────────

/// One `tracing` event per record. Targets and levels must be static, hence
/// the match.
fn emit(sender: &SenderIdentity, r: &CleanRecord) {
    let window = clip(&sender.window, MAX_SHORT_CHARS);
    let app_id = sender.app_id.as_deref().map(|a| clip(a, MAX_SHORT_CHARS));
    let instance_id = sender
        .instance_id
        .as_deref()
        .map(|i| clip(i, MAX_SHORT_CHARS));
    let origin = sender.origin.as_str();
    let trace_id = r.trace_id.as_deref();
    let timestamp = r.timestamp.as_deref();
    let attributes = r.attributes.as_deref();
    let message = r.message.as_str();
    let (error_type, error_message, stack) = match &r.error {
        Some((name, message, stack)) => (
            Some(name.as_str()),
            Some(message.as_str()),
            stack.as_deref(),
        ),
        None => (None, None, None),
    };
    macro_rules! event {
        ($level:expr, $target:expr) => {
            tracing::event!(
                target: $target,
                $level,
                ot.origin = origin,
                ot.window = %window,
                ot.app_id = app_id.as_deref(),
                ot.instance_id = instance_id.as_deref(),
                fdc3.trace_id = trace_id,
                client.timestamp = timestamp,
                attributes = attributes,
                exception.type = error_type,
                exception.message = error_message,
                exception.stacktrace = stack,
                "{}",
                message
            )
        };
    }
    use tracing::Level as L;
    match (sender.origin, r.level) {
        (Origin::Ui, Level::Error) => event!(L::ERROR, "ot_relay::ui"),
        (Origin::Ui, Level::Warn) => event!(L::WARN, "ot_relay::ui"),
        (Origin::Ui, Level::Info) => event!(L::INFO, "ot_relay::ui"),
        (Origin::Ui, Level::Debug) => event!(L::DEBUG, "ot_relay::ui"),
        (Origin::Widget, Level::Error) => event!(L::ERROR, "ot_relay::widget"),
        (Origin::Widget, Level::Warn) => event!(L::WARN, "ot_relay::widget"),
        (Origin::Widget, Level::Info) => event!(L::INFO, "ot_relay::widget"),
        (Origin::Widget, Level::Debug) => event!(L::DEBUG, "ot_relay::widget"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn record(message: &str) -> Record {
        Record {
            level: Level::Info,
            message: message.to_string(),
            timestamp: None,
            trace_id: None,
            attributes: None,
            error: None,
        }
    }

    fn batch(n: usize) -> RecordBatch {
        RecordBatch {
            records: (0..n).map(|i| record(&format!("r{i}"))).collect(),
        }
    }

    /// A clock tests move by hand, in milliseconds.
    fn manual_clock() -> (Clock, Arc<AtomicU64>) {
        let start = Instant::now();
        let ms = Arc::new(AtomicU64::new(0));
        let read = Arc::clone(&ms);
        (
            Arc::new(move || start + Duration::from_millis(read.load(Ordering::SeqCst))),
            ms,
        )
    }

    #[test]
    fn warns_about_drops_at_most_once_a_minute() {
        let (clock, ms) = manual_clock();
        let relay = Relay::with_clock(clock);
        let a = SenderIdentity::ui("a");
        let warned = |relay: &Relay| relay.take_tokens(&a, 5, true).1;
        assert!(warned(&relay), "first drop warns");
        assert!(!warned(&relay), "throttled");
        ms.store(59_000, Ordering::SeqCst);
        assert!(!warned(&relay));
        ms.store(60_000, Ordering::SeqCst);
        assert!(warned(&relay), "after a minute");
        assert!(
            !relay.take_tokens(&SenderIdentity::ui("b"), 1, false).1,
            "no drop, no warning"
        );
    }

    #[test]
    fn carries_fractional_tokens_over() {
        let (clock, ms) = manual_clock();
        let relay = Relay::with_clock(clock);
        let a = SenderIdentity::ui("a");
        assert_eq!(relay.take_tokens(&a, 50, false).0, 50);
        ms.store(150, Ordering::SeqCst); // +1.5 tokens
        assert_eq!(relay.take_tokens(&a, 5, false).0, 1);
        ms.store(200, Ordering::SeqCst); // +0.5 → 1.0
        assert_eq!(relay.take_tokens(&a, 5, false).0, 1);
    }

    #[test]
    fn evicts_the_least_recently_used_sender() {
        let (clock, ms) = manual_clock();
        let relay = Relay::with_clock(clock);
        for i in 0..MAX_SENDERS {
            ms.store(i as u64, Ordering::SeqCst);
            relay.ingest(&SenderIdentity::ui(format!("w{i}")), batch(1));
        }
        ms.store(5_000, Ordering::SeqCst);
        relay.ingest(&SenderIdentity::ui("w0"), batch(1)); // w0 is recent again
        relay.ingest(&SenderIdentity::ui("new"), batch(1));
        let buckets = relay.buckets.lock().unwrap();
        assert!(buckets.contains_key(&SenderIdentity::ui("w0").key()));
        assert!(
            !buckets.contains_key(&SenderIdentity::ui("w1").key()),
            "w1 was oldest"
        );
    }

    #[test]
    fn empty_batches_take_no_sender_slot() {
        let relay = Relay::new();
        relay.ingest(&SenderIdentity::ui("w"), RecordBatch::default());
        assert!(relay.buckets.lock().unwrap().is_empty());
    }

    #[test]
    fn scrubs_url_credentials_and_queries() {
        assert_eq!(
            clean(
                "fetch failed: https://u:p@h.example/p?sig=abc (timeout)",
                200,
                false
            ),
            "fetch failed: https://h.example/p (timeout)"
        );
        assert_eq!(
            clean("x \"wss://agent/fdc3?token=t\" y", 200, false),
            "x \"wss://agent/fdc3\" y"
        );
        assert_eq!(clean("no url :// here", 200, false), "no url :// here");
    }

    #[test]
    fn strips_invisible_formatting_characters() {
        assert_eq!(
            clean("ok\u{202E}gpj.exe\u{200B}\u{2028}", 200, false),
            "okgpj.exe"
        );
    }

    #[test]
    fn stack_continuation_lines_are_indented() {
        let cleaned = clean("Error: x\n2026-01-01 INFO fake line", 100, true);
        assert_eq!(cleaned, "Error: x\n    2026-01-01 INFO fake line");
        assert_eq!(clean(&"y".repeat(10), 3, false), "yyy…");
    }

    #[test]
    fn unsupported_attribute_values_are_dropped_not_fatal() {
        let b: RecordBatch = serde_json::from_str(
            r#"{"records":[{"level":"info","message":"m","attributes":{"ok":"v","nul":null,"obj":{"a":1}}}]}"#,
        )
        .unwrap();
        let r = sanitize(b.records.into_iter().next().unwrap());
        let attrs: serde_json::Value = serde_json::from_str(&r.attributes.unwrap()).unwrap();
        assert_eq!(attrs, serde_json::json!({ "ok": "v" }));
    }

    #[test]
    fn caps_records_per_batch() {
        let relay = Relay::new();
        let out = relay.ingest(&SenderIdentity::ui("w"), batch(MAX_RECORDS + 5));
        assert_eq!(
            out,
            IngestOutcome {
                accepted: MAX_RECORDS,
                dropped: 5
            }
        );
    }

    #[test]
    fn rate_limits_per_sender_and_refills() {
        let (clock, ms) = manual_clock();
        let relay = Relay::with_clock(clock);
        let a = SenderIdentity::ui("a");
        let mut accepted = 0;
        for _ in 0..5 {
            accepted += relay.ingest(&a, batch(MAX_RECORDS)).accepted;
        }
        assert_eq!(accepted, BURST as usize, "burst then empty");
        assert_eq!(
            relay.ingest(&SenderIdentity::ui("b"), batch(5)).accepted,
            5,
            "other senders unaffected"
        );
        ms.store(1_000, Ordering::SeqCst);
        assert_eq!(
            relay.ingest(&a, batch(MAX_RECORDS)).accepted,
            RECORDS_PER_SECOND as usize,
            "refilled at the sustained rate"
        );
    }

    #[test]
    fn bounds_tracked_senders() {
        let relay = Relay::new();
        for i in 0..MAX_SENDERS + 10 {
            relay.ingest(&SenderIdentity::ui(format!("w{i}")), batch(1));
        }
        assert!(relay.buckets.lock().unwrap().len() <= MAX_SENDERS);
    }

    #[test]
    fn sanitizes_untrusted_fields() {
        let mut attrs = BTreeMap::new();
        attrs.insert("ok.key_1".into(), AttributeValue::String("v".repeat(1_000)));
        attrs.insert("Bad Key".into(), AttributeValue::Bool(true));
        attrs.insert("n".into(), AttributeValue::Number(3.0));
        for i in 0..20 {
            attrs.insert(format!("k{i:02}"), AttributeValue::Bool(true));
        }
        let r = sanitize(Record {
            level: Level::Error,
            message: format!("line1\nline2\u{1b}[31m{}", "m".repeat(5_000)),
            timestamp: Some("t".repeat(1_000)),
            trace_id: Some("ffffffffffffffff0000000000000000".into()),
            attributes: Some(attrs),
            error: Some(ErrorInfo {
                name: "TypeError".into(),
                message: "boom".into(),
                stack: Some(format!("at a\nat b\u{0}{}", "s".repeat(20_000))),
            }),
        });
        assert!(!r.message.contains('\n') && !r.message.contains('\u{1b}'));
        assert!(r.message.chars().count() <= MAX_MESSAGE_CHARS + 1);
        assert!(r.timestamp.unwrap().chars().count() <= MAX_SHORT_CHARS + 1);
        let attrs: serde_json::Value = serde_json::from_str(&r.attributes.unwrap()).unwrap();
        let attrs = attrs.as_object().unwrap();
        assert!(attrs.len() <= MAX_ATTRIBUTES);
        assert!(!attrs.contains_key("Bad Key"));
        assert!(attrs.values().all(|v| v
            .as_str()
            .is_none_or(|s| s.chars().count() <= MAX_ATTRIBUTE_VALUE_CHARS + 1)));
        let (_, _, stack) = r.error.unwrap();
        let stack = stack.unwrap();
        assert!(
            stack.contains("at a\n    at b"),
            "newlines kept, continuation indented"
        );
        assert!(!stack.contains('\u{0}'));
        // One continuation line: its indent isn't counted against the cap.
        assert!(stack.chars().count() <= MAX_STACK_CHARS + 1 + CONTINUATION_INDENT.len());
    }

    #[test]
    fn parses_the_wire_format() {
        let b: RecordBatch = serde_json::from_str(
            r#"{"records":[{"level":"warn","message":"m","traceId":"t","attributes":{"a":1,"b":"x","c":true},"error":{"name":"E","message":"x","stack":"s"}}]}"#,
        )
        .unwrap();
        assert_eq!(b.records[0].level, Level::Warn);
        assert_eq!(
            b.records[0].attributes.as_ref().unwrap()["a"],
            AttributeValue::Number(1.0)
        );
        assert!(serde_json::from_str::<RecordBatch>(
            r#"{"records":[{"level":"fatal","message":"m"}]}"#
        )
        .is_err());
    }

    #[test]
    fn emits_one_event_per_record_with_the_callers_identity() {
        use std::io::Write;
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
        let buf = Buf::default();
        let out = buf.clone();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .event_format(crate::logs::JsonLines)
                .with_writer(move || out.clone()),
        );
        let sender = SenderIdentity {
            origin: Origin::Widget,
            window: "w-1".into(),
            app_id: Some("chart".into()),
            instance_id: Some("i-1".into()),
        };
        let mut b = batch(1);
        b.records[0].level = Level::Error;
        b.records[0].trace_id = Some("t-1".into());
        b.records[0].error = Some(ErrorInfo {
            name: "TypeError".into(),
            message: "boom".into(),
            stack: Some("at x".into()),
        });
        tracing::subscriber::with_default(subscriber, || {
            Relay::new().ingest(&sender, b);
        });
        let text = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        let line: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(line["target"], "ot_relay::widget");
        assert_eq!(line["level"], "ERROR");
        assert_eq!(line["message"], "r0");
        assert_eq!(line["ot.origin"], "widget");
        assert_eq!(line["ot.app_id"], "chart");
        assert_eq!(line["ot.window"], "w-1");
        assert_eq!(line["fdc3.trace_id"], "t-1");
        assert_eq!(line["exception.type"], "TypeError");
        assert_eq!(line["exception.stacktrace"], "at x");
    }

    #[cfg(feature = "testing")]
    #[test]
    fn counts_drops_by_reason_and_origin() {
        let capture = crate::testing::Capture::new();
        let relay = Relay::with_meter(&capture.meter("ot-telemetry"));
        relay.ingest(&SenderIdentity::ui("w"), batch(MAX_RECORDS + 3));
        for _ in 0..3 {
            relay.ingest(&SenderIdentity::ui("w"), batch(MAX_RECORDS));
        }
        let sum = |reason| {
            capture.sum(
                "ot.telemetry.relay.dropped",
                &[("reason", reason), ("origin", "ui")],
            )
        };
        assert!((sum("batch_too_large") - 3.0).abs() < f64::EPSILON);
        // 20 + 3×20 = 80 records against a burst of 50.
        assert!((sum("rate_limited") - 30.0).abs() < f64::EPSILON);
    }
}
