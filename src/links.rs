//! Git runs `git-remote-speq` for `speq::` URLs and `git-credential-speq` for the credential helper `speq`. Both are
//! the speq binary under another name (it dispatches on how it was invoked), placed next to it: a symlink on Unix, a
//! copy on Windows (kept in step with the binary after upgrades).
use crate::error::{IoContext, Result};
use std::path::{Path, PathBuf};

pub const REMOTE_HELPER: &str = "git-remote-speq";
pub const CREDENTIAL_HELPER: &str = "git-credential-speq";

/// What the running binary should do, from the name it was started under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invocation {
    Speq,
    RemoteHelper,
    CredentialHelper,
}

pub fn invocation(argv0: &Path) -> Invocation {
    // Git may start helpers with either separator on Windows, and file names are case-insensitive there.
    let raw = argv0.to_string_lossy().to_lowercase();
    let file = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    let name = file.strip_suffix(".exe").unwrap_or(file);
    match name {
        REMOTE_HELPER => Invocation::RemoteHelper,
        CREDENTIAL_HELPER => Invocation::CredentialHelper,
        _ => Invocation::Speq,
    }
}

fn helper_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    })
}

pub fn helper_paths(exe: &Path) -> Vec<PathBuf> {
    let dir = exe.parent().unwrap_or(Path::new("."));
    [REMOTE_HELPER, CREDENTIAL_HELPER]
        .iter()
        .map(|name| helper_path(dir, name))
        .collect()
}

#[cfg(unix)]
fn current(helper: &Path, exe: &Path) -> bool {
    std::fs::read_link(helper)
        .is_ok_and(|target| target == exe.file_name().map(PathBuf::from).unwrap_or_default())
}

#[cfg(unix)]
fn place(helper: &Path, exe: &Path) -> Result<()> {
    let _ = std::fs::remove_file(helper);
    // Relative, so the pair survives the folder being moved.
    let target = exe.file_name().map(PathBuf::from).unwrap_or_default();
    std::os::unix::fs::symlink(&target, helper).ctx(format!("create {}", helper.display()))
}

#[cfg(windows)]
fn current(helper: &Path, exe: &Path) -> bool {
    let (Ok(h), Ok(e)) = (std::fs::metadata(helper), std::fs::metadata(exe)) else {
        return false;
    };
    h.len() == e.len() && h.modified().ok() >= e.modified().ok()
}

#[cfg(windows)]
fn place(helper: &Path, exe: &Path) -> Result<()> {
    let _ = std::fs::remove_file(helper);
    std::fs::copy(exe, helper)
        .map(|_| ())
        .ctx(format!("create {}", helper.display()))
}

/// Puts both helpers next to `exe` unless they are already there and current. Returns the helpers that are still
/// missing (for example because a package manager owns the folder and did not install them).
pub fn ensure(exe: &Path) -> Vec<PathBuf> {
    helper_paths(exe)
        .into_iter()
        .filter(|helper| {
            !current(helper, exe) && (place(helper, exe).is_err() || !current(helper, exe))
        })
        .collect()
}

/// Removes both helpers next to `exe`; a helper that is already gone is fine.
pub fn remove(exe: &Path) -> Result<()> {
    for helper in helper_paths(exe) {
        match std::fs::remove_file(&helper) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(e).ctx(format!("remove {}", helper.display()));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_binary_dispatches_on_its_name() {
        assert_eq!(
            invocation(Path::new("/usr/local/bin/speq")),
            Invocation::Speq
        );
        assert_eq!(
            invocation(Path::new("/home/a/.local/bin/git-remote-speq")),
            Invocation::RemoteHelper
        );
        assert_eq!(
            invocation(Path::new(r"C:\Speq\bin\git-credential-speq.exe")),
            Invocation::CredentialHelper
        );
        assert_eq!(
            invocation(Path::new("GIT-REMOTE-SPEQ")),
            Invocation::RemoteHelper
        );
        assert_eq!(invocation(Path::new("git-remote-other")), Invocation::Speq);
    }

    #[cfg(unix)]
    #[test]
    fn ensure_links_both_helpers_once_and_remove_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("speq");
        std::fs::write(&exe, b"binary").unwrap();

        assert!(ensure(&exe).is_empty());
        for helper in helper_paths(&exe) {
            assert_eq!(std::fs::read_link(&helper).unwrap(), PathBuf::from("speq"));
            assert_eq!(std::fs::read(&helper).unwrap(), b"binary");
        }
        // A stale helper (a regular file) is replaced.
        std::fs::remove_file(dir.path().join(REMOTE_HELPER)).unwrap();
        std::fs::write(dir.path().join(REMOTE_HELPER), b"old").unwrap();
        assert!(ensure(&exe).is_empty());
        assert!(current(&dir.path().join(REMOTE_HELPER), &exe));

        remove(&exe).unwrap();
        assert!(helper_paths(&exe).iter().all(|h| !h.exists()));
        remove(&exe).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ensure_reports_helpers_it_cannot_place() {
        let exe = Path::new("/nonexistent-speq-dir/speq");
        assert_eq!(ensure(exe).len(), 2);
    }
}
