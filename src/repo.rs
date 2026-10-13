//! The specs repository: an ordinary Git repository made by `speq clone`. It fetches from the API's read-only Git
//! endpoint and pushes through `git-remote-speq` (push URL `speq::<api>/v1/projects/<id>`). What marks it is
//! `speq.address`, not `origin`: a user may change or remove `origin` like in any Git repository, and speq must still
//! recognise the folder to say how to put `origin` back.
use crate::api::parse_api_url;
use crate::error::{Error, Result};
use crate::git::Git;
use std::path::{Path, PathBuf};
use url::Url;

/// Push URLs of specs repositories start with this: Git hands the rest to `git-remote-speq`.
pub const TRANSPORT: &str = "speq::";
/// The folder `speq clone` uses inside an implementation repository, and where speq looks for it from there.
pub const DEFAULT_DIR: &str = "specs";
/// Git config key holding the repository's [`address`], written by `speq clone`.
pub const ADDRESS_KEY: &str = "speq.address";
/// Where the older, pre-Git speq kept a workspace's state.
pub const LEGACY_DIR: &str = ".speq";

/// The nearest folder at or above `start` holding a workspace of the older speq.
pub fn legacy_workspace(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(LEGACY_DIR).join("config.toml").is_file())
        .map(Path::to_path_buf)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecsRepo {
    /// Top-level directory of the working tree.
    pub root: PathBuf,
    pub api_url: Url,
    pub project_id: String,
}

fn valid_project_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `<api>/v1/projects/<id>`: what follows `speq::` in the push URL, and what `git-remote-speq` is given.
pub fn address(api_url: &Url, project_id: &str) -> String {
    format!(
        "{}/v1/projects/{project_id}",
        api_url.as_str().trim_end_matches('/')
    )
}

/// The read-only Git HTTP endpoint `git fetch` uses.
pub fn fetch_url(api_url: &Url, project_id: &str) -> String {
    format!("{}/git", address(api_url, project_id))
}

/// Splits an [`address`] into the API origin (validated like `--api-url`) and the project ID.
pub fn parse_address(address: &str) -> Result<(Url, String)> {
    let invalid = || Error::invalid(format!("{address:?} is not a Speq repository address"));
    let url = Url::parse(address).map_err(|_| invalid())?;
    let project_id = url
        .path()
        .strip_prefix("/v1/projects/")
        .filter(|id| valid_project_id(id))
        .ok_or_else(invalid)?
        .to_string();
    // `origin()` drops user info, so it is refused here rather than silently ignored.
    if url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid());
    }
    let origin = url.origin().ascii_serialization();
    Ok((parse_api_url(&origin)?, project_id))
}

impl SpecsRepo {
    /// The specs repository at `dir`'s working tree, if `dir` is inside one.
    pub fn open(git: &Git, dir: &Path) -> Result<Option<SpecsRepo>> {
        let top = git.output(Some(dir), ["rev-parse", "--show-toplevel"])?;
        if !top.status.success() {
            return Ok(None);
        }
        let root = PathBuf::from(String::from_utf8_lossy(&top.stdout).trim());
        let get = |key: &str| -> Result<String> {
            let out = git.output(Some(&root), ["config", "--get", key])?;
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        };
        let mut address = get(ADDRESS_KEY)?;
        if address.is_empty() {
            // Cloned before `speq.address` existed: the push URL was the mark then.
            match get("remote.origin.pushurl")?.strip_prefix(TRANSPORT) {
                Some(rest) => address = rest.to_string(),
                None => return Ok(None),
            }
        }
        let (api_url, project_id) = parse_address(&address)?;
        Ok(Some(SpecsRepo {
            root,
            api_url,
            project_id,
        }))
    }

    /// The repository `start` is in, else a `specs/` repository in `start` or any parent (the implementation
    /// repository around it). A pre-Git `.speq/` workspace on the way is reported with what to do instead.
    pub fn discover(git: &Git, start: &Path) -> Result<SpecsRepo> {
        if let Some(repo) = SpecsRepo::open(git, start)? {
            return Ok(repo);
        }
        for dir in start.ancestors() {
            let candidate = dir.join(DEFAULT_DIR);
            if candidate.join(".git").exists()
                && let Some(repo) = SpecsRepo::open(git, &candidate)?
            {
                return Ok(repo);
            }
            if legacy_workspace(dir).as_deref() == Some(dir) {
                return Err(Error::invalid(format!(
                    "{} is a workspace of an older speq. speq now works through Git: push anything you still need \
                     with the speq version that made it, run `speq deinit` to remove it, then `speq clone <repo>`",
                    dir.display()
                )));
            }
        }
        Err(Error::NotInitialized)
    }

    /// How to point `origin` back at this repository's project, or None when it already does. Best effort: speq
    /// runs it after every Git command, so a failure to read the config only skips the advice.
    pub fn origin_repair(&self, git: &Git) -> Option<Vec<String>> {
        let out = git
            .output(
                Some(&self.root),
                ["config", "--get-regexp", r"^remote\.origin\."],
            )
            .ok()?;
        // Exit 1 means no such keys: `origin` was removed or renamed.
        if !out.status.success() && out.status.code() != Some(1) {
            return None;
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let (mut urls, mut push_urls) = (Vec::new(), Vec::new());
        for line in stdout.lines() {
            match line.split_once(' ') {
                Some(("remote.origin.url", value)) => urls.push(value.to_string()),
                Some(("remote.origin.pushurl", value)) => push_urls.push(value.to_string()),
                _ => {}
            }
        }
        repair_commands(
            !stdout.is_empty(),
            &fetch_url(&self.api_url, &self.project_id),
            &format!("{TRANSPORT}{}", address(&self.api_url, &self.project_id)),
            &urls,
            &push_urls,
        )
    }

    /// Where Git should run for a command typed in `cwd`: `cwd` itself inside the repository (so relative paths
    /// mean what the user sees), else the repository root.
    pub fn git_dir_for(&self, cwd: &Path) -> Option<PathBuf> {
        let inside = std::fs::canonicalize(cwd)
            .ok()
            .zip(std::fs::canonicalize(&self.root).ok())
            .is_some_and(|(cwd, root)| cwd.starts_with(root));
        (!inside).then(|| self.root.clone())
    }
}

/// The speq commands that make `origin` fetch from `fetch` and push to `push` again, given whether `origin` exists
/// and its current URLs.
fn repair_commands(
    exists: bool,
    fetch: &str,
    push: &str,
    urls: &[String],
    push_urls: &[String],
) -> Option<Vec<String>> {
    if !exists {
        return Some(vec![
            format!("speq remote add -t main origin {fetch}"),
            format!("speq remote set-url --push origin {push}"),
            "speq fetch origin".to_string(),
            "speq branch -u origin/main main".to_string(),
        ]);
    }
    let mut commands = Vec::new();
    for (current, key, flag, url) in [
        (urls, "url", "", fetch),
        (push_urls, "pushurl", "--push ", push),
    ] {
        if *current != [url] {
            // `remote set-url` cannot replace several values at once, and every extra push URL would get the push.
            if current.len() > 1 {
                commands.push(format!("speq config --unset-all remote.origin.{key}"));
            }
            commands.push(format!("speq remote set-url {flag}origin {url}"));
        }
    }
    (!commands.is_empty()).then_some(commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_round_trip_and_reject_anything_else() {
        let api = parse_api_url("https://speq-api.example.com").unwrap();
        let addr = address(&api, "ea7ba883-cdad-4500-8efa-30d1397e6660");
        assert_eq!(
            addr,
            "https://speq-api.example.com/v1/projects/ea7ba883-cdad-4500-8efa-30d1397e6660"
        );
        assert_eq!(
            fetch_url(&api, "p1"),
            "https://speq-api.example.com/v1/projects/p1/git"
        );
        let (url, id) = parse_address(&addr).unwrap();
        assert_eq!(url, api);
        assert_eq!(id, "ea7ba883-cdad-4500-8efa-30d1397e6660");
        assert_eq!(
            parse_address("http://127.0.0.1:8787/v1/projects/proj_payment")
                .unwrap()
                .1,
            "proj_payment"
        );

        for bad in [
            "http://example.com/v1/projects/p", // plain http off loopback
            "https://user:pw@example.com/v1/projects/p", // credentials
            "https://example.com/v1/projects/",
            "https://example.com/v1/projects/a/b",
            "https://example.com/v1/projects/p?x=1",
            "https://example.com/other/p",
            "not a url",
        ] {
            assert!(parse_address(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn origin_repair_names_only_what_is_wrong() {
        const FETCH: &str = "https://speq.example.com/v1/projects/p/git";
        const PUSH: &str = "speq::https://speq.example.com/v1/projects/p";
        let v = |urls: &[&str]| urls.iter().map(|u| u.to_string()).collect::<Vec<_>>();
        let repair = |exists, urls: &[&str], push: &[&str]| {
            repair_commands(exists, FETCH, PUSH, &v(urls), &v(push))
        };

        assert_eq!(repair(true, &[FETCH], &[PUSH]), None);
        assert_eq!(
            repair(
                true,
                &["https://github.com/acme/payment-specs.git"],
                &[PUSH]
            )
            .unwrap(),
            [format!("speq remote set-url origin {FETCH}")]
        );
        // Without a push URL Git would push to the read-only fetch URL.
        assert_eq!(
            repair(true, &[FETCH], &[]).unwrap(),
            [format!("speq remote set-url --push origin {PUSH}")]
        );
        assert_eq!(
            repair(
                true,
                &["https://github.com/a/b.git"],
                &["speq::https://speq.example.com/v1/projects/other"]
            )
            .unwrap(),
            [
                format!("speq remote set-url origin {FETCH}"),
                format!("speq remote set-url --push origin {PUSH}"),
            ]
        );
        // An extra push URL would receive every push too, and `set-url` cannot replace several values.
        assert_eq!(
            repair(true, &[FETCH], &[PUSH, "https://github.com/a/b.git"]).unwrap(),
            [
                "speq config --unset-all remote.origin.pushurl".to_string(),
                format!("speq remote set-url --push origin {PUSH}"),
            ]
        );
        assert_eq!(
            repair(true, &[FETCH, FETCH], &[PUSH]).unwrap(),
            [
                "speq config --unset-all remote.origin.url".to_string(),
                format!("speq remote set-url origin {FETCH}"),
            ]
        );
        // Removed or renamed: added again, fetched, and tracked by main.
        assert_eq!(
            repair(false, &[], &[]).unwrap(),
            [
                format!("speq remote add -t main origin {FETCH}"),
                format!("speq remote set-url --push origin {PUSH}"),
                "speq fetch origin".to_string(),
                "speq branch -u origin/main main".to_string(),
            ]
        );
    }
}
