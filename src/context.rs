//! `speq context <epic>[/<story>[/<task>]]`: a deterministic list of the files a coding agent should read for one
//! epic, one story, or one task of a story, from the specs repository's working tree (the epic and its stories live
//! under `epics/<epic>/`).
use crate::error::{Error, Result};
use crate::git::Git;
use crate::paths::{
    DocumentKind, STORY_FILES, document_kind, is_folder_name, is_story_folder, is_task_file,
};
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

/// What the context is for: a whole epic, one story, or one task file (named without `.md`) of a story.
#[derive(Debug, PartialEq, Eq)]
enum Target<'a> {
    Epic(&'a str),
    Story(&'a str, &'a str),
    Task(&'a str, &'a str, &'a str),
}

impl<'a> Target<'a> {
    fn parse(target: &'a str) -> Result<Target<'a>> {
        let parsed = match target.split('/').collect::<Vec<_>>()[..] {
            [epic] => Target::Epic(epic),
            [epic, story] => Target::Story(epic, story),
            [epic, story, task] => {
                Target::Task(epic, story, task.strip_suffix(".md").unwrap_or(task))
            }
            _ => {
                return Err(Error::invalid(
                    "use `speq context <epic>`, `<epic>/<story>` or `<epic>/<story>/<task>`, for example `epic-payment/st-001-create-payment`",
                ));
            }
        };
        let valid = match parsed {
            Target::Epic(epic) => is_folder_name(epic),
            Target::Story(epic, story) => is_folder_name(epic) && is_story_folder(story),
            Target::Task(epic, story, task) => {
                is_folder_name(epic)
                    && is_story_folder(story)
                    && is_task_file(&format!("{task}.md"))
            }
        };
        if !valid {
            return Err(Error::invalid(
                "the epic must be a lowercase folder name, the story a folder named `st-<number>-<name>`, and the task a file named `<kind>-<number>-<name>` (`.md` optional), such as `epic-payment/st-001-create-payment/be-001-api`",
            ));
        }
        Ok(parsed)
    }

    fn epic(&self) -> &'a str {
        match self {
            Target::Epic(epic) | Target::Story(epic, _) | Target::Task(epic, _, _) => epic,
        }
    }
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

/// The story's own documents (not its tasks) that exist, in `STORY_FILES` order.
fn story_files(files: &BTreeSet<String>, story_dir: &str) -> Vec<String> {
    STORY_FILES
        .iter()
        .map(|f| format!("{story_dir}/{f}"))
        .filter(|p| files.contains(p))
        .collect()
}

/// The story's own documents, then all of its tasks.
fn story_with_tasks(files: &BTreeSet<String>, story_dir: &str) -> Vec<String> {
    let mut paths = story_files(files, story_dir);
    paths.extend(in_dir(
        files,
        &format!("{story_dir}/tasks/"),
        DocumentKind::StoryTask,
    ));
    paths
}

/// The story folders of an epic that hold at least one document, by name.
fn stories<'f>(files: &'f BTreeSet<String>, epic_dir: &str) -> BTreeSet<&'f str> {
    let prefix = format!("{epic_dir}/stories/");
    files
        .iter()
        .filter_map(|p| Some(p.strip_prefix(&prefix)?.split_once('/')?.0))
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
    let target = Target::parse(target)?;
    let files = documents(git, repo)?;
    let epic = target.epic();
    let epic_dir = format!("epics/{epic}");

    let focus = match target {
        Target::Epic(_) => {
            if !files.iter().any(|p| p.starts_with(&format!("{epic_dir}/"))) {
                return Err(Error::invalid(format!(
                    "no documents found for {epic}; check the names with `speq ls-files epics`"
                )));
            }
            stories(&files, &epic_dir)
                .into_iter()
                .flat_map(|story| story_with_tasks(&files, &format!("{epic_dir}/stories/{story}")))
                .collect()
        }
        Target::Story(_, story) => {
            let paths = story_with_tasks(&files, &format!("{epic_dir}/stories/{story}"));
            if paths.is_empty() {
                return Err(Error::invalid(format!(
                    "no documents found for {epic}/{story}; check the names with `speq ls-files epics/{epic}`"
                )));
            }
            paths
        }
        Target::Task(_, story, task) => {
            let story_dir = format!("{epic_dir}/stories/{story}");
            let task_path = format!("{story_dir}/tasks/{task}.md");
            if !files.contains(&task_path) {
                return Err(Error::invalid(format!(
                    "no task {task} found in {epic}/{story}; check the names with `speq ls-files {story_dir}/tasks`"
                )));
            }
            let mut paths = story_files(&files, &story_dir);
            paths.push(task_path);
            paths
        }
    };

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
    candidates.extend(focus);

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

#[cfg(test)]
mod tests {
    use super::*;

    fn files(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn targets_name_an_epic_a_story_or_a_task() {
        assert_eq!(
            Target::parse("epic-payment").unwrap(),
            Target::Epic("epic-payment")
        );
        assert_eq!(
            Target::parse("epic-payment/st-001-create-payment").unwrap(),
            Target::Story("epic-payment", "st-001-create-payment")
        );
        for task in ["be-001-api", "be-001-api.md"] {
            assert_eq!(
                Target::parse(&format!("epic-payment/st-001-create-payment/{task}")).unwrap(),
                Target::Task("epic-payment", "st-001-create-payment", "be-001-api"),
                "{task}"
            );
        }
        assert_eq!(
            Target::parse("epic-payment/st-001-create-payment/be-001-api")
                .unwrap()
                .epic(),
            "epic-payment"
        );
    }

    #[test]
    fn malformed_targets_are_rejected_with_the_expected_shape() {
        for bad in [
            "",
            "/",
            "Epic-Payment",
            "epics/epic-payment",
            "epic-payment/",
            "epic-payment/nope",
            "epic-payment/st-1-short-number",
            "epic-payment/st-001-create-payment/",
            "epic-payment/st-001-create-payment/README",
            "epic-payment/st-001-create-payment/be-001-api.txt",
            "epic-payment/st-001-create-payment/x-001-one-letter-kind",
            "../epic-payment",
            "epic-payment/st-001-create-payment/tasks/be-001-api",
        ] {
            let err = Target::parse(bad).expect_err(bad).to_string();
            assert!(
                err.contains("epic-payment/st-001-create-payment"),
                "{bad}: {err}"
            );
        }
    }

    #[test]
    fn stories_are_the_story_folders_that_hold_documents() {
        let files = files(&[
            "epics/a/prd.md",
            "epics/a/stories/st-002-b/README.md",
            "epics/a/stories/st-001-a/tasks/be-001-x.md",
            "epics/a/stories/st-001-a/README.md",
            "epics/ab/stories/st-009-other-epic/README.md",
        ]);
        assert_eq!(
            stories(&files, "epics/a").into_iter().collect::<Vec<_>>(),
            ["st-001-a", "st-002-b"]
        );
    }

    #[test]
    fn a_story_lists_its_documents_in_story_order_then_its_tasks() {
        let files = files(&[
            "epics/a/stories/st-001-a/tasks/fe-002-ui.md",
            "epics/a/stories/st-001-a/tasks/be-001-api.md",
            "epics/a/stories/st-001-a/layout.html",
            "epics/a/stories/st-001-a/acceptance-criteria.md",
            "epics/a/stories/st-001-a/README.md",
        ]);
        let dir = "epics/a/stories/st-001-a";
        assert_eq!(
            story_with_tasks(&files, dir),
            [
                "epics/a/stories/st-001-a/README.md",
                "epics/a/stories/st-001-a/acceptance-criteria.md",
                "epics/a/stories/st-001-a/layout.html",
                "epics/a/stories/st-001-a/tasks/be-001-api.md",
                "epics/a/stories/st-001-a/tasks/fe-002-ui.md",
            ]
        );
        assert_eq!(
            story_files(&files, dir),
            [
                "epics/a/stories/st-001-a/README.md",
                "epics/a/stories/st-001-a/acceptance-criteria.md",
                "epics/a/stories/st-001-a/layout.html",
            ]
        );
    }
}
