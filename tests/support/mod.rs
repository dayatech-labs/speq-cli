//! A stateful in-process fake of the Speq API (the contract in `speq-api/contract/`). Each project is a real bare
//! Git repository: `git fetch` is served by `git upload-pack`, and `POST /git/push` re-creates the commits there with
//! Git's own plumbing, checking SHAs, identities and trees the way the API does.
#![allow(dead_code)]
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use speq::credentials::MemoryStore;
use speq::dirs::AppDirs;
use speq::env::Env;
use speq::hashing::{sha256_bytes, sha256_hex};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
pub mod http;

pub const PROJECT: &str = "proj_payment";
pub const REPO: &str = "acme/payment-specs";
pub const STORY: &str = "epic-payment/st-001-create-payment";

#[derive(Default)]
pub struct Session {
    user: String,
    access: Option<String>,
    revoked: bool,
}

pub struct Project {
    pub repo: Option<String>,
    pub bare: PathBuf,
}

pub struct State {
    pub projects: BTreeMap<String, Project>,
    pub members: HashMap<String, Vec<String>>, // user -> project ids
    pub auth_version: HashMap<String, i64>,
    cli_requests: HashMap<String, (String, bool)>, // id -> (challenge, consumed)
    pub next_login_user: String,
    sessions: HashMap<String, Session>,
    access_index: HashMap<String, String>,
    counter: usize,
    pub snapshot_ttl: i64,
    pub send_304_headers: bool,
    /// Lifetime of the access token the next login receives (the real API issues 30 days).
    pub token_ttl: i64,
    pub log: Vec<String>,
    /// Paths every role is denied (role policy): pushes touching them are `403 policy_denied`.
    pub forbid_paths: Vec<String>,
    /// Commit this external change right before the next push is applied (someone else won the race).
    pub external_before_push: Option<(String, String)>,
    /// Every `POST /git/push` body, for assertions on what the helper sent.
    pub pushes: Vec<Value>,
    config: PathBuf,
}

pub struct FakeApi {
    pub url: String,
    pub state: Arc<Mutex<State>>,
    _repos: tempfile::TempDir,
    _server: http::HttpServer,
}

// ---------------------------------------------------------------- git plumbing

/// Runs Git for the fake itself: an empty global config, no system config, a fixed identity.
fn git_in(
    config: &Path,
    dir: &Path,
    args: &[&str],
    stdin: &[u8],
    env: &[(&str, &str)],
) -> (bool, Vec<u8>, String) {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Someone Else")
        .env("GIT_AUTHOR_EMAIL", "someone@acme.test")
        .env("GIT_COMMITTER_NAME", "Someone Else")
        .env("GIT_COMMITTER_EMAIL", "someone@acme.test")
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        command.env(k, v);
    }
    let mut child = command.spawn().expect("run git");
    {
        use std::io::Write;
        let mut input = child.stdin.take().expect("stdin");
        let _ = input.write_all(stdin);
    }
    let out = child.wait_with_output().expect("git output");
    (
        out.status.success(),
        out.stdout,
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn git_ok(config: &Path, dir: &Path, args: &[&str], stdin: &[u8], env: &[(&str, &str)]) -> String {
    let (ok, out, err) = git_in(config, dir, args, stdin, env);
    assert!(ok, "fake git {args:?} failed: {err}");
    String::from_utf8_lossy(&out).trim().to_string()
}

/// A commit on `main` of `bare` made with plumbing (no working tree), as an external push would. Added files get
/// `mode` (`100644`, or `120000` for a symbolic link whose content is its target).
fn commit_files(
    config: &Path,
    bare: &Path,
    changes: &[(&str, Option<&str>)],
    mode: &str,
    message: &str,
) -> String {
    let index = bare.join("fake-index");
    let index_env = [("GIT_INDEX_FILE", index.to_str().expect("utf-8 path"))];
    let parent = git_in(
        config,
        bare,
        &["rev-parse", "--verify", "-q", "refs/heads/main"],
        b"",
        &[],
    );
    let _ = std::fs::remove_file(&index);
    if parent.0 {
        git_ok(
            config,
            bare,
            &["read-tree", "refs/heads/main"],
            b"",
            &index_env,
        );
    }
    for (path, content) in changes {
        match content {
            Some(text) => {
                let blob = git_ok(
                    config,
                    bare,
                    &["hash-object", "-w", "--stdin"],
                    text.as_bytes(),
                    &[],
                );
                git_ok(
                    config,
                    bare,
                    &[
                        "update-index",
                        "--add",
                        "--cacheinfo",
                        &format!("{mode},{blob},{path}"),
                    ],
                    b"",
                    &index_env,
                );
            }
            None => {
                git_ok(
                    config,
                    bare,
                    &["update-index", "--force-remove", path],
                    b"",
                    &index_env,
                );
            }
        }
    }
    let tree = git_ok(config, bare, &["write-tree"], b"", &index_env);
    let parent_sha = String::from_utf8_lossy(&parent.1).trim().to_string();
    let mut args = vec!["commit-tree", tree.as_str(), "-m", message];
    if parent.0 {
        args.extend(["-p", parent_sha.as_str()]);
    }
    let commit = git_ok(config, bare, &args, b"", &[]);
    git_ok(
        config,
        bare,
        &["update-ref", "refs/heads/main", &commit],
        b"",
        &[],
    );
    let _ = std::fs::remove_file(&index);
    commit
}

fn initial_files() -> Vec<(&'static str, Option<&'static str>)> {
    vec![
        ("llms.txt", Some("# llms\n")),
        ("epics/epic-payment/prd.md", Some("# PRD\n\nPayments.\n")),
        (
            "epics/epic-payment/briefs/br-001-overview.md",
            Some("# Brief\n"),
        ),
        (
            "epics/epic-payment/architecture.md",
            Some("# Architecture\n"),
        ),
        (
            "epics/epic-payment/stories/st-001-create-payment/README.md",
            Some("# Create payment\n"),
        ),
        (
            "epics/epic-payment/stories/st-001-create-payment/tasks/be-001-api.md",
            Some("# Backend\n"),
        ),
        // Not a document: in the repository, never pushable.
        (".github/CODEOWNERS", Some("* @acme\n")),
    ]
}

impl State {
    fn new(repos: &Path) -> State {
        let config = repos.join("fake-gitconfig");
        std::fs::write(&config, "").expect("empty git config");
        let mut projects = BTreeMap::new();
        for (id, repo, files) in [
            (PROJECT, "acme/payment-specs", initial_files()),
            (
                "proj_other",
                "other/payment-specs",
                vec![("llms.txt", Some("# other\n"))],
            ),
            (
                "proj_cart",
                "acme/cart-specs",
                vec![("llms.txt", Some("# cart\n"))],
            ),
        ] {
            let bare = repos.join(format!("{id}.git"));
            let bare_arg = bare.to_str().expect("utf-8 path");
            git_ok(
                &config,
                repos,
                &["init", "-q", "--bare", "-b", "main", bare_arg],
                b"",
                &[],
            );
            commit_files(
                &config,
                &bare,
                &files,
                "100644",
                "docs: initialize product specs",
            );
            projects.insert(
                id.to_string(),
                Project {
                    repo: Some(repo.into()),
                    bare,
                },
            );
        }
        let mut members = HashMap::new();
        for user in ["user_a", "user_b"] {
            members.insert(
                user.to_string(),
                vec![
                    PROJECT.to_string(),
                    "proj_other".to_string(),
                    "proj_cart".to_string(),
                ],
            );
        }
        State {
            projects,
            members,
            auth_version: HashMap::new(),
            cli_requests: HashMap::new(),
            next_login_user: "user_a".into(),
            sessions: HashMap::new(),
            access_index: HashMap::new(),
            counter: 0,
            snapshot_ttl: 600,
            send_304_headers: true,
            token_ttl: 30 * 24 * 3600,
            log: vec![],
            forbid_paths: vec![],
            external_before_push: None,
            pushes: vec![],
            config,
        }
    }

    fn next(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}_{}", self.counter)
    }

    pub fn head(&self, project: &str) -> String {
        let bare = &self.projects[project].bare;
        git_ok(
            &self.config,
            bare,
            &["rev-parse", "refs/heads/main"],
            b"",
            &[],
        )
    }

    pub fn file(&self, project: &str, path: &str) -> Option<String> {
        let bare = &self.projects[project].bare;
        let (ok, out, _) = git_in(
            &self.config,
            bare,
            &["show", &format!("refs/heads/main:{path}")],
            b"",
            &[],
        );
        ok.then(|| String::from_utf8_lossy(&out).into_owned())
    }

    pub fn external_change(&mut self, project: &str, path: &str, content: Option<&str>) -> String {
        let bare = self.projects[project].bare.clone();
        commit_files(
            &self.config,
            &bare,
            &[(path, content)],
            "100644",
            "docs: edit on GitHub",
        )
    }

    /// A symbolic link committed on GitHub, as a malicious or careless writer could.
    pub fn external_symlink(&mut self, project: &str, path: &str, target: &str) -> String {
        let bare = self.projects[project].bare.clone();
        commit_files(
            &self.config,
            &bare,
            &[(path, Some(target))],
            "120000",
            "docs: link on GitHub",
        )
    }

    pub fn log_of(&self, project: &str) -> Vec<String> {
        let bare = &self.projects[project].bare;
        git_ok(
            &self.config,
            bare,
            &["rev-list", "refs/heads/main"],
            b"",
            &[],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    fn user_for(&self, token: &str) -> Option<String> {
        let id = self.access_index.get(token)?;
        let s = self.sessions.get(id)?;
        (!s.revoked && s.access.as_deref() == Some(token)).then(|| s.user.clone())
    }

    fn snapshot_etag(&self, user: &str) -> String {
        let version = self.auth_version.get(user).copied().unwrap_or(1);
        let ids = self.members.get(user).cloned().unwrap_or_default();
        let deny = self.forbid_paths.join(",");
        format!(
            "\"{}\"",
            &sha256_hex(format!("{user}:{version}:{ids:?}:{deny}").as_bytes())[..32]
        )
    }

    /// `POST /git/push`, checked like the API, applied with plumbing.
    fn push(&mut self, project: &str, user: &str, body: &Value) -> Resp {
        self.pushes.push(body.clone());
        let config = self.config.clone();
        let bare = self.projects[project].bare.clone();
        let email = format!("{user}@example.com");
        let commits = body["commits"].as_array().cloned().unwrap_or_default();
        if commits.len() > 20 {
            return error(
                422,
                "too_many_commits",
                "A push may carry at most 20 commits",
                Some(json!({ "count": commits.len(), "limit": 20 })),
            );
        }
        let mut seen = HashSet::new();
        let mut denied = vec![];
        for commit in &commits {
            let sha = commit["sha"].as_str().unwrap_or_default();
            let raw = commit["raw"].as_str().unwrap_or_default();
            let hashed = git_ok(
                &config,
                &bare,
                &["hash-object", "-t", "commit", "--stdin"],
                raw.as_bytes(),
                &[],
            );
            if hashed != sha {
                return error(
                    422,
                    "sha_mismatch",
                    "The commit object does not hash to its SHA",
                    Some(json!({ "sha": sha })),
                );
            }
            for line in raw.lines().take_while(|l| !l.is_empty()) {
                if let Some(person) = line
                    .strip_prefix("author ")
                    .or_else(|| line.strip_prefix("committer "))
                {
                    let got = person
                        .split('<')
                        .nth(1)
                        .and_then(|r| r.split('>').next())
                        .unwrap_or_default();
                    if !got.eq_ignore_ascii_case(&email) {
                        return error(
                            422,
                            "identity_mismatch",
                            "Author and committer must be your Speq account email",
                            Some(json!({ "sha": sha, "email": got, "expected": email })),
                        );
                    }
                }
            }
            let parents: Vec<&str> = raw
                .lines()
                .take_while(|l| !l.is_empty())
                .filter_map(|l| l.strip_prefix("parent "))
                .collect();
            let expected = parents
                .iter()
                .find(|p| !seen.contains(**p))
                .or(parents.first())
                .copied()
                .unwrap_or_default();
            if commit["base"].as_str() != Some(expected) {
                return error(
                    422,
                    "base_invalid",
                    "Changes must be relative to the parent already on the remote",
                    Some(json!({ "sha": sha, "expected": expected })),
                );
            }
            for change in commit["changes"].as_array().cloned().unwrap_or_default() {
                let path = change["path"].as_str().unwrap_or_default().to_string();
                if self.forbid_paths.contains(&path) {
                    denied.push(json!({ "sha": sha, "path": path, "action": "update" }));
                }
            }
            seen.insert(sha.to_string());
        }
        if !denied.is_empty() {
            return error(
                403,
                "policy_denied",
                "Not allowed to apply every change in this push",
                Some(Value::Array(denied)),
            );
        }
        if let Some((path, content)) = self.external_before_push.take() {
            self.external_change(project, &path, Some(&content));
        }
        let tip = commits
            .last()
            .and_then(|c| c["sha"].as_str())
            .unwrap_or_default()
            .to_string();
        let head = self.head(project);
        if head == tip {
            return (
                200,
                json!({ "head_sha": tip, "commits": commits.len(), "replayed": true }),
                vec![],
            );
        }
        if body["old_head"].as_str() != Some(head.as_str()) {
            return error(
                409,
                "fetch_first",
                "The remote has commits you do not have; fetch first",
                Some(json!({ "head_sha": head })),
            );
        }
        let index = bare.join("push-index");
        let index_env = [("GIT_INDEX_FILE", index.to_str().expect("utf-8 path"))];
        for commit in &commits {
            let raw = commit["raw"].as_str().unwrap_or_default();
            let _ = std::fs::remove_file(&index);
            git_ok(
                &config,
                &bare,
                &["read-tree", commit["base"].as_str().unwrap_or_default()],
                b"",
                &index_env,
            );
            for change in commit["changes"].as_array().cloned().unwrap_or_default() {
                let path = change["path"].as_str().unwrap_or_default();
                match change["content"].as_str() {
                    Some(content) => {
                        let blob = git_ok(
                            &config,
                            &bare,
                            &["hash-object", "-w", "--stdin"],
                            content.as_bytes(),
                            &[],
                        );
                        git_ok(
                            &config,
                            &bare,
                            &[
                                "update-index",
                                "--add",
                                "--cacheinfo",
                                &format!("100644,{blob},{path}"),
                            ],
                            b"",
                            &index_env,
                        );
                    }
                    None => {
                        git_ok(
                            &config,
                            &bare,
                            &["update-index", "--force-remove", path],
                            b"",
                            &index_env,
                        );
                    }
                }
            }
            let tree = git_ok(&config, &bare, &["write-tree"], b"", &index_env);
            if !raw.starts_with(&format!("tree {tree}\n")) {
                return error(
                    422,
                    "tree_mismatch",
                    "The changes do not produce the commit's tree",
                    None,
                );
            }
            git_ok(
                &config,
                &bare,
                &["hash-object", "-t", "commit", "-w", "--stdin"],
                raw.as_bytes(),
                &[],
            );
        }
        let _ = std::fs::remove_file(&index);
        git_ok(
            &config,
            &bare,
            &["update-ref", "refs/heads/main", &tip, &head],
            b"",
            &[],
        );
        (
            200,
            json!({ "head_sha": tip, "commits": commits.len(), "replayed": false }),
            vec![],
        )
    }
}

type Resp = (u16, Value, Vec<(String, String)>);

fn error(status: u16, code: &str, message: &str, details: Option<Value>) -> Resp {
    (
        status,
        json!({ "error": { "code": code, "message": message, "request_id": "req-test", "details": details } }),
        vec![],
    )
}

fn iso(offset_seconds: i64) -> String {
    let t = time::OffsetDateTime::now_utc() + time::Duration::seconds(offset_seconds);
    t.format(&time::format_description::well_known::Rfc3339)
        .expect("format")
}

fn pkt(line: &str) -> Vec<u8> {
    format!("{:04x}{line}", line.len() + 4).into_bytes()
}

/// Git smart HTTP for one project (`.../git/info/refs`, `.../git/git-upload-pack`), behind Basic or Bearer auth.
fn git_http(
    state: &Mutex<State>,
    method: &str,
    url: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
) -> http::Reply {
    let st = state.lock().expect("state");
    let challenge = || http::Reply {
        status: 401,
        headers: vec![("WWW-Authenticate".into(), "Basic realm=\"Speq\"".into())],
        body: b"{\"error\":{\"code\":\"unauthorized\"}}".to_vec(),
    };
    let token = headers.get("authorization").and_then(|h| {
        h.strip_prefix("Bearer ").map(str::to_string).or_else(|| {
            h.strip_prefix("Basic ")
                .and_then(|b| STANDARD.decode(b).ok())
                .and_then(|d| String::from_utf8(d).ok())
                .and_then(|d| d.split_once(':').map(|(_, p)| p.to_string()))
        })
    });
    let Some(user) = token.as_deref().and_then(|t| st.user_for(t)) else {
        return challenge();
    };
    let path = url.split('?').next().unwrap_or(url);
    let rest = path.trim_start_matches("/v1/projects/");
    let (project, tail) = rest.split_once("/git/").unwrap_or((rest, ""));
    let member = st
        .members
        .get(&user)
        .is_some_and(|ids| ids.iter().any(|i| i == project));
    let Some(p) = st.projects.get(project).filter(|_| member) else {
        return http::Reply {
            status: 404,
            headers: vec![],
            body: vec![],
        };
    };
    let bare = p.bare.to_str().expect("utf-8 path").to_string();
    let config = st.config.clone();
    drop(st);
    match (method, tail) {
        ("GET", "info/refs") if url.ends_with("?service=git-upload-pack") => {
            let refs = git_ok(
                &config,
                Path::new("."),
                &["upload-pack", "--stateless-rpc", "--advertise-refs", &bare],
                b"",
                &[],
            );
            let mut out = pkt("# service=git-upload-pack\n");
            out.extend_from_slice(b"0000");
            out.extend_from_slice(refs.as_bytes());
            http::Reply {
                status: 200,
                headers: vec![(
                    "Content-Type".into(),
                    "application/x-git-upload-pack-advertisement".into(),
                )],
                body: out,
            }
        }
        ("POST", "git-upload-pack") => {
            assert!(
                headers.get("content-encoding").is_none_or(|e| e != "gzip"),
                "fake: gzip request bodies are not supported"
            );
            let (ok, out, err) = git_in(
                &config,
                Path::new("."),
                &["upload-pack", "--stateless-rpc", &bare],
                body,
                &[],
            );
            assert!(ok, "fake upload-pack failed: {err}");
            http::Reply {
                status: 200,
                headers: vec![(
                    "Content-Type".into(),
                    "application/x-git-upload-pack-result".into(),
                )],
                body: out,
            }
        }
        _ => http::Reply {
            status: 403,
            headers: vec![],
            body: b"{\"error\":{\"code\":\"push_not_supported\"}}".to_vec(),
        },
    }
}

fn handle(
    state: &Mutex<State>,
    base: &str,
    method: &str,
    url: &str,
    headers: &HashMap<String, String>,
    raw_body: &[u8],
) -> Resp {
    let mut st = state.lock().expect("state");
    let path = url.split('?').next().unwrap_or(url).to_string();
    st.log.push(format!("{method} {path}"));
    let body: Value = serde_json::from_slice(raw_body).unwrap_or(Value::Null);
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned);

    match (method, path.as_str()) {
        ("POST", "/v1/auth/cli/requests") => {
            let id = st.next("req");
            let challenge = body["code_challenge"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            st.cli_requests.insert(id.clone(), (challenge, false));
            return (
                201,
                json!({ "request_id": id, "verification_url": format!("{base}/v1/auth/cli/verify?request_id={id}"), "expires_at": iso(600), "poll_after_seconds": 0 }),
                vec![],
            );
        }
        ("POST", p) if p.starts_with("/v1/auth/cli/requests/") && p.ends_with("/complete") => {
            let id = p
                .trim_start_matches("/v1/auth/cli/requests/")
                .trim_end_matches("/complete")
                .to_string();
            let Some((challenge, consumed)) = st.cli_requests.get(&id).cloned() else {
                return error(404, "not_found", "Not found", None);
            };
            let verifier = body["code_verifier"].as_str().unwrap_or_default();
            if URL_SAFE_NO_PAD.encode(sha256_bytes(verifier.as_bytes())) != challenge {
                return error(400, "invalid_grant", "PKCE verifier does not match", None);
            }
            if consumed {
                return error(410, "request_consumed", "Request was already used", None);
            }
            st.cli_requests.insert(id, (challenge, true));
            let user = st.next_login_user.clone();
            let session_id = st.next("sess");
            let access = st.next("at");
            st.access_index.insert(access.clone(), session_id.clone());
            st.sessions.insert(
                session_id,
                Session {
                    user,
                    access: Some(access.clone()),
                    revoked: false,
                },
            );
            return (
                200,
                json!({ "access_token": access, "access_token_expires_at": iso(st.token_ttl) }),
                vec![],
            );
        }
        ("POST", "/v1/auth/logout") => {
            // Stateless tokens: only `all` revokes anything, and it ends every session of the user.
            let Some(user) = bearer.as_deref().and_then(|t| st.user_for(t)) else {
                return error(
                    401,
                    "unauthorized",
                    "Missing, invalid, expired, or revoked credentials",
                    None,
                );
            };
            if body["all"].as_bool() == Some(true) {
                st.sessions
                    .values_mut()
                    .filter(|s| s.user == user)
                    .for_each(|s| s.revoked = true);
            }
            return (204, Value::Null, vec![]);
        }
        _ => {}
    }

    // Everything below needs a Bearer access token.
    let Some(user) = bearer.as_deref().and_then(|t| st.user_for(t)) else {
        return error(
            401,
            "unauthorized",
            "Missing, invalid, expired, or revoked credentials",
            None,
        );
    };

    if method == "GET" && path == "/v1/me" {
        return (
            200,
            json!({ "user_id": user, "email": format!("{user}@example.com"), "name": user, "authorization_version": 1, "session": { "kind": "cli", "expires_at": iso(900) } }),
            vec![],
        );
    }
    if method == "GET" && path == "/v1/projects" {
        let etag = st.snapshot_etag(&user);
        if headers.get("if-none-match") == Some(&etag) {
            let mut h = vec![("ETag".to_string(), etag)];
            if st.send_304_headers {
                h.push(("X-Speq-Snapshot-Issued-At".into(), iso(0)));
                h.push(("X-Speq-Snapshot-Expires-At".into(), iso(st.snapshot_ttl)));
            }
            return (304, Value::Null, h);
        }
        let deny = st.forbid_paths.clone();
        let projects: Vec<Value> = st
            .members
            .get(&user)
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|id| st.projects.get(id).map(|p| (id, p)))
            .map(|(id, p)| {
                json!({ "project_id": id, "display_name": id, "repository_full_name": p.repo, "branch": "main", "roles": ["engineer"],
                    "capabilities": { "create": { "allow": ["**"], "deny": deny }, "update": { "allow": ["**"], "deny": deny }, "can_delete": true, "can_restore": false, "can_manage": false } })
            })
            .collect();
        let body = json!({ "user_id": user, "authorization_version": st.auth_version.get(&user).copied().unwrap_or(1), "issued_at": iso(0), "expires_at": iso(st.snapshot_ttl), "projects": projects });
        return (200, body, vec![("ETag".into(), etag)]);
    }

    let Some(rest) = path.strip_prefix("/v1/projects/") else {
        return error(404, "not_found", "Not found", None);
    };
    let (project_id, tail) = rest.split_once('/').unwrap_or((rest, ""));
    let member = st
        .members
        .get(&user)
        .is_some_and(|ids| ids.iter().any(|i| i == project_id));
    if !member || !st.projects.contains_key(project_id) {
        return error(404, "not_found", "Not found", None);
    }
    let project_id = project_id.to_string();
    match (method, tail) {
        ("GET", "git/head") => (200, json!({ "head_sha": st.head(&project_id) }), vec![]),
        ("POST", "git/push") => st.push(&project_id, &user, &body),
        _ => error(404, "not_found", "Not found", None),
    }
}

impl FakeApi {
    pub fn start() -> FakeApi {
        let repos = tempfile::tempdir().expect("repos");
        let state = Arc::new(Mutex::new(State::new(repos.path())));
        // The base URL is only known once the listener is bound, so the handler reads it lazily.
        let base = Arc::new(Mutex::new(String::new()));
        let (handler_state, handler_base) = (state.clone(), base.clone());
        let server = http::serve(move |request| {
            if request.url.contains("/git/info/refs") || request.url.contains("/git/git-") {
                handler_state.lock().expect("state").log.push(format!(
                    "{} {}",
                    request.method,
                    request.url.split('?').next().unwrap_or_default()
                ));
                return git_http(
                    &handler_state,
                    &request.method,
                    &request.url,
                    &request.headers,
                    &request.body,
                );
            }
            let base = handler_base.lock().expect("base").clone();
            let (status, value, extra) = handle(
                &handler_state,
                &base,
                &request.method,
                &request.url,
                &request.headers,
                &request.body,
            );
            let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
            headers.extend(extra);
            http::Reply {
                status,
                headers,
                body: if value.is_null() {
                    vec![]
                } else {
                    value.to_string().into_bytes()
                },
            }
        });
        let url = format!("http://{}", server.addr);
        *base.lock().expect("base") = url.clone();
        FakeApi {
            url,
            state,
            _repos: repos,
            _server: server,
        }
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        f(&mut self.state.lock().expect("state"))
    }

    pub fn requests(&self) -> usize {
        self.with(|s| s.log.len())
    }

    pub fn count(&self, needle: &str) -> usize {
        self.with(|s| s.log.iter().filter(|l| l.contains(needle)).count())
    }

    pub fn file(&self, path: &str) -> Option<String> {
        self.with(|s| s.file(PROJECT, path))
    }

    pub fn head(&self) -> String {
        self.with(|s| s.head(PROJECT))
    }

    pub fn log(&self) -> Vec<String> {
        self.with(|s| s.log_of(PROJECT))
    }

    pub fn external_change(&self, path: &str, content: Option<&str>) -> String {
        self.with(|s| s.external_change(PROJECT, path, content))
    }

    pub fn revoke_all_sessions(&self) {
        self.with(|s| s.sessions.values_mut().for_each(|x| x.revoked = true));
    }
}

/// One "device": its own data directory, credential store, sharing the fake API.
pub struct Device {
    pub env: Env,
    pub creds: Arc<dyn speq::credentials::CredentialStore>,
    pub data: tempfile::TempDir,
    /// Stands in for the home folder that holds `.agents/skills` and `.claude/skills`.
    pub claude: tempfile::TempDir,
}

impl Device {
    pub fn new(api: &FakeApi) -> Device {
        Device::with_store(api, Arc::new(MemoryStore::default()))
    }

    pub fn with_store(api: &FakeApi, creds: Arc<dyn speq::credentials::CredentialStore>) -> Device {
        let data = tempfile::tempdir().expect("tempdir");
        let claude = tempfile::tempdir().expect("tempdir");
        let api_url = speq::api::parse_api_url(&api.url).expect("api url");
        let env = Env {
            client: speq::api::Client::new(api_url.clone()).expect("client"),
            dirs: AppDirs::at(data.path()),
            skills_dirs: vec![
                claude.path().join(".agents/skills"),
                claude.path().join(".claude/skills"),
            ],
            creds: creds.clone(),
            api_url,
            open_browser: false,
        };
        Device {
            env,
            creds,
            data,
            claude,
        }
    }

    /// A second "process" on the same device: same data directory and credential store, own HTTP client.
    pub fn another_process(&self) -> Env {
        Env {
            client: speq::api::Client::new(self.env.api_url.clone()).expect("client"),
            dirs: AppDirs::at(self.data.path()),
            skills_dirs: self.env.skills_dirs.clone(),
            creds: self.creds.clone(),
            api_url: self.env.api_url.clone(),
            open_browser: false,
        }
    }

    pub async fn login(&self) -> speq::session::LoginReport {
        speq::session::Session::new(&self.env)
            .login()
            .await
            .expect("login")
    }
}

/// Output of one `speq` run.
#[derive(Debug)]
pub struct Out {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Out {
    pub fn ok(self) -> Self {
        assert_eq!(
            self.code, 0,
            "stdout:\n{}\nstderr:\n{}",
            self.stdout, self.stderr
        );
        self
    }
}

/// The real `speq` binary on a logged-in machine of its own: an isolated home (no user Git config), its token in
/// a file store, and the fake API as its server. Git runs it as `git-remote-speq` and `git-credential-speq`.
pub struct Machine {
    pub api: FakeApi,
    pub device: Device,
    pub home: tempfile::TempDir,
    pub store: PathBuf,
}

impl Machine {
    pub async fn new() -> Machine {
        let api = FakeApi::start();
        let home = tempfile::tempdir().expect("home");
        std::fs::write(home.path().join(".gitconfig"), "").expect("gitconfig");
        // Windows resolves the app-data folders under `%USERPROFILE%\AppData` and `ProjectDirs` fails unless both
        // `Local` and `Roaming` exist.
        for dir in ["Local", "Roaming"] {
            std::fs::create_dir_all(home.path().join("AppData").join(dir)).expect("app data");
        }
        let store = home.path().join("tokens.json");
        let device = Device::with_store(&api, Arc::new(speq::dev_store::FileStore::new(&store)));
        device.login().await;
        Machine {
            api,
            device,
            home,
            store,
        }
    }

    /// An empty folder inside the machine's home (canonical, so macOS `/private` paths compare equal). Windows
    /// canonical paths are verbatim (`\\?\C:\…`) while Git prints `C:/…`, so the prefix is dropped.
    pub fn dir(&self, name: &str) -> PathBuf {
        let dir = self.home.path().join(name);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let dir = std::fs::canonicalize(dir).expect("canonical");
        match dir.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
            Some(plain) => PathBuf::from(plain),
            None => dir,
        }
    }

    pub fn command(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_speq"));
        command
            .args(args)
            .current_dir(cwd)
            .env("HOME", self.home.path())
            .env("USERPROFILE", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join(".config"))
            .env("XDG_DATA_HOME", self.home.path().join(".local/share"))
            .env("GIT_CONFIG_GLOBAL", self.home.path().join(".gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .env("LC_ALL", "C")
            .env("LANGUAGE", "C")
            .env("NO_COLOR", "1")
            .env("SPEQ_API_URL", &self.api.url)
            .env(speq::dev_store::VARIABLE, &self.store)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .stdin(Stdio::null());
        command
    }

    pub fn run(&self, cwd: &Path, args: &[&str]) -> Out {
        let out = self.command(cwd, args).output().expect("run speq");
        Out {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// `speq clone payment-specs specs` inside a fresh implementation repository; returns (impl root, specs dir).
    pub fn clone_into_app(&self) -> (PathBuf, PathBuf) {
        let app = self.dir("app");
        let init = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&app)
            .env("GIT_CONFIG_GLOBAL", self.home.path().join(".gitconfig"))
            .output()
            .expect("git init");
        assert!(init.status.success());
        self.run(&app, &["clone", "acme/payment-specs"]).ok();
        let specs = app.join("specs");
        (app, specs)
    }

    pub fn write(&self, path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, content).expect("write");
    }
}
