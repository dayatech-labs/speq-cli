//! Dayaboard, the ticket tracker. `speq login` receives a personal Dayaboard API key from the Speq API, which asks
//! Dayaboard for it with the browser's fresh Google sign-in. The key is stored next to the Speq token, expires with
//! it (there is no refresh for either), and is revoked when this device logs out or signs in again. Only the key is
//! sent to Dayaboard, and only to the origin that issued it; the Speq token never goes there.
use crate::api::{http_client, parse_api_url, printable};
use crate::error::{ApiError, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use time::OffsetDateTime;

/// What the Speq API reports about Dayaboard at the end of `speq login`; absent while the integration is off.
#[derive(Deserialize)]
pub struct Grant {
    pub connected: bool,
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub expires_at: Option<String>,
    pub code: Option<String>,
    pub message: Option<String>,
}

impl fmt::Debug for Grant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Grant")
            .field("connected", &self.connected)
            .field("code", &self.code)
            .finish_non_exhaustive()
    }
}

/// The stored key, part of the device credential.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DayaboardKey {
    /// The Dayaboard origin that issued the key: the only place it is sent.
    pub url: String,
    pub api_key: String,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}

impl DayaboardKey {
    pub fn is_expired(&self, now: OffsetDateTime) -> bool {
        self.expires_at <= now
    }
}

impl fmt::Debug for DayaboardKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DayaboardKey")
            .field("url", &self.url)
            .field("api_key", &"[redacted]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// How `speq login` left Dayaboard, for the login report.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Connection {
    Connected {
        url: String,
        #[serde(with = "time::serde::rfc3339")]
        expires_at: OffsetDateTime,
    },
    /// The Speq login still succeeded; creating tickets needs `speq login` again.
    NotConnected { code: String, message: String },
}

impl Grant {
    /// The key to store, or why there is none. A key is kept only for an https (or loopback) origin and a future expiry.
    pub fn accept(self, now: OffsetDateTime) -> (Option<DayaboardKey>, Connection) {
        let refused = |code: &str, message: &str| {
            (
                None,
                Connection::NotConnected {
                    code: printable(code),
                    message: printable(message),
                },
            )
        };
        if !self.connected {
            return refused(
                self.code.as_deref().unwrap_or("dayaboard_unavailable"),
                self.message
                    .as_deref()
                    .unwrap_or("Dayaboard could not be connected"),
            );
        }
        let (Some(url), Some(api_key), Some(expires_at)) =
            (self.url, self.api_key, self.expires_at)
        else {
            return refused(
                "invalid_response",
                "the Speq API sent an incomplete Dayaboard key",
            );
        };
        let Ok(origin) = parse_api_url(&url) else {
            return refused(
                "invalid_response",
                "the Speq API sent an unsafe Dayaboard URL",
            );
        };
        let Some(expires_at) = crate::snapshot::parse_time(&expires_at).filter(|t| *t > now) else {
            return refused(
                "invalid_response",
                "the Speq API sent an invalid Dayaboard key expiry",
            );
        };
        let url = origin.as_str().trim_end_matches('/').to_string();
        (
            Some(DayaboardKey {
                url: url.clone(),
                api_key,
                expires_at,
            }),
            Connection::Connected { url, expires_at },
        )
    }
}

/// Whether logging out revoked the key on Dayaboard; when it could not, the key still ends at `expires_at`.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Revocation {
    pub revoked: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}

/// Revokes the key on Dayaboard. A key Dayaboard no longer accepts (`401`) is already as good as revoked.
pub async fn disconnect(key: &DayaboardKey) -> Result<()> {
    let mut url = parse_api_url(&key.url)?;
    url.set_path("/api/auth/app-disconnect");
    let response = http_client()?
        .post(url)
        .bearer_auth(&key.api_key)
        .send()
        .await
        .map_err(|e| Error::Network(format!("Dayaboard could not be reached: {e}")))?;
    let status = response.status();
    if status.is_success() || status == reqwest::StatusCode::UNAUTHORIZED {
        Ok(())
    } else {
        Err(Error::Network(format!(
            "Dayaboard answered HTTP {}",
            status.as_u16()
        )))
    }
}

/// Best effort: logging out never fails because Dayaboard is unreachable.
pub async fn revoke(key: &DayaboardKey) -> Revocation {
    let revoked = match disconnect(key).await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(error = %e, "could not revoke the Dayaboard API key");
            false
        }
    };
    Revocation {
        revoked,
        expires_at: key.expires_at,
    }
}

// ---------------------------------------------------------- `speq dayaboard`

/// What `speq dayaboard` may reach: the workflow API and Dayaboard's docs. The rest of Dayaboard is browser-only or
/// manages API keys. DELETE is not offered: Dayaboard refuses it for API keys, deletions are made in its Web app.
const ALLOWED_PATHS: [&str; 2] = ["/api/v1/workflow/", "/api/docs/"];
pub const METHODS: [&str; 4] = ["GET", "POST", "PATCH", "PUT"];

/// A path (with an optional query) under [`ALLOWED_PATHS`] that cannot climb out of it or leave the origin.
pub fn check_path(path: &str) -> Result<()> {
    let route = path.split('?').next().unwrap_or_default();
    let allowed = ALLOWED_PATHS
        .iter()
        .any(|prefix| route.len() > prefix.len() && route.starts_with(prefix));
    let plain = path.bytes().all(|b| b.is_ascii_graphic())
        && !path.contains(['#', '\\'])
        && !route.contains(['%'])
        && !route.contains("//")
        && route
            .split('/')
            .all(|segment| segment != "." && segment != "..");
    if allowed && plain {
        Ok(())
    } else {
        Err(Error::invalid(format!(
            "{path:?} is not a Dayaboard workflow or docs path; it must start with /api/v1/workflow/ or /api/docs/"
        )))
    }
}

/// `Idempotency-Key` must be a UUID, as Dayaboard expects.
pub fn check_idempotency_key(key: &str) -> Result<()> {
    let uuid = key.len() == 36
        && key.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        });
    if uuid {
        Ok(())
    } else {
        Err(Error::invalid(format!(
            "{key:?} is not a UUID; the Idempotency-Key must be one"
        )))
    }
}

/// Dayaboard's answer to one call.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    /// The JSON body; a body that is not JSON becomes its printable text.
    pub body: Value,
    pub retry_after: Option<u64>,
}

impl Response {
    /// Dayaboard's error (`{ error, message, details }`) as the CLI's error, for the message and the exit code.
    pub fn error(&self) -> Option<Error> {
        if (200..300).contains(&self.status) {
            return None;
        }
        let text = |name: &str| self.body.get(name).and_then(Value::as_str).map(printable);
        Some(Error::Api(Box::new(ApiError {
            status: self.status,
            code: text("error").unwrap_or_else(|| format!("http_{}", self.status)),
            message: text("message")
                .unwrap_or_else(|| format!("Dayaboard answered HTTP {}", self.status)),
            request_id: None,
            details: self.body.get("details").cloned(),
            retry_after: self.retry_after,
        })))
    }
}

/// Bidirectional overrides survive JSON escaping and could reorder what a terminal shows; control characters are
/// escaped by the JSON printer itself.
fn defang(value: &mut Value) {
    match value {
        Value::String(s)
            if s.contains(|c| matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')) =>
        {
            *s = s
                .chars()
                .map(|c| match c {
                    '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' => ' ',
                    c => c,
                })
                .collect();
        }
        Value::Array(items) => items.iter_mut().for_each(defang),
        Value::Object(fields) => fields.values_mut().for_each(defang),
        _ => {}
    }
}

/// One call with `key`, to the origin that issued it. The caller handles `401` (the key is no longer accepted).
pub async fn send(
    key: &DayaboardKey,
    method: &str,
    path: &str,
    body: Option<&Value>,
    idempotency_key: Option<&str>,
) -> Result<Response> {
    check_path(path)?;
    if let Some(idempotency_key) = idempotency_key {
        check_idempotency_key(idempotency_key)?;
    }
    let method = METHODS
        .iter()
        .find(|m| m.eq_ignore_ascii_case(method))
        .and_then(|m| reqwest::Method::from_bytes(m.as_bytes()).ok())
        .ok_or_else(|| Error::invalid(format!("{method} is not one of {}", METHODS.join(", "))))?;
    let origin = parse_api_url(&key.url)?;
    let url = origin
        .join(path)
        .ok()
        .filter(|url| url.origin() == origin.origin())
        .ok_or_else(|| Error::invalid(format!("{path:?} is not a path on Dayaboard")))?;

    let mut request = http_client()?
        .request(method, url)
        .bearer_auth(&key.api_key);
    if let Some(body) = body {
        request = request.json(body);
    }
    if let Some(idempotency_key) = idempotency_key {
        request = request.header("Idempotency-Key", idempotency_key);
    }
    let unreachable =
        |e: reqwest::Error| Error::Network(format!("Dayaboard could not be reached: {e}"));
    let response = request.send().await.map_err(unreachable)?;
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse().ok());
    let text = response.text().await.map_err(unreachable)?;
    let mut body = serde_json::from_str(&text).unwrap_or_else(|_| Value::String(printable(&text)));
    defang(&mut body);
    Ok(Response {
        status,
        body,
        retry_after,
    })
}
