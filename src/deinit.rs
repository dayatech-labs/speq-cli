//! `speq deinit`: the inverse of `speq clone`. Deletes the specs repository (the whole folder, `.git` included) so
//! the next `speq clone` starts from scratch. It also removes a workspace of the older, pre-Git speq (`.speq/` and
//! its specs folder). The login and the implementation repository's `.gitignore` entry are left in place.
use crate::error::{Error, IoContext, Result};
use crate::git::Git;
use crate::paths::is_valid_local_dir;
use crate::repo::{self, SpecsRepo};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// What `deinit` would delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A repository made by `speq clone`.
    Repo(SpecsRepo),
    /// A workspace of the older speq: `root/.speq/` and `root/<specs>`.
    Legacy {
        root: PathBuf,
        specs: Option<PathBuf>,
    },
}

/// Work that exists only in this folder and is gone after `deinit`.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct AtRisk {
    /// Changed, staged, or new files.
    pub uncommitted: usize,
    /// Commits on any local branch that are not on the remote.
    pub unpushed: usize,
    pub stashes: usize,
}

impl AtRisk {
    pub fn is_empty(&self) -> bool {
        *self == AtRisk::default()
    }

    /// `2 uncommitted changes, 1 unpushed commit`; empty when nothing would be lost.
    pub fn describe(&self) -> String {
        let plural =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        [
            (
                self.uncommitted,
                "uncommitted change",
                "uncommitted changes",
            ),
            (self.unpushed, "unpushed commit", "unpushed commits"),
            (self.stashes, "stash", "stashes"),
        ]
        .iter()
        .filter(|(n, _, _)| *n > 0)
        .map(|(n, one, many)| plural(*n, one, many))
        .collect::<Vec<_>>()
        .join(", ")
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct DeinitReport {
    /// The folder(s) removed.
    pub removed: Vec<String>,
    /// What was lost with them; None for an older workspace, where it cannot be told.
    pub lost: Option<AtRisk>,
    /// The command ran from inside a removed folder: the shell is now in a directory that no longer exists.
    pub was_inside: bool,
    pub legacy: bool,
}

/// The specs repository `cwd` belongs to (as Git commands find it), else an older workspace above `cwd`.
pub fn find(git: &Git, cwd: &Path) -> Result<Target> {
    match SpecsRepo::discover(git, cwd) {
        Ok(repo) => Ok(Target::Repo(repo)),
        Err(error) => match repo::legacy_workspace(cwd) {
            Some(root) => Ok(Target::Legacy {
                specs: legacy_specs_dir(&root)?,
                root,
            }),
            None => Err(error),
        },
    }
}

/// The specs folder an older workspace names in `.speq/config.toml`, if it names a valid one.
fn legacy_specs_dir(root: &Path) -> Result<Option<PathBuf>> {
    #[derive(serde::Deserialize)]
    struct Config {
        spec_source: Source,
    }
    #[derive(serde::Deserialize)]
    struct Source {
        local_dir: String,
    }
    let path = root.join(repo::LEGACY_DIR).join("config.toml");
    let raw = std::fs::read_to_string(&path).ctx(format!("read {}", path.display()))?;
    let Some(dir) = toml::from_str::<Config>(&raw)
        .ok()
        .map(|c| c.spec_source.local_dir)
        .filter(|d| is_valid_local_dir(d))
    else {
        return Ok(None);
    };
    // A linked parent such as `docs -> /elsewhere` would send the delete outside the workspace.
    let mut current = root.to_path_buf();
    for part in dir.split('/') {
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(Error::invalid(format!(
                    "refusing to delete {}: it is a symbolic link (nothing was changed)",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).ctx(format!("inspect {}", current.display())),
        }
    }
    Ok(Some(current))
}

/// What deleting the repository would lose, from Git.
pub fn at_risk(git: &Git, root: &Path) -> Result<AtRisk> {
    let lines = |args: &[&str]| -> Result<usize> {
        Ok(git
            .text(Some(root), args)?
            .lines()
            .filter(|l| !l.is_empty())
            .count())
    };
    let unpushed = git
        .text(
            Some(root),
            [
                "rev-list",
                "--count",
                "--branches",
                "--not",
                "--remotes=origin",
            ],
        )?
        .trim()
        .parse()
        .unwrap_or(0);
    Ok(AtRisk {
        uncommitted: lines(&["status", "--porcelain"])?,
        unpushed,
        stashes: lines(&["stash", "list"])?,
    })
}

/// The folders a target deletes, for the confirmation prompt and the report.
pub fn folders(target: &Target) -> Vec<PathBuf> {
    match target {
        Target::Repo(repo) => vec![repo.root.clone()],
        Target::Legacy { root, specs } => specs
            .iter()
            .cloned()
            .chain([root.join(repo::LEGACY_DIR)])
            .collect(),
    }
}

/// Deletes the target. Callers confirm first: local work that was never pushed goes with it.
pub fn deinit(git: &Git, target: &Target, cwd: &Path) -> Result<DeinitReport> {
    let lost = match target {
        Target::Repo(repo) => Some(at_risk(git, &repo.root)?),
        Target::Legacy { .. } => None,
    };
    let folders = folders(target);
    let cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let inside = folders
        .iter()
        .find(|f| std::fs::canonicalize(f).is_ok_and(|f| cwd.starts_with(f)));
    let was_inside = inside.is_some();
    if let Some(folder) = inside.and_then(|f| f.parent()) {
        // A process cannot delete the directory it is in on Windows; step out first.
        std::env::set_current_dir(folder).ctx(format!("leave {}", cwd.display()))?;
    }
    // For an older workspace the specs folder goes first: if that fails, `.speq/` survives and deinit can run again.
    for folder in &folders {
        crate::fsx::remove_tree(folder)?;
    }
    Ok(DeinitReport {
        removed: folders.iter().map(|f| f.display().to_string()).collect(),
        lost,
        was_inside,
        legacy: matches!(target, Target::Legacy { .. }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_would_be_lost_is_described_in_plain_words() {
        assert_eq!(AtRisk::default().describe(), "");
        assert!(AtRisk::default().is_empty());
        let some = AtRisk {
            uncommitted: 2,
            unpushed: 1,
            stashes: 0,
        };
        assert_eq!(some.describe(), "2 uncommitted changes, 1 unpushed commit");
        assert_eq!(
            AtRisk {
                uncommitted: 0,
                unpushed: 0,
                stashes: 3
            }
            .describe(),
            "3 stashes"
        );
    }

    #[test]
    fn an_older_workspace_lists_its_specs_folder_before_speq_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".speq")).unwrap();
        std::fs::create_dir_all(dir.path().join("docs/specs")).unwrap();
        std::fs::write(
            dir.path().join(".speq/config.toml"),
            "[spec_source]\nproject_id = \"p\"\nrepository = \"a/b-specs\"\nbranch = \"main\"\nlocal_dir = \"docs/specs\"\n",
        )
        .unwrap();
        let specs = legacy_specs_dir(dir.path()).unwrap();
        assert_eq!(specs, Some(dir.path().join("docs/specs")));
        let target = Target::Legacy {
            root: dir.path().to_path_buf(),
            specs,
        };
        assert_eq!(
            folders(&target),
            [dir.path().join("docs/specs"), dir.path().join(".speq")]
        );

        // A config that escapes the workspace names no specs folder: only `.speq/` would go.
        std::fs::write(
            dir.path().join(".speq/config.toml"),
            "[spec_source]\nlocal_dir = \"../elsewhere\"\n",
        )
        .unwrap();
        assert_eq!(legacy_specs_dir(dir.path()).unwrap(), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_specs_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".speq")).unwrap();
        std::fs::write(
            dir.path().join(".speq/config.toml"),
            "[spec_source]\nlocal_dir = \"specs\"\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("specs")).unwrap();
        assert!(
            legacy_specs_dir(dir.path())
                .unwrap_err()
                .to_string()
                .contains("symbolic link")
        );
    }
}
