//! Keep secrets out of logs: [`url`] strips credentials and query strings,
//! [`Redacted`] hides a value from `Debug` output.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// `scheme://host[:port]/path` of `raw` — userinfo, query and fragment
/// removed, since they commonly carry tokens. Input that doesn't look like a
/// URL is returned as `<invalid url>` rather than echoed.
pub fn url(raw: &str) -> String {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return "<invalid url>".to_string();
    };
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let (authority, path) = match rest.find('/') {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    format!("{scheme}://{host}{path}")
}

/// `s` cut to at most `max` characters (with `…` and the original length),
/// for logging strings an untrusted peer controls — an FDC3 message `type`,
/// a self-asserted `appId` — so one frame can't write megabytes to the log.
pub fn clip(s: &str, max: usize) -> std::borrow::Cow<'_, str> {
    match s.char_indices().nth(max) {
        None => std::borrow::Cow::Borrowed(s),
        Some((cut, _)) => std::borrow::Cow::Owned(format!("{}… ({} bytes)", &s[..cut], s.len())),
    }
}

/// A value that serializes normally (config files round-trip) but never
/// prints: `Debug` shows only how many entries it holds. Serialized output is
/// **not** protected — never send a struct holding one to a frontend.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Redacted<T>(pub T);

impl<T: RedactedLen> fmt::Debug for Redacted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<redacted: {} set>", self.0.redacted_len())
    }
}

impl<T: Serialize> Serialize for Redacted<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Redacted<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(d).map(Redacted)
    }
}

/// Size shown by `Redacted`'s `Debug`.
pub trait RedactedLen {
    fn redacted_len(&self) -> usize;
}

impl<K, V> RedactedLen for std::collections::BTreeMap<K, V> {
    fn redacted_len(&self) -> usize {
        self.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn url_drops_userinfo_query_and_fragment() {
        assert_eq!(
            url("https://user:secret@collector.example:4318/v1/traces?token=abc#x"),
            "https://collector.example:4318/v1/traces"
        );
        assert_eq!(url("http://localhost:4318"), "http://localhost:4318");
        assert_eq!(url("not a url"), "<invalid url>");
    }

    #[test]
    fn clip_bounds_untrusted_strings() {
        assert_eq!(clip("short", 8), "short");
        let long = "x".repeat(1_000_000);
        let clipped = clip(&long, 8);
        assert_eq!(clipped, "xxxxxxxx… (1000000 bytes)");
        assert_eq!(
            clip("ééééé", 2),
            "éé… (10 bytes)",
            "cuts on char boundaries"
        );
    }

    #[test]
    fn redacted_debug_hides_values_but_serializes_them() {
        let mut headers = BTreeMap::new();
        headers.insert("authorization".to_string(), "Bearer s3cret".to_string());
        let r = Redacted(headers);
        let dbg = format!("{r:?}");
        assert!(!dbg.contains("s3cret"), "{dbg}");
        assert_eq!(dbg, "<redacted: 1 set>");
        assert!(serde_json::to_string(&r).unwrap().contains("s3cret"));
    }
}
