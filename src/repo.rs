//! The specs repository: an ordinary Git repository made by `speq clone`. It fetches from the API's read-only Git
//! endpoint and pushes through `git-remote-speq`, so its push URL (`speq::<api>/v1/projects/<id>`) is what marks it.
use crate::api::parse_api_url;
use crate::error::{Error, Result};
use crate::git::Git;
use std::path::{Path, PathBuf};
use url::Url;

/// Push URLs of specs repositories start with this: Git hands the rest to `git-remote-speq`.
pub const TRANSPORT: &str = "speq::";
/// The folder `speq clone` uses inside an implementation repository, and where speq looks for it from there.
pub const DEFAULT_DIR: &str = "specs";
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
        let push = git.output(Some(&root), ["config", "--get", "remote.origin.pushurl"])?;
        let push = String::from_utf8_lossy(&push.stdout).trim().to_string();
        let Some(address) = push.strip_prefix(TRANSPORT) else {
            return Ok(None);
        };
        let (api_url, project_id) = parse_address(address)?;
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
}
