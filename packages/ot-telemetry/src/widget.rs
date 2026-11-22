//! What a widget window receives so its page can send logs to the widget
//! endpoint ([`crate::ingress`]): `window.OT_TELEMETRY = { url, token }`.
//!
//! No network code and no `ingress` feature needed: engine hosts
//! (`tauri-webview-host`) only read the two variables their parent set and
//! inject the script.

/// Endpoint URL a host passes to its widget page (set by the spawning process).
pub const URL_ENV: &str = "OT_TELEMETRY_URL";
/// The host window's token (set by the spawning process).
pub const TOKEN_ENV: &str = "OT_TELEMETRY_TOKEN";

/// Longest token accepted from the environment or a request.
pub const MAX_TOKEN_CHARS: usize = 2_048;

/// The endpoint and token for one widget window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WidgetTelemetry {
    pub url: String,
    pub token: String,
}

impl WidgetTelemetry {
    /// Read [`URL_ENV`] and [`TOKEN_ENV`]. `None` unless both are set and
    /// well-formed: a loopback `http` URL and a token of hex digits and dots.
    pub fn from_env(var: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        let url = var(URL_ENV)?;
        let token = var(TOKEN_ENV)?;
        let loopback = url
            .strip_prefix("http://127.0.0.1:")
            .is_some_and(|rest| rest.bytes().all(|b| b.is_ascii_graphic()));
        let token_ok = !token.is_empty()
            && token.len() <= MAX_TOKEN_CHARS
            && token.bytes().all(|b| b.is_ascii_hexdigit() || b == b'.');
        (loopback && token_ok).then_some(Self { url, token })
    }

    /// The initialization script that defines `window.OT_TELEMETRY`.
    pub fn init_script(&self) -> String {
        let value = serde_json::json!({ "url": self.url, "token": self.token });
        format!("window.OT_TELEMETRY = Object.freeze({value});")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn reads_well_formed_settings_only() {
        let read = |url: &str, token: &str| {
            WidgetTelemetry::from_env(&env(&[(URL_ENV, url), (TOKEN_ENV, token)]))
        };
        assert!(read("http://127.0.0.1:5123/v1/records", "7b22.ab").is_some());
        assert!(read("http://127.0.0.1:5123/v1/records", "7b22@").is_none());
        assert!(read("http://127.0.0.1:5123/v1/records", "").is_none());
        assert!(read("https://collector.example/v1/records", "ab.cd").is_none());
        assert!(read("http://localhost:5123/v1/records", "ab.cd").is_none());
        assert!(read("http://127.0.0.1:5 123/", "ab.cd").is_none());
        assert_eq!(WidgetTelemetry::from_env(&env(&[(TOKEN_ENV, "ab")])), None);
    }

    #[test]
    fn script_encodes_values_as_json() {
        let w = WidgetTelemetry {
            url: "http://127.0.0.1:1/\"});alert(1)//".into(),
            token: "ab.cd".into(),
        };
        assert!(w
            .init_script()
            .contains(r#""url":"http://127.0.0.1:1/\"});alert(1)//""#));
    }

    #[test]
    fn script_defines_a_frozen_global() {
        let w = WidgetTelemetry {
            url: "http://127.0.0.1:1/v1/records".into(),
            token: "ab.cd".into(),
        };
        assert_eq!(
            w.init_script(),
            r#"window.OT_TELEMETRY = Object.freeze({"token":"ab.cd","url":"http://127.0.0.1:1/v1/records"});"#
        );
    }
}
