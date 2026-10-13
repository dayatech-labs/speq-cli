//! `speq context <epic>/<story>`: a deterministic list of the files a coding agent should read for one story, from
//! the specs repository's working tree (the epic and its stories live under `epics/<epic>/`).
use crate::error::{Error, Result};
use crate::git::Git;
use crate::paths::{DocumentKind, STORY_FILES, document_kind, ensure_valid_epic_story};
use crate::repo::SpecsRepo;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct ContextReport {
    pub project_id: String,
    /// The commit checked out (empty before the first commit).
    pub head_sha: String,
    pub paths: Vec<String>,
}

/// Canonical documents in the working tree, tracked or new (ignored files excluded). Symlinks are never listed:
/// a coding agent is pointed only at regular files inside the repository.
fn documents(git: &Git, repo: &SpecsRepo) -> Result<BTreeSet<String>> {
    let output = git.output(
        Some(&repo.root),
        [
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?;
    if !output.status.success() {
        return Err(Error::Other(crate::git::git_failure(&output)));
    }
    Ok(output
        .stdout
        .split(|b| *b == 0)
        .filter_map(|p| std::str::from_utf8(p).ok())
        .filter(|p| document_kind(p).is_some())
        .filter(|p| {
            std::fs::symlink_metadata(repo.root.join(p)).is_ok_and(|m| m.file_type().is_file())
        })
        .map(str::to_string)
        .collect())
}

/// The documents of one kind directly inside `dir` (a canonical folder path ending in `/`), by name.
fn in_dir(files: &BTreeSet<String>, dir: &str, kind: DocumentKind) -> Vec<String> {
    files
        .iter()
        .filter(|p| {
            p.strip_prefix(dir).is_some_and(|rest| !rest.contains('/'))
                && document_kind(p) == Some(kind)
        })
        .cloned()
        .collect()
}

/// `path` (inside the repository) as the user should type it from `cwd`.
fn shown(repo_root: &Path, cwd: &Path, path: &str) -> String {
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let full: PathBuf = canonical(repo_root).join(path);
    match full.strip_prefix(canonical(cwd)) {
        Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
        Err(_) => full.to_string_lossy().into_owned(),
    }
}

pub fn context(git: &Git, repo: &SpecsRepo, cwd: &Path, target: &str) -> Result<ContextReport> {
    let (epic, story) = target
        .split_once('/')
        .filter(|(_, s)| !s.contains('/'))
        .ok_or_else(|| {
            Error::invalid(
                "use `speq context <epic>/<story>`, for example `epic-payment/st-001-create-payment`",
            )
        })?;
    ensure_valid_epic_story(epic, story)?;

    let files = documents(git, repo)?;
    let epic_dir = format!("epics/{epic}");
    let story_dir = format!("{epic_dir}/stories/{story}");

    let mut story_paths: Vec<String> = STORY_FILES
        .iter()
        .map(|f| format!("{story_dir}/{f}"))
        .filter(|p| files.contains(p))
        .collect();
    story_paths.extend(in_dir(
        &files,
        &format!("{story_dir}/tasks/"),
        DocumentKind::StoryTask,
    ));
    if story_paths.is_empty() {
        return Err(Error::invalid(format!(
            "no documents found for {epic}/{story}; check the names with `speq ls-files epics/{epic}`"
        )));
    }

    let mut candidates: Vec<String> = ["llms.txt", "README.md", "architecture.md"]
        .iter()
        .map(|f| f.to_string())
        .collect();
    candidates.extend(in_dir(
        &files,
        &format!("{epic_dir}/briefs/"),
        DocumentKind::Brief,
    ));
    candidates.push(format!("{epic_dir}/prd.md"));
    candidates.push(format!("{epic_dir}/architecture.md"));
    candidates.extend(story_paths);

    let head = git.output(Some(&repo.root), ["rev-parse", "--verify", "-q", "HEAD"])?;
    Ok(ContextReport {
        project_id: repo.project_id.clone(),
        head_sha: String::from_utf8_lossy(&head.stdout).trim().to_string(),
        paths: candidates
            .into_iter()
            .filter(|p| files.contains(p))
            .map(|p| shown(&repo.root, cwd, &p))
            .collect(),
    })
}
