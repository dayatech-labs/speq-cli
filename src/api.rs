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
    /// Dayaboard project an epic's first ticket goes to, set by a platform admin. Older APIs send none.
    #[serde(default, deserialize_with = "printable_text::option")]
    pub dayaboard_project_key: Option<String>,
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
    /// Origin of the Speq Web app; a document opens at `<web_url>/p/<project_id>/<path>`. Older APIs send none.
    #[serde(default)]
    pub web_url: Option<String>,
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
    /// Absent while the Dayaboard integration is off.
    #[serde(default)]
    pub dayaboard: Option<crate::dayaboard::Grant>,
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
    /// Older APIs send neither flag: the account is then a plain user as far as the CLI can tell.
    #[serde(default)]
    pub is_platform_admin: bool,
    #[serde(default)]
    pub is_superadmin: bool,
}

/// Server text made safe to show: control characters (newlines, terminal escape sequences) and bidirectional
/// overrides become spaces, so a value can neither forge terminal output nor add lines to Git's helper protocol.
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if forges_output(c) { ' ' } else { c })
        .collect()
}

/// A character nobody sees but a program, or a model, still reads: zero-width and direction marks, bidirectional
/// controls and the invisible operators, line and paragraph separators, the byte-order mark, interlinear
/// annotation marks, and the Unicode tag block (which spells ASCII invisibly, so it can hide an instruction in plain
/// sight). Variation selectors are left out: emoji need them.
pub fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{E0000}'..='\u{E007F}'
            | '\u{E0100}'..='\u{E01EF}'
    )
}

/// A control character (terminal escape sequences, line breaks), a bidirectional override or another invisible
/// character: text that can forge what a terminal shows or hide what a reader cannot see.
pub fn forges_output(c: char) -> bool {
    c.is_control() || is_invisible(c)
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

/// A project or UI product id as the API issues them (UUIDs, or short slugs): safe as one path segment of a request.
pub fn is_project_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

/// A prototype page as the API serves it: the HTML is untrusted text, never rendered or run by speq.
#[derive(Debug, Clone, Deserialize)]
pub struct UiPage {
    pub path: String,
    pub content: String,
    /// The blob ID of this page.
    pub sha: String,
    /// The commit of the ui repository the page was read at.
    pub head_sha: String,
}

/// One mock persona of a UI product, from `personas.json` at the root of its ui repository. Every text is untrusted.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Persona {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub note: Option<String>,
    /// The page the persona opens on, a slug such as `admin/orders`.
    #[serde(default)]
    pub start: Option<String>,
    /// The pages the persona sees, as patterns such as `admin/*`.
    #[serde(default)]
    pub pages: Vec<String>,
}

/// `GET /v1/ui-products/:id/personas`.
#[derive(Debug, Clone, Deserialize)]
pub struct UiPersonas {
    /// The commit of the ui repository `personas.json` was read at.
    pub head_sha: String,
    /// `invalid` or `too_large` when `personas.json` is unusable.
    #[serde(default)]
    pub problem: Option<String>,
    #[serde(default)]
    pub personas: Vec<Persona>,
}

/// `GET /v1/ui-products/:id/design`: `DESIGN.md` at the root of the ui repository.
#[derive(Debug, Clone, Deserialize)]
pub struct UiDesign {
    /// The commit of the ui repository `DESIGN.md` was read at.
    pub head_sha: String,
    /// `too_large` or `invalid` when `DESIGN.md` is unusable; `content` is then `null`.
    #[serde(default)]
    pub problem: Option<String>,
    /// The text of the file; `null` when there is none or it is unusable. An empty file is `""`. Untrusted.
    #[serde(default)]
    pub content: Option<String>,
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

/// The HTTP client every Bearer-authenticated call uses.
pub(crate) fn http_client() -> Result<reqwest::Client> {
    install_crypto_provider();
    reqwest::Client::builder()
        .user_agent(format!("speq-cli/{}", env!("CARGO_PKG_VERSION")))
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        // The API never redirects; following one could forward the Bearer token elsewhere.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| Error::Other(format!("cannot build the HTTP client: {e}")))
}

impl Client {
    pub fn new(base: Url) -> Result<Client> {
        Ok(Client {
            http: http_client()?,
            base,
        })
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
        // `dayaboard`: also receive a Dayaboard API key; the API ignores it while the integration is off.
        let body = serde_json::json!({ "code_challenge": code_challenge, "code_challenge_method": "S256", "device_name": device_name, "dayaboard": true });
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

    // ---- ui prototypes

    /// One prototype page (`pages/**.html`) of a UI product's `<product>-ui` repository, as text. Read-only.
    pub async fn ui_page(
        &self,
        token: &SecretString,
        ui_product_id: &str,
        path: &str,
    ) -> Result<UiPage> {
        // The id becomes a path segment of the request: only a plain token may take that place.
        if !is_project_id(ui_product_id) {
            return Err(Error::invalid(format!(
                "{ui_product_id:?} is not a UI product id"
            )));
        }
        let api_path = format!("/v1/ui-products/{ui_product_id}/pages/content");
        let page: UiPage = Self::json(
            self.send(self.get_call(&api_path, &[("path", path)], token))
                .await?,
        )
        .await?;
        // Both go to the terminal and into reports verbatim, so each must be what it claims to be.
        if !is_object_id(&page.sha) || !is_object_id(&page.head_sha) {
            return Err(Error::Other(
                "the Speq API returned an invalid commit ID for the page".into(),
            ));
        }
        Ok(page)
    }

    /// The mock personas of a UI product, from `personas.json` of its `<product>-ui` repository. Read-only.
    pub async fn ui_personas(
        &self,
        token: &SecretString,
        ui_product_id: &str,
    ) -> Result<UiPersonas> {
        if !is_project_id(ui_product_id) {
            return Err(Error::invalid(format!(
                "{ui_product_id:?} is not a UI product id"
            )));
        }
        let api_path = format!("/v1/ui-products/{ui_product_id}/personas");
        let response: UiPersonas =
            Self::json(self.send(self.get_call(&api_path, &[], token)).await?).await?;
        if !is_object_id(&response.head_sha) {
            return Err(Error::Other(
                "the Speq API returned an invalid commit ID for the personas".into(),
            ));
        }
        Ok(response)
    }

    /// `DESIGN.md` of a UI product's `<product>-ui` repository. Read-only.
    pub async fn ui_design(&self, token: &SecretString, ui_product_id: &str) -> Result<UiDesign> {
        if !is_project_id(ui_product_id) {
            return Err(Error::invalid(format!(
                "{ui_product_id:?} is not a UI product id"
            )));
        }
        let api_path = format!("/v1/ui-products/{ui_product_id}/design");
        let response: UiDesign =
            Self::json(self.send(self.get_call(&api_path, &[], token)).await?).await?;
        if !is_object_id(&response.head_sha) {
            return Err(Error::Other(
                "the Speq API returned an invalid commit ID for the design".into(),
            ));
        }
        // A problem comes without text, and only these two exist: anything else is not this API's answer.
        match (response.problem.as_deref(), &response.content) {
            (None, _) | (Some("too_large" | "invalid"), None) => {}
            _ => {
                return Err(Error::Other(
                    "unexpected API response: the design answer breaks its contract".into(),
                ));
            }
        }
        Ok(response)
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
    fn invisible_characters_are_the_ones_nobody_sees_but_a_reader_can_read() {
        for c in [
            '\u{00AD}',
            '\u{061C}',
            '\u{200B}',
            '\u{200D}',
            '\u{200F}',
            '\u{2028}',
            '\u{2029}',
            '\u{202E}',
            '\u{2060}',
            '\u{2066}',
            '\u{206F}',
            '\u{FEFF}',
            '\u{FFF9}',
            '\u{E0001}',
            '\u{E0041}',
            '\u{E007F}',
            '\u{E0100}',
        ] {
            assert!(is_invisible(c), "U+{:04X}", u32::from(c));
            assert!(forges_output(c), "U+{:04X}", u32::from(c));
        }
        // Text people write stays: letters, accents, symbols, CJK, emoji with their variation selector.
        for c in [
            'a', 'é', '—', '✓', '日', '😀', '\u{FE0F}', ' ', '\u{00A0}', '\u{2010}',
        ] {
            assert!(!is_invisible(c), "U+{:04X}", u32::from(c));
        }
        // The edges of each range are not swallowed.
        assert!(!is_invisible('\u{200A}') && !is_invisible('\u{2010}'));
        assert!(
            !is_invisible('\u{E0080}') && !is_invisible('\u{E00FF}') && !is_invisible('\u{E01F0}')
        );
    }

    #[test]
    fn printable_text_drops_invisible_characters_too() {
        assert_eq!(printable("a\u{200B}b\u{E0041}c"), "a b c");
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
