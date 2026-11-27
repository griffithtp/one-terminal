//! Who may connect to the bus, and as which app.
//!
//! Browsers don't apply CORS to WebSockets, so without a check any web page
//! open in the user's ordinary browser could connect to `ws://127.0.0.1:7891`,
//! claim any `appId` and broadcast, raise intents or trigger launches. A
//! browser always sends the page's `Origin` on the upgrade request and a page
//! can't change it, so identity is bound to it, much as the FDC3 Web
//! Connection Protocol validates an app's identity URL against the App
//! Directory:
//!
//! - **No `Origin` header**: a non-browser client (a native app, the Node
//!   contract tests). Only a local process can be one: admitted with its
//!   claimed `appId`, as before.
//! - **A loopback origin** (`localhost`, `127.0.0.0/8`, `[::1]`): only a
//!   process on this machine can serve one (dev servers, the bundled sample
//!   widget, the seeded App Directory apps). Admitted with its claimed
//!   `appId`.
//! - **Any other origin**: must be the origin of a `details.url` in the App
//!   Directory, and the claimed `appId` must be one of the apps at that
//!   origin. Otherwise the upgrade is refused (`403`).
//! - **`Origin: null`** (sandboxed iframes, `file:` pages) and unparsable
//!   origins are refused: a remote page can produce them.
//!
//! The rule is deliberately not configurable: register a web app in the App
//! Directory to let it connect.

use std::net::IpAddr;

use ot_core::widget_url::Url;

use crate::app_dir::AppRecord;

/// How a connection was admitted (for logs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admitted {
    /// No `Origin` header: a non-browser local client.
    NoOrigin,
    /// A loopback origin.
    Loopback,
    /// The origin of the claimed app's App Directory URL.
    Directory,
}

/// Why a connection was refused. The `Display` text is logged and returned
/// as the `403` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// `null`, or not a `scheme://host[:port]` origin.
    BadOrigin(String),
    /// No App Directory app has a URL at this origin.
    UnknownOrigin(String),
    /// Apps are registered at this origin, but not the claimed one.
    AppIdNotAtOrigin { app_id: String, origin: String },
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadOrigin(o) => write!(f, "origin '{o}' is not allowed"),
            Self::UnknownOrigin(o) => write!(
                f,
                "origin '{o}' is not the origin of any app in the App Directory"
            ),
            Self::AppIdNotAtOrigin { app_id, origin } => {
                write!(f, "app '{app_id}' is not registered at origin '{origin}'")
            }
        }
    }
}

/// Decide whether a connection with `origin` (the raw header, if any) may
/// connect as `app_id`, given the current App Directory.
pub fn admit(
    origin: Option<&str>,
    app_id: &str,
    directory: &[AppRecord],
) -> Result<Admitted, Refused> {
    let Some(raw) = origin else {
        return Ok(Admitted::NoOrigin);
    };
    let raw = raw.trim();
    let origin = parse_origin(raw).ok_or_else(|| Refused::BadOrigin(raw.to_string()))?;
    if is_loopback(&origin) {
        return Ok(Admitted::Loopback);
    }
    let serialized = origin.origin().ascii_serialization();
    let mut any_at_origin = false;
    for app in directory {
        let Some(url) = app
            .details
            .as_ref()
            .and_then(|d| d.url.as_deref())
            .and_then(|u| Url::parse(u).ok())
        else {
            continue;
        };
        if url.origin().is_tuple() && url.origin().ascii_serialization() == serialized {
            if app.app_id == app_id {
                return Ok(Admitted::Directory);
            }
            any_at_origin = true;
        }
    }
    if any_at_origin {
        Err(Refused::AppIdNotAtOrigin {
            app_id: app_id.to_string(),
            origin: serialized,
        })
    } else {
        Err(Refused::UnknownOrigin(serialized))
    }
}

/// An `Origin` header value as a URL with a tuple origin (`http`, `https`,
/// `ws`, `wss`, or a custom scheme with a host), or `None` for `null` /
/// garbage.
fn parse_origin(raw: &str) -> Option<Url> {
    if raw.eq_ignore_ascii_case("null") {
        return None;
    }
    let url = Url::parse(raw).ok()?;
    url.origin().is_tuple().then_some(url)
}

fn is_loopback(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    // `host_str` brackets IPv6 literals (`[::1]`).
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_dir::AppDetails;

    fn app(id: &str, url: &str) -> AppRecord {
        AppRecord {
            app_id: id.into(),
            details: Some(AppDetails {
                url: Some(url.into()),
            }),
            ..Default::default()
        }
    }

    fn dir() -> Vec<AppRecord> {
        vec![
            app("blotter", "https://apps.example.com/blotter/"),
            app("chart", "https://apps.example.com/chart/index.html"),
            app("news", "https://news.example.org"),
            app("no-url", ""),
        ]
    }

    #[test]
    fn non_browser_clients_are_admitted_as_local() {
        assert_eq!(admit(None, "anything", &dir()), Ok(Admitted::NoOrigin));
    }

    #[test]
    fn loopback_origins_are_admitted_with_any_app_id() {
        for o in [
            "http://localhost:3010",
            "http://LOCALHOST:5173",
            "http://127.0.0.1:41234",
            "http://127.9.9.9",
            "http://[::1]:3011",
            "https://localhost",
        ] {
            assert_eq!(
                admit(Some(o), "sample-widget", &dir()),
                Ok(Admitted::Loopback),
                "{o}"
            );
        }
    }

    #[test]
    fn a_registered_origin_is_admitted_as_its_own_app() {
        let d = dir();
        assert_eq!(
            admit(Some("https://apps.example.com"), "blotter", &d),
            Ok(Admitted::Directory)
        );
        // Two apps share the origin: either may connect.
        assert_eq!(
            admit(Some("https://apps.example.com"), "chart", &d),
            Ok(Admitted::Directory)
        );
        // Default port and case normalise.
        assert_eq!(
            admit(Some("HTTPS://NEWS.EXAMPLE.ORG:443"), "news", &d),
            Ok(Admitted::Directory)
        );
    }

    #[test]
    fn a_registered_origin_cannot_claim_another_apps_id() {
        assert_eq!(
            admit(Some("https://news.example.org"), "blotter", &dir()),
            Err(Refused::AppIdNotAtOrigin {
                app_id: "blotter".into(),
                origin: "https://news.example.org".into()
            })
        );
    }

    #[test]
    fn unknown_and_opaque_origins_are_refused() {
        let d = dir();
        assert!(matches!(
            admit(Some("https://evil.example"), "blotter", &d),
            Err(Refused::UnknownOrigin(_))
        ));
        // Same host, different port or scheme: a different origin.
        assert!(matches!(
            admit(Some("https://apps.example.com:8443"), "blotter", &d),
            Err(Refused::UnknownOrigin(_))
        ));
        assert!(matches!(
            admit(Some("http://apps.example.com"), "blotter", &d),
            Err(Refused::UnknownOrigin(_))
        ));
        for o in ["null", "NULL", "", "not an origin", "file:///x"] {
            assert!(
                matches!(admit(Some(o), "blotter", &d), Err(Refused::BadOrigin(_))),
                "{o}"
            );
        }
    }

    #[test]
    fn lookalike_hosts_are_not_loopback() {
        for o in [
            "http://localhost.evil.example",
            "http://evil.localhost",
            "http://127.0.0.1.evil.example",
            "http://tauri.localhost",
            "tauri://localhost",
        ] {
            assert!(admit(Some(o), "x", &dir()).is_err(), "{o}");
        }
    }

    #[test]
    fn refusals_explain_themselves() {
        let e = admit(Some("https://evil.example"), "x", &dir()).unwrap_err();
        assert!(e.to_string().contains("not the origin of any app"), "{e}");
    }
}
