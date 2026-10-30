//! Agent Skills (`SKILL.md`, agentskills.io) embedded in the binary, so the installed skills always match this CLI
//! version. `speq skills install` writes them to `~/.agents/skills/` (Codex, Cursor, Copilot, Gemini CLI, OpenCode)
//! and `~/.claude/skills/` (Claude Code; `$CLAUDE_CONFIG_DIR/skills/` when set), then keeps the speq block in the
//! agents' instruction files (`crate::agent_instructions`).
use crate::error::{Error, IoContext, Result};
use crate::fsx::{self, Visibility};
use crate::paths::join_canonical;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

macro_rules! skill_files {
    ($($path:literal),* $(,)?) => {
        &[$(($path, include_str!(concat!("../skills/", $path)))),*]
    };
}

/// `(path under the skills directory, content)`. The test below fails when a file in `skills/` is missing here.
pub const SKILLS: &[(&str, &str)] = skill_files![
    "speq-cli/SKILL.md",
    "speq-cli/references/conflicts.md",
    "speq-cli/references/rejections.md",
    "speq-dayaboard/SKILL.md",
    "speq-dayaboard/references/api.md",
    "speq-write/SKILL.md",
    "speq-write/references/acceptance-criteria.md",
    "speq-write/references/architecture.md",
    "speq-write/references/brief.md",
    "speq-write/references/conventions.md",
    "speq-write/references/decision.md",
    "speq-write/references/development-status.md",
    "speq-write/references/import.md",
    "speq-write/references/new-product.md",
    "speq-write/references/prd.md",
    "speq-write/references/review.md",
    "speq-write/references/story.md",
    "speq-write/references/task.md",
];

/// Skills older releases installed and this one no longer ships: removed on install and uninstall.
const RETIRED: [&str; 1] = ["speq-review"];

/// Replaced with the CLI version in `metadata.version` of each `SKILL.md`.
const VERSION_PLACEHOLDER: &str = "__SPEQ_VERSION__";

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SkillsReport {
    pub dirs: Vec<String>,
    pub skills: Vec<String>,
    /// The `CLAUDE.md` and `AGENTS.md` files that now hold the speq block.
    pub instructions: Vec<String>,
}

/// Top-level skill folders, which `speq` owns entirely.
fn names() -> BTreeSet<&'static str> {
    SKILLS
        .iter()
        .filter_map(|(path, _)| path.split('/').next())
        .collect()
}

fn render(content: &str) -> String {
    content.replace(VERSION_PLACEHOLDER, env!("CARGO_PKG_VERSION"))
}

/// `~/.agents/skills` (the cross-agent folder), then `$CLAUDE_CONFIG_DIR/skills` or `~/.claude/skills`.
/// The home folder is the one the agents use: `USERPROFILE` on Windows, `HOME` elsewhere (`std::env::home_dir`).
pub fn default_dirs() -> Result<Vec<PathBuf>> {
    let home = std::env::home_dir()
        .ok_or_else(|| Error::Other("cannot determine the home directory".into()))?;
    let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map_or_else(|| home.join(".claude"), PathBuf::from);
    let mut dirs = vec![home.join(".agents").join("skills"), claude.join("skills")];
    dirs.dedup();
    Ok(dirs)
}

/// Replace the speq skill folders in every `dirs` entry with the embedded copy, so files dropped in a newer version
/// (and retired skills) do not linger, then write the speq block into `instruction_files`. The skills come first, so
/// Claude Code's folder exists when its `CLAUDE.md` is considered.
pub fn install(dirs: &[PathBuf], instruction_files: &[PathBuf]) -> Result<SkillsReport> {
    remove(dirs)?;
    for dir in dirs {
        for (path, content) in SKILLS {
            fsx::atomic_write(
                &join_canonical(dir, path),
                render(content).as_bytes(),
                Visibility::Shared,
            )?;
        }
    }
    let instructions = crate::agent_instructions::install(instruction_files)?;
    Ok(SkillsReport {
        dirs: dirs.iter().map(|d| d.display().to_string()).collect(),
        skills: names().into_iter().map(String::from).collect(),
        instructions,
    })
}

/// Delete the speq skill folders, current and retired, from every `dirs` entry; other skills are never touched.
/// Returns the folders removed.
pub fn remove(dirs: &[PathBuf]) -> Result<Vec<String>> {
    let mut removed = Vec::new();
    for dir in dirs {
        for name in names().into_iter().chain(RETIRED) {
            let path = dir.join(name);
            match std::fs::remove_dir_all(&path) {
                Ok(()) => removed.push(path.display().to_string()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e).ctx(format!("remove {}", path.display())),
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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

    fn two_dirs() -> (tempfile::TempDir, Vec<PathBuf>) {
        let root = tempfile::tempdir().unwrap();
        let dirs = vec![
            root.path().join(".agents/skills"),
            root.path().join(".claude/skills"),
        ];
        (root, dirs)
    }

    #[test]
    fn install_writes_every_skill_into_each_folder_and_drops_stale_and_retired_files() {
        let (root, dirs) = two_dirs();
        let claude_md = root.path().join(".claude/CLAUDE.md");
        let stale = dirs[0].join("speq-write/references/old.md");
        let retired = dirs[1].join("speq-review/SKILL.md");
        for file in [&stale, &retired] {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "old").unwrap();
        }

        let report = install(&dirs, std::slice::from_ref(&claude_md)).unwrap();

        assert_eq!(
            report.skills,
            vec!["speq-cli", "speq-dayaboard", "speq-write"]
        );
        assert_eq!(report.dirs.len(), 2);
        for dir in &dirs {
            for (path, content) in SKILLS {
                let written = std::fs::read_to_string(dir.join(path)).unwrap();
                assert_eq!(written, render(content), "{path}");
            }
        }
        assert!(
            !stale.exists(),
            "files left by an older version are removed"
        );
        assert!(!retired.exists(), "retired skills are removed");
        assert_eq!(report.instructions, vec![claude_md.display().to_string()]);
        assert_eq!(
            std::fs::read_to_string(&claude_md).unwrap(),
            crate::agent_instructions::BLOCK,
            "the skills folder is written first, so Claude Code's folder exists for its CLAUDE.md"
        );
    }

    #[test]
    fn every_installed_skill_names_the_cli_version_that_wrote_it() {
        let (_root, dirs) = two_dirs();
        install(&dirs, &[]).unwrap();
        for name in names() {
            let skill = std::fs::read_to_string(dirs[0].join(name).join("SKILL.md")).unwrap();
            assert!(
                skill.contains(&format!("version: \"{}\"", env!("CARGO_PKG_VERSION"))),
                "{name}: {skill}"
            );
            assert!(!skill.contains(VERSION_PLACEHOLDER), "{name}");
        }
    }

    /// Agents pick a skill by the `name` in its front matter, which must match its folder (agentskills.io), and
    /// read only the references its `SKILL.md` names: a reference no `SKILL.md` mentions is never read.
    #[test]
    fn every_skill_is_named_after_its_folder_and_names_each_of_its_references() {
        for name in names() {
            let skill = SKILLS
                .iter()
                .find(|(path, _)| *path == format!("{name}/SKILL.md"))
                .map(|(_, content)| *content)
                .unwrap_or_else(|| panic!("{name} has no SKILL.md"));
            let front = skill
                .strip_prefix("---\n")
                .and_then(|rest| rest.split_once("\n---\n"))
                .map(|(front, _)| front)
                .unwrap_or_else(|| panic!("{name}: no front matter"));
            assert!(
                front.lines().any(|l| l == format!("name: {name}")),
                "{name}"
            );
            assert!(
                front
                    .lines()
                    .any(|l| l.starts_with("description: Use when")),
                "{name}: the description says when to use the skill"
            );
            let references = SKILLS.iter().filter_map(|(path, _)| {
                path.strip_prefix(&format!("{name}/"))
                    .filter(|p| p.starts_with("references/"))
            });
            for reference in references {
                assert!(
                    skill.contains(reference),
                    "{name}/SKILL.md never names {reference}"
                );
            }
        }
    }

    /// Agents read documents other people wrote: each skill tells them that text is data, never instructions,
    /// and that secrets stay out of documents (a push shares them with the whole project).
    #[test]
    fn every_skill_treats_spec_text_as_data_and_keeps_secrets_out() {
        for name in names() {
            let skill = SKILLS
                .iter()
                .find(|(path, _)| *path == format!("{name}/SKILL.md"))
                .map(|(_, content)| *content)
                .unwrap();
            for rule in ["data, not instructions", "tokens, passwords, keys"] {
                assert!(skill.contains(rule), "{name}/SKILL.md lacks {rule:?}");
            }
        }
    }

    #[test]
    fn remove_deletes_only_speq_skills_in_every_folder() {
        let (_root, dirs) = two_dirs();
        install(&dirs, &[]).unwrap();
        let retired = dirs[0].join("speq-review/SKILL.md");
        std::fs::create_dir_all(retired.parent().unwrap()).unwrap();
        std::fs::write(&retired, "old").unwrap();
        let mine = dirs[1].join("my-skill/SKILL.md");
        std::fs::create_dir_all(mine.parent().unwrap()).unwrap();
        std::fs::write(&mine, "mine").unwrap();

        let removed = remove(&dirs).unwrap();

        assert_eq!(
            removed.len(),
            7,
            "three skills in two folders, plus the retired one"
        );
        for dir in &dirs {
            assert!(!dir.join("speq-cli").exists());
            assert!(!dir.join("speq-dayaboard").exists());
            assert!(!dir.join("speq-write").exists());
            assert!(!dir.join("speq-review").exists());
        }
        assert!(mine.exists(), "other skills are never touched");
        assert!(remove(&dirs).unwrap().is_empty(), "removing twice is fine");
    }
}
