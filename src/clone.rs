//! `speq list` and `speq clone`.
use crate::api::{Me, ProjectSummary};
use crate::env::Env;
use crate::error::{Error, IoContext, Result};
use crate::fsx::{self, Visibility};
use crate::git::Git;
use crate::links;
use crate::repo::{self, TRANSPORT};
use crate::session::Session;
use serde::Serialize;
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Debug, Serialize)]
pub struct ListReport {
    pub user_id: String,
    pub expires_at: String,
    pub projects: Vec<ProjectSummary>,
}

/// From the snapshot when it is still valid; refreshed in the foreground otherwise.
pub async fn list(env: &Env) -> Result<ListReport> {
    let snapshot = Session::new(env).snapshot().await?;
    Ok(ListReport {
        user_id: snapshot.user_id,
        expires_at: snapshot
            .expires_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        projects: snapshot.projects,
    })
}

/// Matches `owner/repo`, the bare repo name, or the project's display name (all case-insensitive).
/// It must identify exactly one project the user can use that is bound to a repository.
pub fn resolve_repo<'a>(
    projects: &'a [ProjectSummary],
    wanted: &str,
) -> Result<&'a ProjectSummary> {
    let matches: Vec<(&ProjectSummary, &str)> = projects
        .iter()
        .filter_map(|p| p.repository_full_name.as_deref().map(|r| (p, r)))
        .filter(|(p, full)| {
            let short = full.rsplit('/').next().unwrap_or(full);
            [*full, short, p.display_name.as_str()]
                .iter()
                .any(|name| name.eq_ignore_ascii_case(wanted))
        })
        .collect();
    match matches.as_slice() {
        [] => Err(Error::invalid(format!(
            "{wanted:?} is not in your repository list; see `speq list`"
        ))),
        [(project, _)] => Ok(project),
        many => Err(Error::invalid(format!(
            "{wanted:?} is ambiguous; use the full name: {}",
            many.iter()
                .map(|(_, full)| *full)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

#[derive(Debug, Serialize)]
pub struct CloneReport {
    pub project_id: String,
    pub repository: String,
    /// The new repository, as given (or chosen) relative to the current directory.
    pub path: String,
    /// The commit checked out; None when the remote branch is still empty.
    pub head_sha: Option<String>,
    /// Who commits made in this repository are authored as (the Speq account).
    pub author: String,
    /// The implementation repository's `.gitignore` that now ignores the new folder.
    pub ignored_in: Option<String>,
}

pub struct CloneOptions<'a> {
    pub repo: &'a str,
    pub dir: Option<&'a str>,
    /// The folder used inside a Git repository when `dir` is not given: the `--specs-dir`, `specs` by default.
    pub default_dir: &'a str,
}

/// Settings every specs repository gets. Line endings are kept byte for byte (a document must hash the same here
/// and on GitHub); pulls rebase so pushes rarely carry merge commits. Symbolic links from the remote are checked
/// out as plain files holding the target path: a link such as `llms.txt -> ~/.ssh/id_ed25519` would otherwise let
/// whoever writes the remote make people (and agents) read or write outside the repository.
const REPOSITORY_CONFIG: [(&str, &str); 7] = [
    ("core.autocrlf", "false"),
    ("core.symlinks", "false"),
    ("pull.rebase", "true"),
    ("rebase.autoStash", "true"),
    ("merge.autoStash", "true"),
    ("merge.conflictStyle", "diff3"),
    ("push.default", "upstream"),
];

/// The working tree `dir` is in, if any (the implementation repository `speq clone` runs in).
fn enclosing_work_tree(git: &Git, dir: &Path) -> Result<Option<PathBuf>> {
    let output = git.output(Some(dir), ["rev-parse", "--show-toplevel"])?;
    Ok(output
        .status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())))
}

/// Whether a `.gitignore` already has a line for `folder` (a path relative to it, `/`-separated): the folder itself
/// with or without slashes (`specs`, `specs/`, `/specs/`), or a bare name that matches it anywhere (`specs/` for
/// `docs/specs`).
fn already_ignored(gitignore: &str, folder: &str) -> bool {
    let name = folder.rsplit('/').next().unwrap_or(folder);
    gitignore.lines().map(str::trim).any(|line| {
        let pattern = line.trim_matches('/');
        let anywhere = !line.starts_with('/') && !pattern.contains('/');
        !pattern.is_empty() && (pattern == folder || (anywhere && pattern == name))
    })
}

/// Adds `<folder>/` to the `.gitignore` of the repository around the new one, unless it is listed already.
fn ignore_in(outer: &Path, target: &Path) -> Result<Option<PathBuf>> {
    let (Ok(outer), Ok(target)) = (std::fs::canonicalize(outer), std::fs::canonicalize(target))
    else {
        return Ok(None);
    };
    let Ok(relative) = target.strip_prefix(&outer) else {
        return Ok(None);
    };
    let folder = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    let entry = format!("{folder}/");
    let path = outer.join(".gitignore");
    let existing = fsx::read_optional(&path)?
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    if already_ignored(&existing, &folder) {
        return Ok(Some(path));
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    if !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str(&format!(
        "# Speq specs repository (managed with `speq`)\n{entry}\n"
    ));
    fsx::atomic_write(&path, updated.as_bytes(), Visibility::Shared)?;
    Ok(Some(path))
}

fn config(git: &Git, repo: &Path, args: &[&str]) -> Result<()> {
    git.text(Some(repo), [&["config"][..], args].concat())
        .map(|_| ())
}

/// Everything after the directory exists: configuration, the first fetch, and the checkout.
fn prepare(
    git: &Git,
    target: &Path,
    api_url: &Url,
    project_id: &str,
    me: &Me,
) -> Result<Option<String>> {
    let init = git.output(
        None,
        [
            std::ffi::OsStr::new("init"),
            "-q".as_ref(),
            "-b".as_ref(),
            "main".as_ref(),
            target.as_os_str(),
        ],
    )?;
    if !init.status.success() {
        return Err(Error::Other(crate::git::git_failure(&init)));
    }
    let origin = api_url.as_str().trim_end_matches('/');
    let name = me
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| me.email.split('@').next().unwrap_or(&me.email).to_string());
    config(
        git,
        target,
        &[repo::ADDRESS_KEY, &repo::address(api_url, project_id)],
    )?;
    config(
        git,
        target,
        &["remote.origin.url", &repo::fetch_url(api_url, project_id)],
    )?;
    config(
        git,
        target,
        &[
            "remote.origin.pushurl",
            &format!("{TRANSPORT}{}", repo::address(api_url, project_id)),
        ],
    )?;
    config(
        git,
        target,
        &[
            "remote.origin.fetch",
            "+refs/heads/main:refs/remotes/origin/main",
        ],
    )?;
    // An empty helper first resets the list, so a system helper (osxkeychain, manager) never stores the token.
    let helper = format!("credential.{origin}.helper");
    config(git, target, &["--add", &helper, ""])?;
    config(git, target, &["--add", &helper, "speq"])?;
    config(git, target, &["user.name", &name])?;
    config(git, target, &["user.email", &me.email])?;
    for (key, value) in REPOSITORY_CONFIG {
        config(git, target, &[key, value])?;
    }
    config(git, target, &["branch.main.remote", "origin"])?;
    config(git, target, &["branch.main.merge", "refs/heads/main"])?;

    // Attached to the terminal: Git's progress, and the credential helper's message when the session is gone.
    let fetched = git
        .command(Some(target))
        .args(["fetch", "--quiet", "origin"])
        .stdin(std::process::Stdio::null())
        .status()
        .map_err(|e| Error::Other(format!("cannot run Git: {e}")))?;
    if !fetched.success() {
        return Err(Error::Other(
            "`git fetch` from the Speq API failed (Git's message is above)".into(),
        ));
    }
    if !git.succeeds(
        Some(target),
        ["rev-parse", "--verify", "-q", "refs/remotes/origin/main"],
    )? {
        return Ok(None);
    }
    git.text(
        Some(target),
        ["checkout", "-q", "-B", "main", "refs/remotes/origin/main"],
    )?;
    Ok(Some(
        git.text(Some(target), ["rev-parse", "HEAD"])?
            .trim()
            .to_string(),
    ))
}

/// The folder named after `owner/repo`, or `fallback` when the name the API sent is not one plain folder name
/// (`..`, `\`, a drive prefix, a hidden name): the clone must land in the current directory.
fn folder_for<'a>(repository: &'a str, fallback: &'a str) -> &'a str {
    repository
        .rsplit('/')
        .next()
        .filter(|name| crate::paths::is_valid_local_dir(name))
        .unwrap_or(fallback)
}

/// `speq clone <repo> [dir]`: a Git repository of the project's specs, wired to the Speq API for fetch and push.
/// `dir` defaults to `specs` inside an implementation repository (ignored there), else to the repository name.
pub async fn clone(
    env: &Env,
    git: &Git,
    cwd: &Path,
    options: CloneOptions<'_>,
) -> Result<CloneReport> {
    let session = Session::new(env);
    let auth = session.authenticate().await?;
    // The list comes from the API (via the snapshot); it never lets a user pick an unlisted repository.
    let snapshot = session.snapshot().await?;
    let project = resolve_repo(&snapshot.projects, options.repo)?.clone();
    let me = env.client.me(&auth.token).await?;
    let repository = project.repository_full_name.clone().unwrap_or_default();

    let outer = enclosing_work_tree(git, cwd)?;
    let dir = match options.dir {
        Some(dir) => dir.to_string(),
        None if outer.is_some() => options.default_dir.to_string(),
        None => folder_for(&repository, options.default_dir).to_string(),
    };
    let target = cwd.join(&dir);
    let created = !target.exists();
    if !created
        && (!target.is_dir()
            || std::fs::read_dir(&target)
                .ctx(format!("read {}", target.display()))?
                .next()
                .is_some())
    {
        return Err(Error::invalid(format!(
            "{dir} already exists and is not an empty directory"
        )));
    }

    let exe = std::env::current_exe().ctx("locate the speq executable")?;
    let missing = links::ensure(&exe);
    if !missing.is_empty() {
        return Err(Error::invalid(format!(
            "Git needs {} next to speq; reinstall speq or ask your package manager to add them",
            missing
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(" and ")
        )));
    }

    let head = match prepare(git, &target, &env.api_url, &project.project_id, &me) {
        Ok(head) => head,
        Err(error) => {
            // Like `git clone`: a failed clone leaves nothing behind.
            if created {
                let _ = std::fs::remove_dir_all(&target);
            } else {
                let _ = std::fs::remove_dir_all(target.join(".git"));
            }
            return Err(error);
        }
    };
    let ignored_in = match outer {
        Some(outer) => ignore_in(&outer, &target)?.map(|p| p.display().to_string()),
        None => None,
    };
    Ok(CloneReport {
        project_id: project.project_id,
        repository,
        path: dir,
        head_sha: head,
        author: format!("{} <{}>", me.name.unwrap_or_default(), me.email)
            .trim()
            .to_string(),
        ignored_in,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Capabilities, RuleSet};

    fn project(id: &str, display: &str, repo: Option<&str>) -> ProjectSummary {
        let rules = RuleSet {
            allow: vec![],
            deny: vec![],
        };
        ProjectSummary {
            project_id: id.into(),
            display_name: display.into(),
            repository_full_name: repo.map(Into::into),
            branch: "main".into(),
            roles: vec![],
            capabilities: Capabilities {
                create: rules.clone(),
                update: rules,
                can_delete: false,
                can_restore: false,
                can_manage: false,
            },
        }
    }

    #[test]
    fn an_equivalent_gitignore_line_counts_as_already_ignored() {
        for existing in [
            "specs/",
            "specs",
            "/specs",
            "/specs/",
            "# Speq\nspecs/\n.speq/",
        ] {
            assert!(already_ignored(existing, "specs"), "{existing:?}");
        }
        assert!(
            already_ignored("specs/", "docs/specs"),
            "a bare name matches at any depth"
        );
        assert!(already_ignored("/docs/specs/", "docs/specs"));
        for existing in [
            "",
            "/target",
            "/specs-old/",
            "/other/specs/",
            "# specs",
            "!specs/",
        ] {
            assert!(!already_ignored(existing, "specs"), "{existing:?}");
        }
        assert!(
            !already_ignored("/specs/", "docs/specs"),
            "anchored at the root only"
        );
    }

    #[test]
    fn the_entry_is_the_folder_relative_to_the_gitignore() {
        let ignored = |existing: Option<&str>, folder: &str| {
            let outer = tempfile::tempdir().unwrap();
            let gitignore = outer.path().join(".gitignore");
            if let Some(existing) = existing {
                std::fs::write(&gitignore, existing).unwrap();
            }
            std::fs::create_dir_all(outer.path().join(folder)).unwrap();
            let written = ignore_in(outer.path(), &outer.path().join(folder))
                .unwrap()
                .unwrap();
            assert_eq!(
                std::fs::canonicalize(written).unwrap(),
                std::fs::canonicalize(&gitignore).unwrap()
            );
            std::fs::read_to_string(gitignore).unwrap()
        };
        let block =
            |entry: &str| format!("# Speq specs repository (managed with `speq`)\n{entry}\n");

        assert_eq!(ignored(None, "specs"), block("specs/"));
        assert_eq!(
            ignored(Some("/target"), "specs"),
            format!("/target\n\n{}", block("specs/"))
        );
        assert_eq!(ignored(None, "docs/specs"), block("docs/specs/"));
        // Listed already: the file is left as it is.
        assert_eq!(ignored(Some("/specs/\n"), "specs"), "/specs/\n");
        assert_eq!(ignored(Some("specs/\n"), "docs/specs"), "specs/\n");
    }

    #[test]
    fn the_default_folder_is_one_plain_name_inside_the_current_directory() {
        assert_eq!(folder_for("acme/payment-specs", "specs"), "payment-specs");
        assert_eq!(folder_for("payment_specs.v2", "specs"), "payment_specs.v2");
        for unsafe_name in [
            "",
            "acme/",
            "acme/..",
            "acme/.",
            "acme/.git",
            r"acme/..\..\escape",
            r"acme/C:\Users\me\AppData",
            "acme/C:escape",
            "acme/spec specs",
            "acme/specs\u{1b}[2J",
        ] {
            assert_eq!(folder_for(unsafe_name, "specs"), "specs", "{unsafe_name:?}");
        }
    }

    #[test]
    fn short_names_must_be_unique() {
        let projects = vec![
            project("1", "Payment", Some("acme/payment-specs")),
            project("2", "Payment", Some("other/payment-specs")),
            project("3", "Cart", Some("acme/cart-specs")),
            project("4", "Unbound", None),
        ];
        assert_eq!(
            resolve_repo(&projects, "cart-specs").unwrap().project_id,
            "3"
        );
        assert_eq!(
            resolve_repo(&projects, "Other/Payment-Specs")
                .unwrap()
                .project_id,
            "2"
        );
        let ambiguous = resolve_repo(&projects, "payment-specs")
            .unwrap_err()
            .to_string();
        assert!(
            ambiguous.contains("acme/payment-specs") && ambiguous.contains("other/payment-specs")
        );
        assert!(resolve_repo(&projects, "missing").is_err());
    }

    #[test]
    fn display_name_resolves_when_unique() {
        let projects = vec![
            project("1", "Engauge", Some("acme/engauge-specs")),
            project("2", "Payment", Some("acme/payment-specs")),
            project("3", "Payment", Some("other/payment-specs")),
            project("4", "Orphan", None),
        ];
        assert_eq!(resolve_repo(&projects, "engauge").unwrap().project_id, "1");
        let ambiguous = resolve_repo(&projects, "Payment").unwrap_err().to_string();
        assert!(
            ambiguous.contains("acme/payment-specs") && ambiguous.contains("other/payment-specs")
        );
        // A project without a repository cannot be initialized, even by display name.
        assert!(resolve_repo(&projects, "Orphan").is_err());
    }
}
