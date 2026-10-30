//! `speq uninstall`: remove this machine's Speq state, the agent skills and their instruction block, and, when the
//! official installer placed it,
//! the binary and its Git helpers. Specs repositories are never touched, and the server-side session is not ended
//! (`speq logout --all`). The command revokes the Dayaboard API key before this runs.
use crate::env::Env;
use crate::error::{IoContext, Result};
use crate::fsx::FileLock;
use crate::session::Session;
use crate::upgrade::Install;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct UninstallReport {
    /// False when the credential store could not be cleaned (see `manual_steps`).
    pub credential_removed: bool,
    /// The per-user data directory that was removed (cache, locks).
    pub data_dir: String,
    /// The binary that was deleted; only an official install is ours to delete.
    pub removed_binary: Option<String>,
    /// The agent skill folders that `speq skills install` had written.
    pub removed_skills: Vec<String>,
    /// The `CLAUDE.md` and `AGENTS.md` files the speq block was removed from.
    pub removed_instructions: Vec<String>,
    /// What is left for the user to do by hand.
    pub manual_steps: Vec<String>,
}

/// Everything is attempted, and what could not be done is reported instead of aborting: an uninstall that stops at
/// an unreachable credential store (headless Linux, a locked keychain) could never be completed.
pub fn uninstall(env: &Env, exe: &Path, install: &Install) -> Result<UninstallReport> {
    let mut manual_steps = Vec::new();
    let mut credential_removed = true;
    {
        let _lock = FileLock::exclusive(&env.auth_lock_path())?;
        // Tries the credential and the snapshot before it reports a failure.
        if let Err(e) = Session::new(env).clear_local() {
            credential_removed = false;
            manual_steps.push(format!(
                "the stored credential could not be fully removed ({e}); delete the speq entry from your OS credential store by hand (the token expires on its own)"
            ));
        }
    }
    match std::fs::remove_dir_all(env.dirs.root()) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(e).ctx(format!("remove {}", env.dirs.root().display()));
        }
        _ => {}
    }
    let removed_skills = crate::skills::remove(&env.skills_dirs).unwrap_or_else(|e| {
        let dirs: Vec<String> = env
            .skills_dirs
            .iter()
            .map(|d| d.display().to_string())
            .collect();
        manual_steps.push(format!(
            "the agent skills could not be removed ({e}); delete the speq-* folders in {} by hand",
            dirs.join(" and ")
        ));
        Vec::new()
    });
    let removed_instructions = crate::agent_instructions::remove(&env.instruction_files)
        .unwrap_or_else(|e| {
            manual_steps.push(format!(
                "the speq block could not be removed from the agent instructions ({e}); delete the lines from `<!-- speq:begin` to `<!-- speq:end -->` by hand"
            ));
            Vec::new()
        });

    let removed_binary = match install {
        Install::Official => {
            crate::links::remove(exe)?;
            remove_binary(exe)?;
            if cfg!(windows)
                && let Some(git) = private_git(exe)
                && let Err(e) = std::fs::remove_dir_all(&git)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                manual_steps.push(format!(
                    "delete {} (the copy of Git the installer added for speq): {e}",
                    git.display()
                ));
            }
            manual_steps.extend(official_leftovers(exe));
            Some(exe.display().to_string())
        }
        Install::Managed {
            manager, uninstall, ..
        } => {
            manual_steps.push(format!(
                "speq is managed by {manager}; run `{uninstall}` to remove the program"
            ));
            None
        }
        Install::Unmanaged => {
            manual_steps.push(format!(
                "{} was not installed by the official installer, so it was left in place; delete it yourself",
                exe.display()
            ));
            None
        }
    };
    Ok(UninstallReport {
        credential_removed,
        data_dir: env.dirs.root().display().to_string(),
        removed_binary,
        removed_skills,
        removed_instructions,
        manual_steps,
    })
}

#[cfg(not(windows))]
fn remove_binary(exe: &Path) -> Result<()> {
    // A running executable can be unlinked on Unix.
    std::fs::remove_file(exe).ctx(format!("remove {}", exe.display()))
}

/// A running `.exe` cannot be deleted on Windows, but it can be renamed away (as `upgrade` does).
#[cfg(windows)]
fn remove_binary(exe: &Path) -> Result<()> {
    let old = exe.with_extension("exe.old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).ctx(format!("move {} aside", exe.display()))
}

/// The private Git the Windows installer puts next to `<...>\Speq\bin\speq.exe`, at `<...>\Speq\git`.
pub fn private_git(exe: &Path) -> Option<std::path::PathBuf> {
    let bin = exe.parent()?;
    if bin.file_name()? != "bin" {
        return None;
    }
    Some(bin.parent()?.join("git"))
}

/// The installer edits the shell profile (Unix) or the user PATH (Windows); undoing that is the user's call.
fn official_leftovers(exe: &Path) -> Vec<String> {
    let dir = exe.parent().map(|p| p.display().to_string());
    if cfg!(windows) {
        let mut steps = vec![format!(
            "delete {} once this command has exited (a running program cannot delete itself on Windows)",
            exe.with_extension("exe.old").display()
        )];
        steps.extend(dir.map(|d| {
            format!("{d} may still be on your user PATH; remove it in Environment Variables if nothing else lives there")
        }));
        steps
    } else {
        vec![
            "the installer may have added ~/.local/bin to your PATH with a `# added by the speq installer` line in your shell profile (~/.zshrc, ~/.bashrc, ~/.bash_profile, or ~/.profile); remove it if nothing else lives there"
                .into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_private_git_sits_next_to_the_installers_bin_folder() {
        assert_eq!(
            private_git(Path::new("/home/a/AppData/Local/Speq/bin/speq.exe")),
            Some(Path::new("/home/a/AppData/Local/Speq/git").to_path_buf())
        );
        assert_eq!(private_git(Path::new("/usr/local/speq")), None);
    }
}
