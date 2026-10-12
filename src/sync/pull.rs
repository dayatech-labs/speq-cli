//! `speq pull` and `speq update`: apply the remote manifest without ever overwriting local work.
use super::markers;
use super::plan::{Change, State, classify};
use super::{PULL_ATTEMPTS, remote_shas, require_safe, settle};
use crate::api::SyncManifest;
use crate::env::Env;
use crate::error::{Error, Result};
use crate::hashing::git_blob_sha;
use crate::session::{Authed, Session};
use crate::workspace::{BaseEntry, LocalScan, LockState, Manifest, Side, Workspace};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use std::collections::BTreeMap;
use tokio::task::JoinSet;

const DOWNLOAD_CONCURRENCY: usize = 8;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Conflict {
    pub path: String,
    pub reason: String,
    /// The file holds conflict markers to resolve in an editor, then `speq pull` again.
    pub markers: bool,
}

#[derive(Debug, Serialize)]
pub struct PullReport {
    pub head_sha: String,
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub deleted: Vec<String>,
    /// Local content already equalled the remote: only the baseline was recorded.
    pub adopted: Vec<String>,
    /// Both sides changed different lines: the changes were combined into your file, which is now
    /// a local change on top of the remote.
    pub merged: Vec<String>,
    /// Conflicts whose markers were edited away: your file is kept as a local change on the remote.
    pub resolved: Vec<String>,
    pub conflicts: Vec<Conflict>,
    pub unchanged: usize,
    /// True only when every path was reconciled without conflict.
    pub lock_advanced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Add,
    Update,
    Delete,
    /// Local file equals the remote; record the baseline without touching the file.
    Adopt,
    /// Both sides removed the file; forget the baseline.
    Forget,
    /// A conflict whose markers were edited away: the remote becomes the baseline and the
    /// file stays as a local change on top of it.
    Resolve,
}

/// A conflicting file to merge line by line, with the local bytes at planning time.
#[derive(Debug)]
struct Mark {
    path: String,
    local: Vec<u8>,
}

#[derive(Debug, Default)]
struct Plan {
    actions: Vec<(String, Action)>,
    conflicts: Vec<Conflict>,
    /// Conflicts waiting for the remote content to be merged; see [`merge_marks`].
    marks: Vec<Mark>,
    /// Merge results without any conflicting region.
    merged: Vec<(String, Vec<u8>)>,
    /// Merge results with markers around the regions both sides changed.
    marked: Vec<Marked>,
    unchanged: usize,
}

/// A file rewritten with conflict markers, and your version from before the rewrite.
#[derive(Debug)]
struct Marked {
    path: String,
    merged: Vec<u8>,
    local: Vec<u8>,
}

impl Plan {
    /// Paths whose remote content must be downloaded: what is applied or merged, and the remote
    /// side of conflicts without markers (kept for `speq resolve --theirs`).
    fn wanted(&self, remote: &BTreeMap<String, String>) -> BTreeMap<String, String> {
        self.actions
            .iter()
            .filter(|(_, a)| matches!(a, Action::Add | Action::Update | Action::Resolve))
            .map(|(p, _)| p)
            .chain(self.marks.iter().map(|m| &m.path))
            .chain(
                self.conflicts
                    .iter()
                    .filter(|c| !c.markers)
                    .map(|c| &c.path),
            )
            .filter_map(|p| remote.get(p).map(|sha| (p.clone(), sha.clone())))
            .collect()
    }
}

fn conflict_reason(local: Change, remote: Change) -> &'static str {
    match (local, remote) {
        (Change::Added, Change::Added) => {
            "the remote added a file where an untracked local file exists"
        }
        (Change::Modified, Change::Deleted) => "deleted on the remote but changed locally",
        (Change::Deleted, Change::Modified) => "changed on the remote but deleted locally",
        _ => "changed both locally and on the remote",
    }
}

fn build_plan(
    remote: &BTreeMap<String, String>,
    manifest: &Manifest,
    local: &BTreeMap<String, String>,
) -> Plan {
    let mut paths: Vec<&String> = remote
        .keys()
        .chain(manifest.files.keys())
        .chain(local.keys())
        .collect();
    paths.sort();
    paths.dedup();
    let mut plan = Plan::default();
    for path in paths {
        let r = remote.get(path).map(String::as_str);
        let b = manifest.files.get(path).map(|e| e.sha.as_str());
        let l = local.get(path).map(String::as_str);
        let verdict = classify(r, b, l);
        if verdict.converged {
            plan.actions.push((
                path.clone(),
                if r.is_some() {
                    Action::Adopt
                } else {
                    Action::Forget
                },
            ));
            continue;
        }
        match verdict.state {
            State::RemoteOnly => {
                let action = match verdict.remote {
                    Change::Added => Action::Add,
                    Change::Modified => Action::Update,
                    _ => Action::Delete,
                };
                plan.actions.push((path.clone(), action));
            }
            State::Conflict => plan.conflicts.push(Conflict {
                path: path.clone(),
                reason: conflict_reason(verdict.local, verdict.remote).into(),
                markers: false,
            }),
            _ => plan.unchanged += 1,
        }
    }
    plan
}

/// Conflicts where both sides hold content go to the editor instead of staying opaque:
/// * markers still in the file: leave it exactly as it is, never wrap markers in markers;
/// * markers edited away while the remote is the one they were written against: the edit is
///   the resolution, so the remote becomes the baseline and the file stays a local change;
/// * otherwise: merge line by line once the remote is downloaded (a remote that moved since
///   the markers were written, or a first conflict).
///
/// A delete on either side has nothing to mark and stays a plain conflict.
fn triage(
    ws: &Workspace,
    lock: &LockState,
    remote: &BTreeMap<String, String>,
    scan: &LocalScan,
    plan: &mut Plan,
) -> Result<()> {
    let mut open = Vec::new();
    for mut conflict in std::mem::take(&mut plan.conflicts) {
        let path = conflict.path.clone();
        let (Some(remote_sha), Some(local_sha)) = (remote.get(&path), scan.files.get(&path)) else {
            open.push(conflict);
            continue;
        };
        let bytes = ws
            .read_local(&path)?
            .filter(|bytes| git_blob_sha(bytes) == *local_sha)
            .ok_or_else(|| {
                Error::Other(format!(
                    "{path} changed while pulling; run `speq pull` again"
                ))
            })?;
        if super::still_marked(ws, lock, &path, &bytes)? {
            conflict.markers = true;
            open.push(conflict);
        } else if lock.marked.get(&path) == Some(remote_sha) {
            plan.actions.push((path, Action::Resolve));
        } else {
            conflict.markers = true;
            plan.marks.push(Mark { path, local: bytes });
            open.push(conflict);
        }
    }
    plan.conflicts = open;
    Ok(())
}

/// Merge every pending conflict line by line. Changes to different lines combine into a file
/// that is no conflict at all; only regions both sides changed get markers.
fn merge_marks(ws: &Workspace, plan: &mut Plan, blobs: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    for mark in std::mem::take(&mut plan.marks) {
        let base = ws.read_base(&mark.path)?.unwrap_or_default();
        match markers::merge(&mark.local, &base, &blobs[&mark.path]) {
            markers::Merge::Clean(bytes) => {
                plan.conflicts.retain(|c| c.path != mark.path);
                plan.merged.push((mark.path, bytes));
            }
            markers::Merge::Conflicted(merged) => plan.marked.push(Marked {
                path: mark.path,
                merged,
                local: mark.local,
            }),
        }
    }
    Ok(())
}

enum Fetch {
    Blobs(BTreeMap<String, Vec<u8>>),
    /// The head moved or a blob did not match the manifest: start over from a new manifest.
    Retry,
}

/// Download every path at one pinned head and verify each blob against the manifest.
async fn fetch_blobs(
    env: &Env,
    token: &SecretString,
    project_id: &str,
    head: &str,
    wanted: &BTreeMap<String, String>,
) -> Result<Fetch> {
    let mut blobs = BTreeMap::new();
    let mut queue = wanted.iter();
    let mut tasks: JoinSet<Result<(String, Option<Vec<u8>>)>> = JoinSet::new();
    loop {
        while tasks.len() < DOWNLOAD_CONCURRENCY {
            let Some((path, sha)) = queue.next() else {
                break;
            };
            let (client, token, project, head) = (
                env.client.clone(),
                SecretString::from(token.expose_secret().to_owned()),
                project_id.to_owned(),
                head.to_owned(),
            );
            let (path, sha) = (path.clone(), sha.clone());
            tasks.spawn(async move {
                match client.document(&token, &project, &path, Some(&head)).await {
                    Ok(doc) => {
                        let bytes = doc.content.into_bytes();
                        let ok = doc.path == path
                            && doc.head_sha == head
                            && git_blob_sha(&bytes) == sha
                            && doc.document_sha == sha;
                        Ok((path, ok.then_some(bytes)))
                    }
                    Err(Error::Api(e)) if e.status == 409 && e.code == "head_changed" => {
                        Ok((path, None))
                    }
                    Err(e) => Err(e),
                }
            });
        }
        let Some(joined) = tasks.join_next().await else {
            break;
        };
        match joined.map_err(|e| Error::Other(format!("download task failed: {e}")))?? {
            (path, Some(bytes)) => {
                blobs.insert(path, bytes);
            }
            (_, None) => {
                tasks.abort_all();
                return Ok(Fetch::Retry);
            }
        }
    }
    Ok(Fetch::Blobs(blobs))
}

pub async fn pull(env: &Env, ws: &Workspace) -> Result<PullReport> {
    let _guard = ws.guard(env, false)?;
    let session = Session::new(env);
    let auth = session.authenticate().await?;
    let result = pull_inner(env, ws, &auth).await;
    settle(&session, &auth, result).await
}

async fn pull_inner(env: &Env, ws: &Workspace, auth: &Authed) -> Result<PullReport> {
    let scan = ws.scan()?;
    require_safe(&scan)?;
    let mut manifest = ws.load_manifest()?;
    let lock = ws.load_lock()?;

    for _ in 0..PULL_ATTEMPTS {
        let remote: SyncManifest = env.client.sync(&auth.token, ws.project_id()).await?;
        let remote_paths = remote_shas(&remote)?;
        let mut plan = build_plan(&remote_paths, &manifest, &scan.files);
        triage(ws, &lock, &remote_paths, &scan, &mut plan)?;
        for path in plan
            .actions
            .iter()
            .map(|(p, _)| p)
            .chain(plan.marks.iter().map(|m| &m.path))
        {
            ws.check_target(path)?;
        }

        let wanted = plan.wanted(&remote_paths);
        let blobs = match fetch_blobs(env, &auth.token, ws.project_id(), &remote.head_sha, &wanted)
            .await?
        {
            Fetch::Blobs(blobs) => blobs,
            Fetch::Retry => continue,
        };
        merge_marks(ws, &mut plan, &blobs)?;
        return apply(ws, &mut manifest, &remote, &remote_paths, plan, blobs);
    }
    Err(Error::Other(
        "the remote kept changing while pulling; nothing was changed locally, try again".into(),
    ))
}

fn apply(
    ws: &Workspace,
    manifest: &mut Manifest,
    remote: &SyncManifest,
    remote_paths: &BTreeMap<String, String>,
    plan: Plan,
    blobs: BTreeMap<String, Vec<u8>>,
) -> Result<PullReport> {
    let mut report = PullReport {
        head_sha: remote.head_sha.clone(),
        added: vec![],
        updated: vec![],
        deleted: vec![],
        adopted: vec![],
        merged: vec![],
        resolved: vec![],
        conflicts: plan.conflicts.clone(),
        unchanged: plan.unchanged,
        lock_advanced: false,
    };
    let sizes: BTreeMap<&str, u64> = remote
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.size))
        .collect();

    let previous = ws.load_lock()?;
    let mut marked_at = BTreeMap::new();
    let outcome = (|| -> Result<()> {
        for (path, action) in &plan.actions {
            match action {
                Action::Add | Action::Update => {
                    let bytes = &blobs[path];
                    // Baseline first: a crash between the two writes is repaired by the next pull.
                    ws.write_base(path, bytes)?;
                    ws.write_local(path, bytes)?;
                    manifest.files.insert(
                        path.clone(),
                        BaseEntry {
                            sha: remote_paths[path].clone(),
                            size: bytes.len() as u64,
                        },
                    );
                    if *action == Action::Add {
                        report.added.push(path.clone())
                    } else {
                        report.updated.push(path.clone())
                    }
                }
                Action::Delete => {
                    ws.remove_local(path)?;
                    ws.remove_base(path)?;
                    manifest.files.remove(path);
                    report.deleted.push(path.clone());
                }
                Action::Adopt => {
                    let bytes = ws
                        .read_local(path)?
                        .ok_or_else(|| Error::Other(format!("{path} disappeared while pulling")))?;
                    if git_blob_sha(&bytes) != remote_paths[path] {
                        return Err(Error::Other(format!(
                            "{path} changed while pulling; run `speq pull` again"
                        )));
                    }
                    ws.write_base(path, &bytes)?;
                    manifest.files.insert(
                        path.clone(),
                        BaseEntry {
                            sha: remote_paths[path].clone(),
                            size: sizes
                                .get(path.as_str())
                                .copied()
                                .unwrap_or(bytes.len() as u64),
                        },
                    );
                    report.adopted.push(path.clone());
                }
                Action::Forget => {
                    ws.remove_base(path)?;
                    manifest.files.remove(path);
                }
                Action::Resolve => {
                    let bytes = &blobs[path];
                    ws.write_base(path, bytes)?;
                    manifest.files.insert(
                        path.clone(),
                        BaseEntry {
                            sha: remote_paths[path].clone(),
                            size: bytes.len() as u64,
                        },
                    );
                    report.resolved.push(path.clone());
                }
            }
        }
        for (path, bytes) in &plan.merged {
            // The merged file first: a crash before the baseline moves leaves a file that already
            // holds the remote's changes, which the next pull merges to the same result.
            ws.write_local(path, bytes)?;
            let remote_bytes = &blobs[path];
            ws.write_base(path, remote_bytes)?;
            manifest.files.insert(
                path.clone(),
                BaseEntry {
                    sha: remote_paths[path].clone(),
                    size: remote_bytes.len() as u64,
                },
            );
            report.merged.push(path.clone());
        }
        for marked in &plan.marked {
            let path = &marked.path;
            // Keep both sides before the file is rewritten: `speq resolve` can restore either.
            ws.set_conflict(Side::Ours, path, Some(&marked.local))?;
            ws.set_conflict(Side::Theirs, path, Some(&blobs[path]))?;
            ws.write_local(path, &marked.merged)?;
            marked_at.insert(path.clone(), remote_paths[path].clone());
        }
        // Conflicts without markers (a delete on one side) keep both versions as they stand now.
        for conflict in plan.conflicts.iter().filter(|c| !c.markers) {
            let path = &conflict.path;
            // Your version is only ever replaced by a newer one. A deleted file, or one that still
            // has markers (the remote deleted it after they were written), is not your version, so
            // what was saved before the merge stays.
            if let Some(local) = ws.read_local(path)?
                && !super::still_marked(ws, &previous, path, &local)?
            {
                ws.set_conflict(Side::Ours, path, Some(&local))?;
            }
            ws.set_conflict(Side::Theirs, path, blobs.get(path).map(Vec::as_slice))?;
        }
        Ok(())
    })();

    // Persist what was really applied even after a failure, so the state never claims more.
    let saved = ws.save_manifest(manifest);
    outcome?;
    saved?;

    // The lock moves last, and only when the whole manifest reconciled cleanly.
    if report.conflicts.is_empty() {
        ws.save_lock(&LockState {
            head_sha: Some(remote.head_sha.clone()),
            ..LockState::default()
        })?;
        report.lock_advanced = true;
    } else {
        let mut lock = previous.clone();
        lock.pending = report.conflicts.iter().map(|c| c.path.clone()).collect();
        // Markers still open keep the remote they were written against; new ones replace it.
        lock.marked.extend(marked_at);
        // A conflict that lost its markers (the remote deleted the file meanwhile) is no longer one.
        lock.marked.retain(|path, _| {
            report
                .conflicts
                .iter()
                .any(|c| c.markers && &c.path == path)
        });
        // Markers left untouched while the remote moved on were written against an older remote
        // version: resolving them cannot bring the workspace to this head, so `speq pull` must.
        let stale = report.conflicts.iter().any(|c| {
            c.markers
                && !plan.marked.iter().any(|m| m.path == c.path)
                && previous.marked.get(&c.path) != remote_paths.get(&c.path)
        });
        lock.pending_head = (!stale).then(|| remote.head_sha.clone());
        ws.save_lock(&lock)?;
    }
    // Conflicts that are gone no longer need their saved versions.
    for path in previous
        .pending
        .iter()
        .filter(|p| !report.conflicts.iter().any(|c| &c.path == *p))
    {
        ws.forget_conflict(path)?;
    }
    Ok(report)
}

#[derive(Debug, Serialize)]
pub struct UpdateReport {
    pub path: String,
    pub result: &'static str,
    pub head_sha: String,
}

/// Safe-apply one file: refuse when the local copy changed. There is no force option.
pub async fn update(env: &Env, ws: &Workspace, path: &str) -> Result<UpdateReport> {
    ws.check_target(path)?;
    let _guard = ws.guard(env, false)?;
    let session = Session::new(env);
    let auth = session.authenticate().await?;
    let result = update_inner(env, ws, &auth, path).await;
    settle(&session, &auth, result).await
}

async fn update_inner(
    env: &Env,
    ws: &Workspace,
    auth: &Authed,
    path: &str,
) -> Result<UpdateReport> {
    let scan = ws.scan()?;
    require_safe(&scan)?;
    let mut manifest = ws.load_manifest()?;
    for _ in 0..PULL_ATTEMPTS {
        let remote = env.client.sync(&auth.token, ws.project_id()).await?;
        let remote_paths = remote_shas(&remote)?;
        let r = remote_paths.get(path).map(String::as_str);
        let b = manifest.files.get(path).map(|e| e.sha.clone());
        let l = scan.files.get(path).map(String::as_str);
        let verdict = classify(r, b.as_deref(), l);

        let done = |result: &'static str| UpdateReport {
            path: path.to_string(),
            result,
            head_sha: remote.head_sha.clone(),
        };
        if verdict.state == State::Conflict || verdict.local != Change::None && !verdict.converged {
            return Err(Error::conflict(format!(
                "{path} has local changes; update refuses to overwrite them (see `speq diff {path}`)"
            )));
        }
        if r.is_none() && b.is_none() && l.is_none() {
            return Err(Error::invalid(format!(
                "{path} does not exist locally or on the remote"
            )));
        }
        match (verdict.converged, verdict.remote) {
            (true, _) if r.is_some() => {
                let bytes = ws
                    .read_local(path)?
                    .ok_or_else(|| Error::Other(format!("{path} disappeared")))?;
                ws.write_base(path, &bytes)?;
                manifest.files.insert(
                    path.to_string(),
                    BaseEntry {
                        sha: remote_paths[path].clone(),
                        size: bytes.len() as u64,
                    },
                );
            }
            (true, _) => {
                ws.remove_base(path)?;
                manifest.files.remove(path);
            }
            (false, Change::None) => return Ok(done("up-to-date")),
            (false, Change::Deleted) => {
                ws.remove_local(path)?;
                ws.remove_base(path)?;
                manifest.files.remove(path);
            }
            (false, _) => {
                let wanted = BTreeMap::from([(path.to_string(), remote_paths[path].clone())]);
                let Fetch::Blobs(mut blobs) =
                    fetch_blobs(env, &auth.token, ws.project_id(), &remote.head_sha, &wanted)
                        .await?
                else {
                    continue;
                };
                let bytes = blobs.remove(path).expect("fetched path");
                ws.write_base(path, &bytes)?;
                ws.write_local(path, &bytes)?;
                manifest.files.insert(
                    path.to_string(),
                    BaseEntry {
                        sha: remote_paths[path].clone(),
                        size: bytes.len() as u64,
                    },
                );
            }
        }
        ws.save_manifest(&manifest)?;
        // Only a full pull may move the lock; resolving one path just clears its pending marker.
        let mut lock = ws.load_lock()?;
        if lock.pending.iter().any(|p| p == path) {
            lock.pending.retain(|p| p != path);
            ws.save_lock(&lock)?;
        }
        return Ok(done("updated"));
    }
    Err(Error::Other(
        "the remote kept changing; nothing was changed locally, try again".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(sha: &str) -> BaseEntry {
        BaseEntry {
            sha: sha.into(),
            size: 1,
        }
    }

    #[test]
    fn plan_applies_remote_only_and_flags_conflicts() {
        let remote: BTreeMap<String, String> = [
            ("new.md", "n"),
            ("edit.md", "e2"),
            ("both.md", "b2"),
            ("kept.md", "k"),
        ]
        .iter()
        .map(|(p, s)| (p.to_string(), s.to_string()))
        .collect();
        let manifest = Manifest {
            files: [
                ("edit.md", "e1"),
                ("both.md", "b1"),
                ("kept.md", "k"),
                ("gone.md", "g"),
            ]
            .iter()
            .map(|(p, s)| (p.to_string(), entry(s)))
            .collect(),
        };
        let local: BTreeMap<String, String> = [
            ("edit.md", "e1"),
            ("both.md", "bX"),
            ("kept.md", "kX"),
            ("gone.md", "g"),
            ("mine.md", "m"),
        ]
        .iter()
        .map(|(p, s)| (p.to_string(), s.to_string()))
        .collect();
        let plan = build_plan(&remote, &manifest, &local);
        let get = |p: &str| plan.actions.iter().find(|(x, _)| x == p).map(|(_, a)| *a);
        assert_eq!(get("new.md"), Some(Action::Add));
        assert_eq!(get("edit.md"), Some(Action::Update));
        assert_eq!(get("gone.md"), Some(Action::Delete));
        assert_eq!(get("kept.md"), None); // local-only change is kept
        assert_eq!(get("mine.md"), None);
        assert_eq!(
            plan.conflicts
                .iter()
                .map(|c| c.path.as_str())
                .collect::<Vec<_>>(),
            vec!["both.md"]
        );
    }
}
