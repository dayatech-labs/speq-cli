//! Dayaboard, the ticket tracker. `speq login` receives a personal Dayaboard API key from the Speq API, which asks
//! Dayaboard for it with the browser's fresh Google sign-in. The key is stored next to the Speq token, expires with
//! it (there is no refresh for either), and is revoked when this device logs out or signs in again. Only the key is
//! sent to Dayaboard, and only to the origin that issued it; the Speq token never goes there.
use crate::api::{http_client, parse_api_url, printable};
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
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
