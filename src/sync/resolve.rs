//! `speq resolve`: finish conflicts left by `speq pull`, offline. A resolved file becomes your
//! change on top of the remote version the conflict was made against, so `speq push` can send it.
use crate::env::Env;
use crate::error::{Error, Result};
use crate::hashing::git_blob_sha;
use crate::workspace::{BaseEntry, Side, Workspace};
use serde::Serialize;

/// What a resolved file ends up holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// The file as you saved it; it must no longer contain conflict markers.
    Edited,
    /// Your version from before the merge. Discards the merge result.
    Ours,
    /// The remote version. Discards your edits to the file.
    Theirs,
}

impl Choice {
    fn as_str(self) -> &'static str {
        match self {
            Choice::Edited => "edited",
            Choice::Ours => "ours",
            Choice::Theirs => "theirs",
        }
    }
}

pub enum Target<'a> {
    All,
    Paths(&'a [String]),
}

#[derive(Debug, Serialize)]
pub struct ResolveReport {
    pub choice: &'static str,
    pub resolved: Vec<String>,
    /// Named files that still contain conflict markers, so they were left alone.
    pub still_marked: Vec<String>,
    /// Conflicts without markers (a delete on one side) that `--all` leaves for you to decide.
    pub undecided: Vec<String>,
    /// Every conflict that remains after this command.
    pub pending: Vec<String>,
    /// The lock moved to the head the conflicted pull reached: `speq push` is open.
    pub lock_advanced: bool,
}

/// One conflict ready to be applied: what the file will hold, and the remote baseline.
struct Work {
    path: String,
    /// `None` removes the file (it was deleted on the side that is kept).
    keep: Option<Vec<u8>>,
    local: Option<Vec<u8>>,
    /// `None` when the remote deleted the file: there is no baseline.
    theirs: Option<Vec<u8>>,
}

pub fn resolve(
    env: &Env,
    ws: &Workspace,
    target: Target<'_>,
    choice: Choice,
) -> Result<ResolveReport> {
    let _guard = ws.guard(env, false)?;
    let mut lock = ws.load_lock()?;
    let all = matches!(target, Target::All);
    let paths: Vec<String> = match target {
        Target::All => lock.pending.clone(),
        Target::Paths(paths) => {
            let mut named: Vec<String> = Vec::new();
            for path in paths {
                ws.check_target(path)?;
                if !lock.pending.contains(path) {
                    return Err(Error::invalid(format!(
                        "{path} has no pending conflict (see `speq status`)"
                    )));
                }
                if !named.contains(path) {
                    named.push(path.clone());
                }
            }
            named
        }
    };

    // Everything is checked before the first write, so a bad path never leaves a half-done run.
    let mut work = Vec::new();
    let mut still_marked = Vec::new();
    let mut undecided = Vec::new();
    for path in paths {
        let ours = ws.read_conflict(Side::Ours, &path)?;
        let theirs = ws.read_conflict(Side::Theirs, &path)?;
        let local = ws.read_local(&path)?;
        let saved = ours.is_some() || theirs.is_some();
        let marked_without_remote = lock.marked.contains_key(&path) && theirs.is_none();
        if !saved || marked_without_remote {
            return Err(Error::invalid(format!(
                "the saved versions of {path} are missing; run `speq pull` to record the conflict again"
            )));
        }
        let unresolved = match &local {
            Some(bytes) => super::still_marked(ws, &lock, &path, bytes)?,
            None => false,
        };
        let keep = match choice {
            Choice::Ours => ours,
            Choice::Theirs => theirs.clone(),
            Choice::Edited if unresolved => {
                still_marked.push(path);
                continue;
            }
            // Nothing was edited to resolve a delete conflict, so `--all` must not decide it.
            Choice::Edited if all && !lock.marked.contains_key(&path) => {
                undecided.push(path);
                continue;
            }
            Choice::Edited => local.clone(),
        };
        work.push(Work {
            path,
            keep,
            local,
            theirs,
        });
    }

    let mut manifest = ws.load_manifest()?;
    let mut resolved = Vec::new();
    let outcome = (|| -> Result<()> {
        for Work {
            path,
            keep,
            local,
            theirs,
        } in &work
        {
            match (keep, local) {
                (Some(bytes), local) if local.as_ref() != Some(bytes) => {
                    ws.write_local(path, bytes)?
                }
                (None, Some(_)) => ws.remove_local(path)?,
                _ => {}
            }
            // The remote version becomes the baseline, so what differs from it is your change.
            match theirs {
                Some(bytes) => {
                    ws.write_base(path, bytes)?;
                    manifest.files.insert(
                        path.clone(),
                        BaseEntry {
                            sha: git_blob_sha(bytes),
                            size: bytes.len() as u64,
                        },
                    );
                }
                None => {
                    ws.remove_base(path)?;
                    manifest.files.remove(path);
                }
            }
            resolved.push(path.clone());
        }
        Ok(())
    })();
    // Persist what was applied even after a failure; the lock below only moves on success.
    let saved = ws.save_manifest(&manifest);
    outcome?;
    saved?;

    lock.pending.retain(|p| !resolved.contains(p));
    lock.marked.retain(|p, _| lock.pending.contains(p));
    let mut lock_advanced = false;
    if lock.pending.is_empty()
        && !resolved.is_empty()
        && let Some(head) = lock.pending_head.take()
    {
        lock.head_sha = Some(head);
        lock_advanced = true;
    }
    if !resolved.is_empty() {
        ws.save_lock(&lock)?;
    }
    for path in &resolved {
        ws.forget_conflict(path)?;
    }
    Ok(ResolveReport {
        choice: choice.as_str(),
        resolved,
        still_marked,
        undecided,
        pending: lock.pending,
        lock_advanced,
    })
}
