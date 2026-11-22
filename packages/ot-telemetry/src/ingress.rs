//! The widget endpoint: a loopback HTTP listener that widget pages send log
//! records to, so widget errors reach this process's logs (and, when
//! exporting, OTLP) without going through the FDC3 bus.
//!
//! - Each process that hosts widgets runs its own endpoint on an ephemeral
//!   `127.0.0.1` port and records into its own [`Relay`].
//! - A request is accepted only with a token this process minted for one
//!   window: `hex(claims).hex(HMAC-SHA256(key, hex(claims)))`. The key is
//!   random per process and never leaves it — engine hosts receive a minted
//!   token, not the key. Tokens die with the process; there is no expiry.
//! - The sender identity (window label, app id) comes from the verified
//!   claims, never from the payload.
//! - The body is a `text/plain` JSON `{token, records}` capped at
//!   [`MAX_BODY_BYTES`] before parsing, so `fetch` and `navigator.sendBeacon`
//!   send it as a simple request (no CORS preflight, except Chromium's
//!   Private Network Access one, answered by `OPTIONS`).

use std::process::Command;
use std::sync::{Arc, OnceLock};

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::redact::clip;
use crate::relay::{Record, RecordBatch, Relay, SenderIdentity, MAX_SHORT_CHARS};
use crate::widget::{WidgetTelemetry, MAX_TOKEN_CHARS, TOKEN_ENV, URL_ENV};

/// Path records are posted to.
pub const RECORDS_PATH: &str = "/v1/records";
/// Largest request body read; longer ones are refused before parsing.
pub const MAX_BODY_BYTES: usize = 16 * 1024;

type HmacSha256 = Hmac<Sha256>;

/// Who a token was minted for.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Claims {
    /// Window or webview label.
    #[serde(rename = "w")]
    window: String,
    #[serde(rename = "a", default, skip_serializing_if = "Option::is_none")]
    app_id: Option<String>,
}

type AppResolver = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// State shared by the handle and the server.
struct Shared {
    key: [u8; 32],
    relay: Relay,
    /// Looks up the app id of a window whose token carries none.
    app_resolver: OnceLock<AppResolver>,
    #[cfg(feature = "otlp")]
    rejected: crate::metrics::Counter<u64>,
}

impl Shared {
    fn mac(&self) -> HmacSha256 {
        HmacSha256::new_from_slice(&self.key).expect("HMAC takes any key length")
    }

    fn mint(&self, claims: &Claims) -> String {
        let encoded = hex::encode(serde_json::to_vec(claims).expect("claims serialise"));
        let mut mac = self.mac();
        mac.update(encoded.as_bytes());
        format!("{encoded}.{}", hex::encode(mac.finalize().into_bytes()))
    }

    /// The sender a token identifies, if this process minted it.
    fn verify(&self, token: &str) -> Option<SenderIdentity> {
        if token.len() > MAX_TOKEN_CHARS {
            return None;
        }
        let (encoded, tag) = token.split_once('.')?;
        let tag = hex::decode(tag).ok()?;
        let mut mac = self.mac();
        mac.update(encoded.as_bytes());
        // Constant-time comparison.
        mac.verify_slice(&tag).ok()?;
        let claims: Claims = serde_json::from_slice(&hex::decode(encoded).ok()?).ok()?;
        let app_id = claims.app_id.or_else(|| {
            let resolve = self.app_resolver.get()?;
            resolve(&claims.window).map(|a| clip(&a, MAX_SHORT_CHARS).into_owned())
        });
        Some(SenderIdentity::widget(claims.window, app_id))
    }
}

/// The handle to this process's widget endpoint: its URL, and tokens for the
/// windows it hosts. Cheap to clone.
#[derive(Clone)]
pub struct Ingress {
    url: String,
    shared: Arc<Shared>,
}

/// The listener, to run on an async runtime with [`Server::run`].
pub struct Server {
    listener: std::net::TcpListener,
    shared: Arc<Shared>,
}

impl Ingress {
    /// Bind `127.0.0.1` on an ephemeral port. Synchronous, so the URL is known
    /// before any widget window is created; serve with [`Server::run`].
    pub fn bind() -> std::io::Result<(Ingress, Server)> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}{RECORDS_PATH}", listener.local_addr()?);
        let mut key = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut key);
        let shared = Arc::new(Shared {
            key,
            relay: Relay::new(),
            app_resolver: OnceLock::new(),
            #[cfg(feature = "otlp")]
            rejected: rejected_counter(&crate::metrics::meter("ot-telemetry")),
        });
        Ok((
            Ingress {
                url,
                shared: Arc::clone(&shared),
            },
            Server { listener, shared },
        ))
    }

    /// The URL widget pages post to (`http://127.0.0.1:<port>/v1/records`).
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The endpoint and a token for one window. `window` and `app_id` are
    /// clipped to [`MAX_SHORT_CHARS`].
    pub fn widget(&self, window: &str, app_id: Option<&str>) -> WidgetTelemetry {
        let claims = Claims {
            window: clip(window, MAX_SHORT_CHARS).into_owned(),
            app_id: app_id.map(|a| clip(a, MAX_SHORT_CHARS).into_owned()),
        };
        WidgetTelemetry {
            url: self.url.clone(),
            token: self.shared.mint(&claims),
        }
    }

    /// Set [`URL_ENV`] and [`TOKEN_ENV`] on an engine host process (a
    /// framework binary that injects them into its one window). Call after
    /// `propagation::inject_env`, which removes inherited values. Never use
    /// it for third-party binaries.
    pub fn configure_host(&self, cmd: &mut Command, window: &str, app_id: Option<&str>) {
        let widget = self.widget(window, app_id);
        cmd.env(URL_ENV, widget.url).env(TOKEN_ENV, widget.token);
    }
}

/// This process's widget endpoint, or none (disabled, or the bind failed).
/// Manage it as app state; every method is a no-op without an endpoint.
#[derive(Clone, Default)]
pub struct WidgetEndpoint(Option<Ingress>);

impl WidgetEndpoint {
    /// Bind the endpoint when `enabled`. Run the returned server on the app's
    /// async runtime. A bind failure is logged and leaves widgets without
    /// telemetry; it never stops the app.
    pub fn start(enabled: bool) -> (Self, Option<Server>) {
        if !enabled {
            return (Self(None), None);
        }
        match Ingress::bind() {
            Ok((ingress, server)) => (Self(Some(ingress)), Some(server)),
            Err(e) => {
                tracing::warn!(target: "telemetry", "widget telemetry endpoint not started: {e}");
                (Self(None), None)
            }
        }
    }

    /// The initialization script for widget window `window` (empty without
    /// an endpoint). Pass `app_id` when known; otherwise the
    /// [`resolve_app_ids_with`](Self::resolve_app_ids_with) callback names it.
    pub fn init_script(&self, window: &str, app_id: Option<&str>) -> String {
        self.0
            .as_ref()
            .map(|i| i.widget(window, app_id).init_script())
            .unwrap_or_default()
    }

    /// See [`Ingress::configure_host`].
    pub fn configure_host(&self, cmd: &mut Command, window: &str, app_id: Option<&str>) {
        if let Some(i) = &self.0 {
            i.configure_host(cmd, window, app_id);
        }
    }

    /// Name the app of a window whose token carries no app id (e.g. a
    /// Terminal panel, whose app is tracked by label). Set once; later calls
    /// are ignored.
    pub fn resolve_app_ids_with(&self, f: impl Fn(&str) -> Option<String> + Send + Sync + 'static) {
        if let Some(i) = &self.0 {
            let _ = i.shared.app_resolver.set(Box::new(f));
        }
    }
}

impl Server {
    /// Serve until the process exits. Must run inside a Tokio runtime.
    pub async fn run(self) {
        let listener = match tokio::net::TcpListener::from_std(self.listener) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(target: "telemetry", "widget telemetry endpoint not started: {e}");
                return;
            }
        };
        if let Ok(addr) = listener.local_addr() {
            tracing::info!(target: "telemetry", "widget telemetry endpoint on http://{addr}{RECORDS_PATH}");
        }
        if let Err(e) = axum::serve(listener, router(self.shared)).await {
            tracing::warn!(target: "telemetry", "widget telemetry endpoint stopped: {e}");
        }
    }
}

/// Why a request was refused (the `reason` of `ot.telemetry.ingress.rejected`).
#[derive(Clone, Copy, Debug)]
enum Rejection {
    TooLarge,
    Malformed,
    Unauthorized,
}

impl Rejection {
    #[cfg_attr(not(feature = "otlp"), allow(dead_code))]
    fn as_str(self) -> &'static str {
        match self {
            Rejection::TooLarge => "too_large",
            Rejection::Malformed => "malformed",
            Rejection::Unauthorized => "unauthorized",
        }
    }

    fn status(self) -> StatusCode {
        match self {
            Rejection::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Rejection::Malformed => StatusCode::BAD_REQUEST,
            Rejection::Unauthorized => StatusCode::UNAUTHORIZED,
        }
    }
}

#[derive(Deserialize)]
struct Envelope {
    token: String,
    records: Vec<Record>,
}

fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route(RECORDS_PATH, post(post_records).options(preflight))
        .fallback(|| async { StatusCode::NOT_FOUND })
        .layer(axum::middleware::map_response(allow_any_origin))
        .with_state(shared)
}

async fn post_records(State(shared): State<Arc<Shared>>, body: Body) -> Response {
    let reject = |why: Rejection| {
        #[cfg(feature = "otlp")]
        shared
            .rejected
            .add(1, &[crate::metrics::KeyValue::new("reason", why.as_str())]);
        // Not logged: a flood of bad requests must not flood the log file.
        why.status().into_response()
    };
    let bytes: Bytes = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(_) => return reject(Rejection::TooLarge),
    };
    let Ok(envelope) = serde_json::from_slice::<Envelope>(&bytes) else {
        return reject(Rejection::Malformed);
    };
    let Some(sender) = shared.verify(&envelope.token) else {
        return reject(Rejection::Unauthorized);
    };
    shared.relay.ingest(
        &sender,
        RecordBatch {
            records: envelope.records,
        },
    );
    StatusCode::NO_CONTENT.into_response()
}

/// CORS and Private Network Access preflight. The token, not the origin, is
/// the credential, so any origin may post.
async fn preflight() -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type"),
    );
    headers.insert(
        "access-control-allow-private-network",
        HeaderValue::from_static("true"),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
    response
}

async fn allow_any_origin(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    response
}

#[cfg(feature = "otlp")]
fn rejected_counter(meter: &crate::metrics::Meter) -> crate::metrics::Counter<u64> {
    meter
        .u64_counter("ot.telemetry.ingress.rejected")
        .with_description("Widget telemetry requests refused, by reason")
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Method, Request};
    use tower::ServiceExt;

    fn ingress() -> Ingress {
        Ingress::bind().unwrap().0
    }

    async fn send(ingress: &Ingress, method: Method, body: impl Into<Body>) -> Response {
        router(Arc::clone(&ingress.shared))
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(RECORDS_PATH)
                    .header(header::CONTENT_TYPE, "text/plain;charset=UTF-8")
                    .body(body.into())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    fn envelope(token: &str) -> String {
        serde_json::json!({
            "token": token,
            "records": [{ "level": "error", "message": "widget failed" }]
        })
        .to_string()
    }

    #[test]
    fn tokens_verify_only_with_the_minting_key() {
        let a = ingress();
        let b = ingress();
        let token = a.widget("t-panel-1", Some("ticker")).token;
        assert_eq!(
            a.shared.verify(&token),
            Some(SenderIdentity::widget(
                "t-panel-1".into(),
                Some("ticker".into())
            ))
        );
        assert_eq!(b.shared.verify(&token), None, "another process's key");
    }

    #[test]
    fn tampered_tokens_are_refused() {
        let a = ingress();
        let token = a.widget("t-panel-1", None).token;
        let (claims, tag) = token.split_once('.').unwrap();
        let forged = hex::encode(br#"{"w":"t-panel-2"}"#);
        for bad in [
            format!("{forged}.{tag}"),
            format!("{claims}.{}", "0".repeat(tag.len())),
            claims.to_string(),
            format!("{claims}.zz"),
            String::new(),
            "a".repeat(MAX_TOKEN_CHARS + 1),
        ] {
            assert_eq!(a.shared.verify(&bad), None, "{bad}");
        }
    }

    #[test]
    fn a_window_without_an_app_id_is_resolved_by_label() {
        let (a, _server) = Ingress::bind().unwrap();
        let endpoint = WidgetEndpoint(Some(a.clone()));
        endpoint.resolve_app_ids_with(|w| (w == "t-panel-1").then(|| "ticker".to_string()));
        let sender = |w: &str, app: Option<&str>| a.shared.verify(&a.widget(w, app).token).unwrap();
        assert_eq!(sender("t-panel-1", None).app_id.as_deref(), Some("ticker"));
        assert_eq!(sender("t-panel-2", None).app_id, None);
        assert_eq!(
            sender("t-panel-1", Some("chart")).app_id.as_deref(),
            Some("chart"),
            "a minted app id wins"
        );
    }

    #[test]
    fn a_disabled_endpoint_injects_nothing() {
        let (endpoint, server) = WidgetEndpoint::start(false);
        assert!(server.is_none());
        assert_eq!(endpoint.init_script("w", None), "");
        let mut cmd = Command::new("host");
        endpoint.configure_host(&mut cmd, "w", None);
        assert_eq!(cmd.get_envs().count(), 0);
    }

    #[test]
    fn claims_are_clipped() {
        let a = ingress();
        let token = a.widget(&"w".repeat(500), Some(&"a".repeat(500))).token;
        let sender = a.shared.verify(&token).unwrap();
        assert!(sender.window.len() < 200, "{}", sender.window);
        assert!(sender.app_id.unwrap().len() < 200);
    }

    #[test]
    fn hosts_get_the_url_and_a_token_only() {
        let a = ingress();
        let mut cmd = Command::new("host");
        a.configure_host(&mut cmd, "ticker", Some("ticker"));
        let env: Vec<_> = cmd
            .get_envs()
            .map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_owned())))
            .collect();
        assert_eq!(env.len(), 2);
        let token = env
            .iter()
            .find(|(k, _)| k == TOKEN_ENV)
            .and_then(|(_, v)| v.clone())
            .unwrap();
        assert!(a.shared.verify(&token.to_string_lossy()).is_some());
    }

    #[tokio::test]
    async fn accepts_a_valid_post() {
        let a = ingress();
        let token = a.widget("t-panel-1", Some("ticker")).token;
        let res = send(&a, Method::POST, envelope(&token)).await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert_eq!(res.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    }

    #[tokio::test]
    async fn refuses_bad_tokens_bodies_and_sizes() {
        let a = ingress();
        let res = send(&a, Method::POST, envelope("00.00")).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(res.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");

        let res = send(&a, Method::POST, "not json").await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        let token = a.widget("t-panel-1", None).token;
        let huge = serde_json::json!({
            "token": token,
            "records": [{ "level": "info", "message": "x".repeat(MAX_BODY_BYTES) }]
        })
        .to_string();
        let res = send(&a, Method::POST, huge).await;
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn answers_the_private_network_preflight() {
        let a = ingress();
        let res = send(&a, Method::OPTIONS, Body::empty()).await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            res.headers()["access-control-allow-private-network"],
            "true"
        );
        assert_eq!(res.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    }

    #[tokio::test]
    async fn serves_over_loopback() {
        let (a, server) = Ingress::bind().unwrap();
        assert!(a.url().starts_with("http://127.0.0.1:"));
        tokio::spawn(server.run());
        let token = a.widget("t-panel-1", None).token;
        let body = envelope(&token);
        let addr = a
            .url()
            .trim_start_matches("http://")
            .trim_end_matches(RECORDS_PATH)
            .to_string();
        let response = tokio::task::spawn_blocking(move || {
            use std::io::{Read, Write};
            let mut stream = std::net::TcpStream::connect(addr).unwrap();
            write!(
                stream,
                "POST {RECORDS_PATH} HTTP/1.1\r\nhost: x\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            let mut out = String::new();
            stream.read_to_string(&mut out).unwrap();
            out
        })
        .await
        .unwrap();
        assert!(response.starts_with("HTTP/1.1 204"), "{response}");
    }
}
