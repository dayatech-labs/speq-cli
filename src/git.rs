//! The Git executable speq drives. Every Git command is Git's own: speq finds Git, checks its version, and runs it
//! with an environment that cannot point it at a different repository than the one speq chose.
use crate::error::{Error, Result};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// `git init -b` needs 2.28.
pub const MIN_VERSION: (u32, u32) = (2, 28);

/// Variables that would send Git to another repository, index, or object store than the one speq names.
const REPOSITORY_ENV: [&str; 8] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
];

#[derive(Debug, Clone)]
pub struct Git {
    exe: PathBuf,
    /// Put first on `PATH` for Git, so it finds `git-remote-speq` and `git-credential-speq` next to speq.
    helper_dir: Option<PathBuf>,
    /// Passed as `--git-dir`: set when Git itself named the repository (speq running as a remote helper).
    git_dir: Option<PathBuf>,
}

/// `git version 2.54.0 (Apple Git-157)` → `(2, 54)`.
pub fn parse_version(text: &str) -> Option<(u32, u32)> {
    let version = text.trim().strip_prefix("git version ")?;
    let mut parts = version.split(|c: char| !c.is_ascii_digit());
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// The portable Git the Windows installer puts next to speq when the machine has none.
fn bundled_git() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let dirs = directories::BaseDirs::new()?;
    let exe = dirs
        .data_local_dir()
        .join("Speq")
        .join("git")
        .join("cmd")
        .join("git.exe");
    exe.is_file().then_some(exe)
}

pub fn install_hint() -> &'static str {
    if cfg!(windows) {
        "Re-run the speq installer (it installs a private copy of Git), or install Git for Windows: https://git-scm.com/download/win"
    } else if cfg!(target_os = "macos") {
        "Run `xcode-select --install` (or `brew install git`), then try again"
    } else {
        "Install Git with your package manager (for example `sudo apt install git`), then try again"
    }
}

impl Git {
    /// `SPEQ_GIT` when set, else `git` on `PATH`, else (Windows) the installer's private copy. Refuses a Git older
    /// than [`MIN_VERSION`].
    pub fn locate(helper_dir: Option<PathBuf>) -> Result<Git> {
        let candidates: Vec<PathBuf> = std::env::var_os("SPEQ_GIT")
            .map(PathBuf::from)
            .into_iter()
            .chain([PathBuf::from("git")])
            .chain(bundled_git())
            .collect();
        for exe in candidates {
            let Ok(output) = Command::new(&exe)
                .arg("--version")
                .stdin(Stdio::null())
                .output()
            else {
                continue;
            };
            let found = parse_version(&String::from_utf8_lossy(&output.stdout));
            return match found {
                Some(version) if version >= MIN_VERSION => Ok(Git {
                    exe,
                    helper_dir,
                    git_dir: None,
                }),
                Some((major, minor)) => Err(Error::GitMissing(format!(
                    "Git {major}.{minor} is too old; speq needs Git {}.{} or newer.",
                    MIN_VERSION.0, MIN_VERSION.1
                ))),
                None => Err(Error::GitMissing(format!(
                    "{} did not report a Git version.",
                    exe.display()
                ))),
            };
        }
        Err(Error::GitMissing(
            "speq runs Git for you, and Git was not found on this machine.".into(),
        ))
    }

    /// The same Git, always operating on the repository at `git_dir`.
    pub fn with_git_dir(mut self, git_dir: PathBuf) -> Git {
        self.git_dir = Some(git_dir);
        self
    }

    /// A Git command in `dir` (`None`: the current directory), with repository-selecting variables removed and the
    /// helper directory first on `PATH`.
    pub fn command(&self, dir: Option<&Path>) -> Command {
        let mut command = Command::new(&self.exe);
        if let Some(dir) = dir {
            command.arg("-C").arg(dir);
        }
        if let Some(git_dir) = &self.git_dir {
            command.arg("--git-dir").arg(git_dir);
        }
        for name in REPOSITORY_ENV {
            command.env_remove(name);
        }
        if let Some(helpers) = &self.helper_dir {
            let mut path = OsString::from(helpers.as_os_str());
            if let Some(current) = std::env::var_os("PATH") {
                path.push(if cfg!(windows) { ";" } else { ":" });
                path.push(current);
            }
            command.env("PATH", path);
        }
        command
    }

    /// Runs Git with captured output; never fails on a non-zero exit (see [`Git::text`]).
    pub fn output<I, S>(&self, dir: Option<&Path>, args: I) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command(dir)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Error::Other(format!("cannot run Git: {e}")))
    }

    /// Raw stdout of a Git command that must succeed; a failure carries Git's own message.
    pub fn bytes<I, S>(&self, dir: Option<&Path>, args: I) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = self.output(dir, args)?;
        if !output.status.success() {
            return Err(Error::Other(git_failure(&output)));
        }
        Ok(output.stdout)
    }

    /// Stdout of a Git command that must succeed, as text.
    pub fn text<I, S>(&self, dir: Option<&Path>, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        String::from_utf8(self.bytes(dir, args)?)
            .map_err(|_| Error::Other("Git printed output that is not UTF-8".into()))
    }

    /// Whether a Git command exits 0 (for questions such as `merge-base --is-ancestor`).
    pub fn succeeds<I, S>(&self, dir: Option<&Path>, args: I) -> Result<bool>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Ok(self.output(dir, args)?.status.success())
    }

    /// Runs Git attached to this terminal and returns its exit code, as if the user had typed `git <args>`.
    pub fn passthrough<I, S>(&self, dir: Option<&Path>, args: I) -> Result<u8>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let status = self
            .command(dir)
            .args(args)
            .status()
            .map_err(|e| Error::Other(format!("cannot run Git: {e}")))?;
        // A Git killed by a signal has no exit code; report a plain failure.
        Ok(status
            .code()
            .and_then(|c| u8::try_from(c).ok())
            .unwrap_or(1))
    }
}

/// Git's own error line(s), for a command that failed while speq ran it.
pub fn git_failure(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim();
    if message.is_empty() {
        format!("Git exited with {}", output.status)
    } else {
        message.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_from_every_git_flavour() {
        assert_eq!(
            parse_version("git version 2.54.0 (Apple Git-157)\n"),
            Some((2, 54))
        );
        assert_eq!(parse_version("git version 2.47.1.windows.2"), Some((2, 47)));
        assert_eq!(parse_version("git version 2.28.0"), Some((2, 28)));
        assert_eq!(parse_version("hub version 2.14"), None);
        assert_eq!(parse_version("git version x"), None);
        assert!(parse_version("git version 2.27.0").unwrap() < MIN_VERSION);
    }

    #[test]
    fn commands_cannot_be_redirected_to_another_repository() {
        let git = Git {
            exe: PathBuf::from("git"),
            helper_dir: Some(PathBuf::from("/opt/speq/bin")),
            git_dir: None,
        };
        let command = git.command(Some(Path::new("/work/specs")));
        let removed: Vec<_> = command
            .get_envs()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        for name in REPOSITORY_ENV {
            assert!(removed.contains(&name.to_string()), "{name}");
        }
        let path = command
            .get_envs()
            .find(|(k, _)| *k == "PATH")
            .and_then(|(_, v)| v)
            .expect("PATH set");
        assert!(path.to_string_lossy().starts_with("/opt/speq/bin"));
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["-C", "/work/specs"]);

        let pinned = git
            .with_git_dir(PathBuf::from("/work/specs/.git"))
            .command(None);
        let args: Vec<_> = pinned.get_args().collect();
        assert_eq!(args, ["--git-dir", "/work/specs/.git"]);
    }
}
