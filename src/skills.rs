//! Claude Code skills embedded in the binary, so the installed skills always match this CLI version.
//! `speq skills install` writes them to `~/.claude/skills/` (or `$CLAUDE_CONFIG_DIR/skills/`).
use crate::error::{Error, IoContext, Result};
use crate::fsx::{self, Visibility};
use crate::paths::join_canonical;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

macro_rules! skill_files {
    ($($path:literal),* $(,)?) => {
        &[$(($path, include_str!(concat!("../skills/", $path)))),*]
    };
}

/// `(path under the skills directory, content)`. The test below fails when a file in `skills/` is missing here.
pub const SKILLS: &[(&str, &str)] = skill_files![
    "speq-review/SKILL.md",
    "speq-write/SKILL.md",
    "speq-write/references/acceptance-criteria.md",
    "speq-write/references/architecture.md",
    "speq-write/references/brief.md",
    "speq-write/references/conventions.md",
    "speq-write/references/decision.md",
    "speq-write/references/import.md",
    "speq-write/references/new-product.md",
    "speq-write/references/prd.md",
    "speq-write/references/story.md",
    "speq-write/references/task.md",
];

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SkillsReport {
    pub dir: String,
    pub skills: Vec<String>,
}

/// Top-level skill folders, which `speq` owns entirely.
fn names() -> BTreeSet<&'static str> {
    SKILLS
        .iter()
        .filter_map(|(path, _)| path.split('/').next())
        .collect()
}

/// `$CLAUDE_CONFIG_DIR/skills`, or `~/.claude/skills`: where Claude Code looks for personal skills.
pub fn default_dir() -> Result<PathBuf> {
    let config = match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => directories::BaseDirs::new()
            .ok_or_else(|| Error::Other("cannot determine the home directory".into()))?
            .home_dir()
            .join(".claude"),
    };
    Ok(config.join("skills"))
}

/// Replace the speq skill folders with the embedded copy, so files dropped in a newer version do not linger.
pub fn install(dir: &Path) -> Result<SkillsReport> {
    remove(dir)?;
    for (path, content) in SKILLS {
        fsx::atomic_write(
            &join_canonical(dir, path),
            content.as_bytes(),
            Visibility::Shared,
        )?;
    }
    Ok(SkillsReport {
        dir: dir.display().to_string(),
        skills: names().into_iter().map(String::from).collect(),
    })
}

/// Delete the speq skill folders; other skills in `dir` are never touched. Returns the folders removed.
pub fn remove(dir: &Path) -> Result<Vec<String>> {
    let mut removed = Vec::new();
    for name in names() {
        let path = dir.join(name);
        match std::fs::remove_dir_all(&path) {
            Ok(()) => removed.push(path.display().to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).ctx(format!("remove {}", path.display())),
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file under `skills/` must be listed in `SKILLS`, or it would silently never be installed.
    #[test]
    fn every_file_in_the_skills_folder_is_embedded() {
        fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
                if entry.file_type().unwrap().is_dir() {
                    walk(&entry.path(), &format!("{name}/"), out);
                } else {
                    out.push(name);
                }
            }
        }
        let mut on_disk = Vec::new();
        walk(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("skills"),
            "",
            &mut on_disk,
        );
        on_disk.sort();
        let mut embedded: Vec<String> = SKILLS.iter().map(|(p, _)| p.to_string()).collect();
        embedded.sort();
        assert_eq!(embedded, on_disk);
    }

    #[test]
    fn install_writes_every_skill_and_drops_files_from_older_versions() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join("speq-write/references/old.md");
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        std::fs::write(&stale, "old").unwrap();

        let report = install(dir.path()).unwrap();

        assert_eq!(report.skills, vec!["speq-review", "speq-write"]);
        for (path, content) in SKILLS {
            let written = std::fs::read_to_string(dir.path().join(path)).unwrap();
            assert_eq!(&written, content, "{path}");
        }
        assert!(
            !stale.exists(),
            "files left by an older version are removed"
        );
    }

    #[test]
    fn remove_deletes_only_speq_skills() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path()).unwrap();
        let mine = dir.path().join("my-skill/SKILL.md");
        std::fs::create_dir_all(mine.parent().unwrap()).unwrap();
        std::fs::write(&mine, "mine").unwrap();

        let removed = remove(dir.path()).unwrap();

        assert_eq!(removed.len(), 2);
        assert!(!dir.path().join("speq-write").exists());
        assert!(!dir.path().join("speq-review").exists());
        assert!(mine.exists(), "other skills are never touched");
        assert!(
            remove(dir.path()).unwrap().is_empty(),
            "removing twice is fine"
        );
    }
}
