//! `speq deinit`: the inverse of `speq init`. Deletes this workspace's `.speq/` and its specs folder, nothing else.
use crate::env::Env;
use crate::error::{Error, IoContext, Result};
use crate::workspace::{SPEQ_DIR, Workspace};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct DeinitReport {
    /// The workspace root the folders were removed from.
    pub root: String,
    /// The specs folder (`local_dir` from the config), relative to `root`.
    pub specs_dir: String,
}

/// Local edits that were never pushed go with the folder, so callers confirm first.
/// The specs folder is removed before `.speq/`: if that fails midway the config survives and the command can be rerun.
pub fn deinit(env: &Env, ws: &Workspace) -> Result<DeinitReport> {
    let _guard = ws.guard(env, false)?;
    refuse_symlinks(ws)?;
    remove_dir(&ws.specs_dir())?;
    remove_dir(&ws.speq_dir())?;
    Ok(DeinitReport {
        root: ws.root.display().to_string(),
        specs_dir: ws.config.spec_source.local_dir.clone(),
    })
}

/// A linked parent such as `docs -> /elsewhere` would send the delete outside the workspace.
fn refuse_symlinks(ws: &Workspace) -> Result<()> {
    let mut current = ws.root.clone();
    for part in ws.config.spec_source.local_dir.split('/') {
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(Error::invalid(format!(
                    "refusing to delete {}: it is a symbolic link (nothing was changed)",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => return Err(e).ctx(format!("inspect {}", current.display())),
        }
    }
    Ok(())
}

fn remove_dir(path: &Path) -> Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(e).ctx(format!("remove {}", path.display()))
        }
        _ => Ok(()),
    }
}

/// The folders `deinit` will delete, for the confirmation prompt.
pub fn targets(ws: &Workspace) -> String {
    format!("{SPEQ_DIR}/ and {}/", ws.config.spec_source.local_dir)
}
