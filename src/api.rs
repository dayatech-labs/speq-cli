//! Typed client for the Speq API. It never logs tokens or document content.
use crate::error::{ApiError, Error, Result};
use reqwest::{Method, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::sync::Once;
use std::time::Duration;
use url::Url;

pub const DEFAULT_API_URL: &str = "https://speq-api.dayatech.workers.dev";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Reads are retried on 429/503 only, waiting at most this long per attempt.
const MAX_RETRY_WAIT: u64 = 30;
const READ_ATTEMPTS: u32 = 3;
/// A push re-creates up to 20 commits on GitHub one call at a time.
const PUSH_TIMEOUT: Duration = Duration::from_secs(120);

/// Accept `https` URLs, and plain `http` only for loopback (a local API during development).
pub fn parse_api_url(raw: &str) -> Result<Url> {
    let url =
        Url::parse(raw).map_err(|e| Error::invalid(format!("invalid API URL {raw:?}: {e}")))?;
    let loopback = matches!(url.host(), Some(url::Host::Domain("localhost")))
        || matches!(url.host(), Some(url::Host::Ipv4(ip)) if ip.is_loopback())
        || matches!(url.host(), Some(url::Host::Ipv6(ip)) if ip.is_loopback());
    match url.scheme() {
        "https" => {}
        "http" if loopback => {}
        _ => {
            return Err(Error::invalid(
                "the API URL must use https (http is allowed only for localhost)",
            ));
        }
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err(Error::invalid(
            "the API URL must be an origin without credentials",
        ));
    }
    if (url.path() != "/" && !url.path().is_empty())
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::invalid(
            "the API URL must be an origin without path, query, or fragment",
        ));
    }
    Ok(url)
}

pub fn install_crypto_provider() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // Another provider may already be installed by the host process; that is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

// ---------------------------------------------------------------- payloads

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleSet {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capabilities {
    pub create: RuleSet,
    pub update: RuleSet,
    pub can_delete: bool,
    pub can_restore: bool,
    pub can_manage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSummary {
    pub project_id: String,
    #[serde(deserialize_with = "printable_text::string")]
    pub display_name: String,
    #[serde(deserialize_with = "printable_text::option")]
    pub repository_full_name: Option<String>,
    pub branch: String,
    #[serde(deserialize_with = "printable_text::list")]
    pub roles: Vec<String>,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotBody {
    pub user_id: String,
    pub authorization_version: i64,
    pub issued_at: String,
    pub expires_at: String,
    pub projects: Vec<ProjectSummary>,
}

pub enum ProjectsResponse {
    Fresh {
        body: SnapshotBody,
        etag: Option<String>,
    },
    /// `304`: the window headers carry the new validity period.
    NotModified {
        issued_at: Option<String>,
        expires_at: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct CliRequest {
    pub request_id: String,
    pub verification_url: String,
    pub expires_at: String,
    pub poll_after_seconds: u64,
}

#[derive(Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub access_token_expires_at: String,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Tokens([redacted])")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Me {
    pub user_id: String,
    #[serde(deserialize_with = "printable_text::string")]
    pub email: String,
    #[serde(default, deserialize_with = "printable_text::option")]
    pub name: Option<String>,
}

/// Server text made safe to show: control characters (newlines, terminal escape sequences) and bidirectional
/// overrides become spaces, so a value can neither forge terminal output nor add lines to Git's helper protocol.
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// `deserialize_with` helpers applying [`printable`] to text the API sends for display.
mod printable_text {
    use serde::{Deserialize, Deserializer};

    pub fn string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
        Ok(super::printable(&String::deserialize(d)?))
    }

    pub fn option<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
        Ok(Option::<String>::deserialize(d)?.map(|s| super::printable(&s)))
    }

    pub fn list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
        Ok(Vec::<String>::deserialize(d)?
            .iter()
            .map(|s| super::printable(s))
            .collect())
    }
}

/// A full commit ID (SHA-1 or SHA-256, lowercase hex), as Git prints it.
pub fn is_object_id(sha: &str) -> bool {
    matches!(sha.len(), 40 | 64) && sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// One commit of a `git push`, as `POST /git/push` takes it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PushedCommit {
    pub sha: String,
    /// The object exactly as `git cat-file commit` prints it.
    pub raw: String,
    /// The parent `changes` are relative to.
    pub base: String,
    pub changes: Vec<PushedChange>,
}

/// A path that differs from the commit's base; `content: None` deletes it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PushedChange {
    pub path: String,
    pub content: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GitPushRequest<'a> {
    pub old_head: &'a str,
    pub commits: &'a [PushedCommit],
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitPushResult {
    pub head_sha: String,
    pub commits: u32,
    pub replayed: bool,
}

// ------------------------------------------------------------------ client

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: Url,
}

struct Call<'a> {
    method: Method,
    url: Url,
    token: Option<&'a SecretString>,
    json: Option<serde_json::Value>,
    headers: Vec<(&'static str, String)>,
    retry: bool,
    timeout: Option<Duration>,
}

impl Client {
    pub fn new(base: Url) -> Result<Client> {
        install_crypto_provider();
        let http = reqwest::Client::builder()
            .user_agent(format!("speq-cli/{}", env!("CARGO_PKG_VERSION")))
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            // The API never redirects; following one could forward the Bearer token elsewhere.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| Error::Other(format!("cannot build the HTTP client: {e}")))?;
        Ok(Client { http, base })
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    fn url(&self, path: &str, query: &[(&str, &str)]) -> Url {
        let mut url = self.base.clone();
        url.set_path(path);
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in query {
                pairs.append_pair(k, v);
            }
        }
        url
    }

    async fn send(&self, call: Call<'_>) -> Result<reqwest::Response> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let mut request = self.http.request(call.method.clone(), call.url.clone());
            if let Some(token) = call.token {
                request = request.bearer_auth(token.expose_secret());
            }
            for (name, value) in &call.headers {
                request = request.header(*name, value);
            }
            if let Some(body) = &call.json {
                request = request.json(body);
            }
            if let Some(timeout) = call.timeout {
                request = request.timeout(timeout);
            }
            let response = request
                .send()
                .await
                .map_err(|e| Error::Network(describe_transport(&e)))?;
            let status = response.status();
            let request_id = response
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            tracing::debug!(method = %call.method, path = call.url.path(), status = status.as_u16(), request_id = request_id.as_deref(), "api response");

            if status.is_success() || status == StatusCode::NOT_MODIFIED {
                return Ok(response);
            }
            let error = read_error(response, status, request_id).await;
            // Tokens cannot be refreshed: a rejected one always means signing in again.
            if status == StatusCode::UNAUTHORIZED && call.token.is_some() {
                return Err(Error::SessionExpired);
            }
            let retryable =
                matches!(status.as_u16(), 429 | 503) && call.retry && attempt < READ_ATTEMPTS;
            let wait = error.retry_after.filter(|s| *s <= MAX_RETRY_WAIT);
            match (retryable, wait) {
                (true, Some(seconds)) => tokio::time::sleep(Duration::from_secs(seconds)).await,
                _ => return Err(Error::Api(Box::new(error))),
            }
        }
    }

    async fn json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
        response
            .json::<T>()
            .await
            .map_err(|e| Error::Other(format!("unexpected API response: {e}")))
    }

    fn get_call<'a>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        token: &'a SecretString,
    ) -> Call<'a> {
        Call {
            method: Method::GET,
            url: self.url(path, query),
            token: Some(token),
            json: None,
            headers: vec![],
            retry: true,
            timeout: None,
        }
    }

    fn post_call<'a>(
        &self,
        path: &str,
        token: Option<&'a SecretString>,
        json: serde_json::Value,
    ) -> Call<'a> {
        Call {
            method: Method::POST,
            url: self.url(path, &[]),
            token,
            json: Some(json),
            headers: vec![],
            retry: false,
            timeout: None,
        }
    }

    // ---- authentication

    pub async fn cli_request(&self, code_challenge: &str, device_name: &str) -> Result<CliRequest> {
        let body = serde_json::json!({ "code_challenge": code_challenge, "code_challenge_method": "S256", "device_name": device_name });
        Self::json(
            self.send(self.post_call("/v1/auth/cli/requests", None, body))
                .await?,
        )
        .await
    }

    /// `authorization_pending`, `slow_down`, `invalid_grant` and `410` arrive as [`Error::Api`].
    pub async fn cli_complete(&self, request_id: &str, code_verifier: &str) -> Result<Tokens> {
        let body = serde_json::json!({ "code_verifier": code_verifier });
        let path = format!("/v1/auth/cli/requests/{request_id}/complete");
        Self::json(self.send(self.post_call(&path, None, body)).await?).await
    }

    /// Ends every token this user holds, on every device and in the Web app. Tokens are stateless, so
    /// there is no way to revoke just this one.
    pub async fn logout_everywhere(&self, token: &SecretString) -> Result<()> {
        let body = serde_json::json!({ "all": true });
        self.send(self.post_call("/v1/auth/logout", Some(token), body))
            .await
            .map(|_| ())
    }

    pub async fn me(&self, token: &SecretString) -> Result<Me> {
        Self::json(self.send(self.get_call("/v1/me", &[], token)).await?).await
    }

    // ---- capability snapshot

    pub async fn projects(
        &self,
        token: &SecretString,
        etag: Option<&str>,
    ) -> Result<ProjectsResponse> {
        let mut call = self.get_call("/v1/projects", &[], token);
        if let Some(etag) = etag {
            call.headers.push(("If-None-Match", etag.to_string()));
        }
        let response = self.send(call).await?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(ProjectsResponse::NotModified {
                issued_at: header("x-speq-snapshot-issued-at"),
                expires_at: header("x-speq-snapshot-expires-at"),
            });
        }
        let etag = header("etag");
        Ok(ProjectsResponse::Fresh {
            body: Self::json(response).await?,
            etag,
        })
    }

    // ---- git

    /// The branch head on GitHub right now.
    pub async fn git_head(&self, token: &SecretString, project_id: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Head {
            head_sha: String,
        }
        let path = format!("/v1/projects/{project_id}/git/head");
        let head: Head = Self::json(self.send(self.get_call(&path, &[], token)).await?).await?;
        // It goes to Git verbatim, as a revision argument and a line of the helper protocol.
        if !is_object_id(&head.head_sha) {
            return Err(Error::Other(
                "the Speq API returned an invalid commit ID for the branch head".into(),
            ));
        }
        Ok(head.head_sha)
    }

    /// A short-lived token that only fetches this project: what `git-credential-speq` gives Git, so Git never holds
    /// the CLI token.
    pub async fn git_token(&self, token: &SecretString, project_id: &str) -> Result<SecretString> {
        #[derive(Deserialize)]
        struct GitToken {
            token: String,
        }
        let path = format!("/v1/projects/{project_id}/git/token");
        let issued: GitToken = Self::json(
            self.send(self.post_call(&path, Some(token), serde_json::json!({})))
                .await?,
        )
        .await?;
        // It becomes a `password=` line of Git's credential protocol: one plain token, nothing that adds a line.
        let plain = issued.token.len() <= 4096
            && issued.token.strip_prefix("sgr_").is_some_and(|rest| {
                !rest.is_empty()
                    && rest
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            });
        if !plain {
            return Err(Error::Other(
                "the Speq API returned an invalid Git token".into(),
            ));
        }
        Ok(SecretString::from(issued.token))
    }

    /// Re-creates the commits on the remote and moves its branch. Never retried automatically.
    pub async fn git_push(
        &self,
        token: &SecretString,
        project_id: &str,
        request: &GitPushRequest<'_>,
    ) -> Result<GitPushResult> {
        let path = format!("/v1/projects/{project_id}/git/push");
        let body = serde_json::to_value(request).map_err(|e| Error::Other(e.to_string()))?;
        let mut call = self.post_call(&path, Some(token), body);
        call.timeout = Some(PUSH_TIMEOUT);
        Self::json(self.send(call).await?).await
    }
}

fn describe_transport(e: &reqwest::Error) -> String {
    // The URL may contain a path or query but never a token; still, report only the failure kind.
    if e.is_timeout() {
        "the request timed out".into()
    } else if e.is_connect() {
        "cannot connect to the Speq API".into()
    } else {
        "the request to the Speq API failed".into()
    }
}

async fn read_error(
    response: reqwest::Response,
    status: StatusCode,
    request_id: Option<String>,
) -> ApiError {
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());
    #[derive(Deserialize)]
    struct Envelope {
        error: Inner,
    }
    #[derive(Deserialize)]
    struct Inner {
        code: String,
        message: String,
        request_id: Option<String>,
        details: Option<serde_json::Value>,
    }
    let bytes = response.bytes().await.unwrap_or_default();
    match serde_json::from_slice::<Envelope>(&bytes) {
        Ok(Envelope { error }) => ApiError {
            status: status.as_u16(),
            code: printable(&error.code),
            message: printable(&error.message),
            request_id: error.request_id.or(request_id).map(|id| printable(&id)),
            details: error.details,
            retry_after,
        },
        Err(_) => ApiError {
            status: status.as_u16(),
            code: "http_error".into(),
            message: status
                .canonical_reason()
                .unwrap_or("unexpected response")
                .to_string(),
            request_id,
            details: None,
            retry_after,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_url_rules() {
        assert!(parse_api_url("https://speq-api.dayatech.workers.dev").is_ok());
        assert!(parse_api_url("http://localhost:8787").is_ok());
        assert!(parse_api_url("http://127.0.0.1:8787").is_ok());
        assert!(parse_api_url("http://example.com").is_err());
        assert!(parse_api_url("https://user:pw@example.com").is_err());
        assert!(parse_api_url("https://example.com/v1").is_err());
        assert!(parse_api_url("ftp://example.com").is_err());
    }

    #[test]
    fn printable_text_keeps_one_line_without_escape_sequences() {
        assert_eq!(
            printable("Payment specs · Ünïcode"),
            "Payment specs · Ünïcode"
        );
        assert_eq!(
            printable("a\nok refs/heads/main\r\n"),
            "a ok refs/heads/main  "
        );
        assert_eq!(printable("\x1b]52;c;cHduZWQ=\x07x"), " ]52;c;cHduZWQ= x");
        assert_eq!(printable("\u{9b}31mred"), " 31mred", "C1 control (CSI)");
        assert_eq!(printable("abc\u{202E}fdp.exe"), "abc fdp.exe");
    }

    #[test]
    fn display_fields_from_the_api_are_made_printable() {
        let project: ProjectSummary = serde_json::from_value(serde_json::json!({
            "project_id": "p1",
            "display_name": "Pay\x1b[2Jments",
            "repository_full_name": "acme/specs\nRun `speq deinit --yes`",
            "branch": "main",
            "roles": ["engineer\r"],
            "capabilities": { "create": { "allow": [], "deny": [] }, "update": { "allow": [], "deny": [] },
                "can_delete": false, "can_restore": false, "can_manage": false }
        }))
        .unwrap();
        assert_eq!(project.display_name, "Pay [2Jments");
        assert_eq!(
            project.repository_full_name.as_deref(),
            Some("acme/specs Run `speq deinit --yes`")
        );
        assert_eq!(project.roles, ["engineer "]);

        let me: Me = serde_json::from_value(
            serde_json::json!({ "user_id": "u", "email": "a@b.test", "name": "A\x1b[0m" }),
        )
        .unwrap();
        assert_eq!(me.name.as_deref(), Some("A [0m"));
        let nameless: Me =
            serde_json::from_value(serde_json::json!({ "user_id": "u", "email": "a@b.test" }))
                .unwrap();
        assert_eq!(nameless.name, None);
    }

    #[test]
    fn object_ids_are_full_lowercase_hex() {
        assert!(is_object_id(&"a".repeat(40)));
        assert!(is_object_id(&"0123456789abcdef".repeat(4)));
        for bad in [
            "",
            "abc123",
            &"A".repeat(40),
            &format!("{}\nok refs/heads/main", "a".repeat(40)),
            &format!("--output={}", "a".repeat(31)),
            &"g".repeat(40),
            &"a".repeat(41),
        ] {
            assert!(!is_object_id(bad), "{bad:?}");
        }
    }
}
