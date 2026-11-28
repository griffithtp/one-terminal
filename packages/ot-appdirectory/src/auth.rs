//! Who may change the App Directory.
//!
//! Records decide which apps the Desktop Agent launches and which web
//! origins may join the FDC3 bus, so `POST` / `PUT` / `DELETE /v2/apps`
//! need a bearer token. Reads stay open: the FDC3 App Directory API is
//! read by every Terminal and Desktop Agent.
//!
//! - The **embedded** directory (in the Desktop Agent) gets a random token
//!   per run; the agent hands it to its management UI and to the scaffolder.
//! - The **standalone** server (`apps/app-directory-server`) reads
//!   `APPD_WRITE_TOKEN`; without one it is read-only.

use std::sync::Arc;

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Json, Response};

use crate::types::ErrorResponse;

/// The write policy of an App Directory router.
#[derive(Clone)]
pub enum WriteAuth {
    /// No writes at all (`403`).
    ReadOnly,
    /// Writes need `Authorization: Bearer <token>`.
    Token(Arc<str>),
}

impl WriteAuth {
    /// `Token` for a non-empty token, else `ReadOnly`.
    pub fn from_token(token: Option<String>) -> Self {
        match token.map(|t| t.trim().to_string()) {
            Some(t) if !t.is_empty() => Self::Token(t.into()),
            _ => Self::ReadOnly,
        }
    }

    /// Whether writes are possible at all (for startup logs).
    pub fn writes_enabled(&self) -> bool {
        matches!(self, Self::Token(_))
    }

    /// Check a write request's headers.
    pub fn check(&self, headers: &HeaderMap) -> Result<(), AuthError> {
        let expected = match self {
            Self::ReadOnly => return Err(AuthError::ReadOnly),
            Self::Token(t) => t,
        };
        let presented = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                let (scheme, rest) = v.split_once(' ')?;
                scheme.eq_ignore_ascii_case("bearer").then(|| rest.trim())
            });
        match presented {
            Some(p) if constant_time_eq(p.as_bytes(), expected.as_bytes()) => Ok(()),
            _ => Err(AuthError::Unauthorized),
        }
    }
}

/// Why a write was refused; converts into the response to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    /// The directory is read-only (`403`).
    ReadOnly,
    /// No or wrong bearer token (`401`).
    Unauthorized,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            Self::ReadOnly => (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    message: "WritesDisabled: this App Directory is read-only".into(),
                }),
            )
                .into_response(),
            Self::Unauthorized => {
                let mut resp = (
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorResponse {
                        message: "Unauthorized: a bearer token is required to change apps".into(),
                    }),
                )
                    .into_response();
                resp.headers_mut().insert(
                    header::WWW_AUTHENTICATE,
                    HeaderValue::from_static("Bearer realm=\"app-directory\""),
                );
                resp
            }
        }
    }
}

/// Compare without an early exit on the first differing byte, so response
/// time doesn't reveal how much of a guessed token was right.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(auth: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(a) = auth {
            h.insert(header::AUTHORIZATION, HeaderValue::from_str(a).unwrap());
        }
        h
    }

    #[test]
    fn empty_or_missing_token_means_read_only() {
        for t in [None, Some(String::new()), Some("   ".into())] {
            assert!(!WriteAuth::from_token(t).writes_enabled());
        }
        assert!(WriteAuth::from_token(Some("x".into())).writes_enabled());
    }

    #[test]
    fn read_only_refuses_every_write() {
        let err = WriteAuth::ReadOnly
            .check(&headers(Some("Bearer anything")))
            .unwrap_err();
        assert_eq!(err, AuthError::ReadOnly);
        assert_eq!(err.into_response().status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn token_must_match_exactly() {
        let auth = WriteAuth::from_token(Some("s3cret".into()));
        assert!(auth.check(&headers(Some("Bearer s3cret"))).is_ok());
        assert!(auth.check(&headers(Some("bearer s3cret"))).is_ok());
        for bad in [
            None,
            Some("Bearer"),
            Some("Bearer s3cre"),
            Some("Bearer s3cret2"),
            Some("Basic s3cret"),
            Some("s3cret"),
        ] {
            let err = auth.check(&headers(bad)).unwrap_err().into_response();
            assert_eq!(err.status(), StatusCode::UNAUTHORIZED, "{bad:?}");
            assert!(err.headers().contains_key(header::WWW_AUTHENTICATE));
        }
    }

    #[test]
    fn constant_time_eq_basics() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
