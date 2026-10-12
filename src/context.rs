//! `speq context <epic>/<story>`: a deterministic list of paths from the local working copy
//! (the epic and its stories live under `epics/<epic>/` in the repository).
use crate::error::{Error, Result};
use crate::paths::{DocumentKind, STORY_FILES, document_kind};
use crate::workspace::{LocalScan, Workspace, ensure_valid_epic_story};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ContextReport {
    pub project_id: String,
    pub head_sha: String,
    pub paths: Vec<String>,
}

/// The documents of one kind directly inside `dir` (a canonical folder path ending in `/`), by name.
/// They come from the working-copy scan, which never follows symlinks and skips non-canonical files.
fn in_dir(scan: &LocalScan, dir: &str, kind: DocumentKind) -> Vec<String> {
    scan.files
        .keys()
        .filter(|p| {
            p.strip_prefix(dir).is_some_and(|rest| !rest.contains('/'))
                && document_kind(p) == Some(kind)
        })
        .cloned()
        .collect()
}

pub fn context(ws: &Workspace, target: &str) -> Result<ContextReport> {
    let (epic, story) = target
        .split_once('/')
        .filter(|(_, s)| !s.contains('/'))
        .ok_or_else(|| {
            Error::invalid(
                "use `speq context <epic>/<story>`, for example `epic-payment/st-001-create-payment`",
            )
        })?;
    ensure_valid_epic_story(epic, story)?;

    let lock = ws.load_lock()?;
    let head_sha = lock.head_sha.clone().filter(|_| lock.pending.is_empty()).ok_or_else(|| Error::conflict("the workspace is not clean; run `speq pull` and resolve conflicts before generating context"))?;

    let scan = ws.scan()?;
    let epic_dir = format!("epics/{epic}");
    let story_dir = format!("{epic_dir}/stories/{story}");

    let mut story_paths: Vec<String> = STORY_FILES
        .iter()
        .map(|f| format!("{story_dir}/{f}"))
        .filter(|p| scan.files.contains_key(p))
        .collect();
    story_paths.extend(in_dir(
        &scan,
        &format!("{story_dir}/tasks/"),
        DocumentKind::StoryTask,
    ));
    if story_paths.is_empty() {
        return Err(Error::invalid(format!(
            "no documents found for {epic}/{story}; check the names with `speq status`"
        )));
    }

    let mut candidates: Vec<String> = ["llms.txt", "README.md", "architecture.md"]
        .iter()
        .map(|f| f.to_string())
        .collect();
    candidates.extend(in_dir(
        &scan,
        &format!("{epic_dir}/briefs/"),
        DocumentKind::Brief,
    ));
    candidates.push(format!("{epic_dir}/prd.md"));
    candidates.push(format!("{epic_dir}/architecture.md"));
    candidates.extend(story_paths);

    let prefix = &ws.config.spec_source.local_dir;
    let paths = candidates
        .into_iter()
        .filter(|p| scan.files.contains_key(p) && ws.check_target(p).is_ok())
        .map(|p| format!("{prefix}/{p}"))
        .collect();
    Ok(ContextReport {
        project_id: ws.project_id().to_string(),
        head_sha,
        paths,
    })
}
