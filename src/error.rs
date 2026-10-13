//! One error type for the whole CLI. Each variant maps to a documented exit code.
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

/// Exit codes shared by every command (documented in `--help`).
pub mod exit {
    pub const OK: u8 = 0;
    pub const FAILURE: u8 = 1;
    /// 2 is reserved for usage errors reported by Clap.
    pub const AUTH: u8 = 3;
    pub const UNAVAILABLE: u8 = 4;
    pub const CONFLICT: u8 = 5;
    pub const DENIED: u8 = 6;
    pub const INVALID: u8 = 7;
}

/// Error body returned by the API: `{ error: { code, message, request_id, details? } }`.
#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    pub code: String,
    pub message: String,
    pub request_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub retry_after: Option<u64>,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}, HTTP {})", self.message, self.code, self.status)?;
        if let Some(id) = &self.request_id {
            write!(f, " [request {id}]")?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not logged in; run `speq login`")]
    NotLoggedIn,
    #[error("the session is no longer valid; run `speq login` again")]
    SessionExpired,
    #[error("network error: {0}")]
    Network(String),
    #[error("{0}")]
    Api(Box<ApiError>),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Invalid(String),
    #[error("not inside a Speq specs repository; run `speq clone <repo>`")]
    NotInitialized,
    #[error("{0}")]
    GitMissing(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("credential store: {0}")]
    CredentialStore(String),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::NotLoggedIn | Error::SessionExpired => exit::AUTH,
            Error::Network(_) => exit::UNAVAILABLE,
            Error::Api(e) => match e.status {
                401 => exit::AUTH,
                403 => exit::DENIED,
                404 | 400 | 422 => exit::INVALID,
                409 => exit::CONFLICT,
                429 | 500..=599 => exit::UNAVAILABLE,
                _ => exit::FAILURE,
            },
            Error::Conflict(_) => exit::CONFLICT,
            Error::Invalid(_) | Error::NotInitialized => exit::INVALID,
            Error::Io { .. }
            | Error::CredentialStore(_)
            | Error::GitMissing(_)
            | Error::Other(_) => exit::FAILURE,
        }
    }

    pub fn code(&self) -> &str {
        match self {
            Error::NotLoggedIn => "not_logged_in",
            Error::SessionExpired => "session_expired",
            Error::Network(_) => "network",
            Error::Api(e) => &e.code,
            Error::Conflict(_) => "conflict",
            Error::Invalid(_) => "invalid",
            Error::NotInitialized => "not_initialized",
            Error::GitMissing(_) => "git_missing",
            Error::Io { .. } => "io",
            Error::CredentialStore(_) => "credential_store",
            Error::Other(_) => "error",
        }
    }

    /// Short heading for the human error template.
    pub fn title(&self) -> &'static str {
        match self {
            Error::NotLoggedIn => "Not logged in",
            Error::SessionExpired => "Session expired",
            Error::Network(_) => "Network error",
            Error::Api(e) => match e.status {
                401 => "Session rejected",
                403 => "Access denied",
                404 => "Not found",
                409 => "Conflict",
                429 => "Rate limited",
                500..=599 => "Server unavailable",
                _ => "Request failed",
            },
            Error::Conflict(_) => "Conflict",
            Error::Invalid(_) => "Invalid request",
            Error::NotInitialized => "Not a Speq specs repository",
            Error::GitMissing(_) => "Git is required",
            Error::Io { .. } => "File error",
            Error::CredentialStore(_) => "Credential store error",
            Error::Other(_) => "Something went wrong",
        }
    }

    /// What happened, without the heading and without the next step.
    pub fn detail(&self) -> String {
        match self {
            Error::NotLoggedIn => "You are not signed in on this machine.".into(),
            Error::SessionExpired => {
                "Your token expired or was revoked, so the local credentials were removed.".into()
            }
            Error::NotInitialized => {
                "Neither this directory, its repository, nor a specs/ folder above it is a repository made by `speq clone`.".into()
            }
            Error::Api(e) => e.message.clone(),
            Error::Network(m)
            | Error::Conflict(m)
            | Error::Invalid(m)
            | Error::CredentialStore(m)
            | Error::GitMissing(m)
            | Error::Other(m) => m.clone(),
            Error::Io { context, source } => format!("{context}: {source}"),
        }
    }

    /// The next step the user can take, when there is an obvious one.
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::NotLoggedIn | Error::SessionExpired => Some("Run `speq login`".into()),
            Error::NotInitialized => Some("Run `speq clone <repo>` (see `speq list`)".into()),
            Error::GitMissing(_) => Some(crate::git::install_hint().into()),
            Error::Network(_) => Some("Check your connection and try again".into()),
            Error::Api(e) => match e.status {
                401 => Some("Run `speq login`".into()),
                429 => Some(match e.retry_after {
                    Some(s) => format!("Try again in {s}s"),
                    None => "Wait a moment and try again".into(),
                }),
                500..=599 => Some("Try again shortly".into()),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn request_id(&self) -> Option<&str> {
        match self {
            Error::Api(e) => e.request_id.as_deref(),
            _ => None,
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Error::Invalid(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Error::Conflict(message.into())
    }

    pub fn api_status(&self) -> Option<u16> {
        match self {
            Error::Api(e) => Some(e.status),
            _ => None,
        }
    }
}

/// Attach context to `std::io` errors: `fs::read(p).ctx("read config")`.
pub trait IoContext<T> {
    fn ctx(self, context: impl Into<String>) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn ctx(self, context: impl Into<String>) -> Result<T> {
        self.map_err(|source| Error::Io {
            context: context.into(),
            source,
        })
    }
}
