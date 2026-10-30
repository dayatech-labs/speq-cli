//! A short block of standing instructions that `speq skills install` keeps in the agents' personal instruction files
//! (see [`default_files`]). Cursor keeps its global rules in its settings, not in a file, so it gets none. Skills load only when a request matches them; this block reminds the agent in every
//! session to refresh `development_status.md` whenever a story or task changes. Only the text between the markers is
//! ours: the rest of the file is never changed, and `speq uninstall` removes just the block.
use crate::error::{Error, IoContext, Result};
use crate::fsx::{self, Visibility};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const BEGIN: &str = "<!-- speq:begin";
const END: &str = "<!-- speq:end -->";

/// The block, markers included, ending in a newline.
pub const BLOCK: &str = "<!-- speq:begin (written by `speq skills install` and removed by `speq uninstall`; edits inside are overwritten) -->
## Speq specs: development status

In a Speq specs repository (its root holds `llms.txt` and `epics/`; from an implementation repository usually `./specs`), `development_status.md` in that root lists every story of every epic and how far its development is, worked out from the `status` of the story's tasks. Whenever you add, rename, or remove a story or a task, or change a task's `status` (also when you finish implementing a task), refresh `development_status.md` in the same change, as the `speq-write` skill describes in `references/development-status.md`.
<!-- speq:end -->
";

/// The global instruction file of each agent that reads one, from the home folder and the environment.
pub fn default_files() -> Result<Vec<PathBuf>> {
    let home = std::env::home_dir()
        .ok_or_else(|| Error::Other("cannot determine the home directory".into()))?;
    Ok(files(&home, |var| std::env::var_os(var)))
}

/// Claude Code, Codex, Gemini CLI, OpenCode, and Copilot CLI, in that order. A variable that is set and not empty
/// moves the agent's folder.
fn files(home: &Path, var: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let dir = |name: &str, fallback: &Path| {
        var(name)
            .filter(|v| !v.is_empty())
            .map_or_else(|| fallback.to_path_buf(), PathBuf::from)
    };
    vec![
        dir("CLAUDE_CONFIG_DIR", &home.join(".claude")).join("CLAUDE.md"),
        dir("CODEX_HOME", &home.join(".codex")).join("AGENTS.md"),
        home.join(".gemini").join("GEMINI.md"),
        dir("XDG_CONFIG_HOME", &home.join(".config"))
            .join("opencode")
            .join("AGENTS.md"),
        dir("COPILOT_HOME", &home.join(".copilot")).join("copilot-instructions.md"),
    ]
}

/// The byte range of the block in `text`, from the start of its first line to the end of its last (newline included).
fn find(text: &str) -> Result<Option<(usize, usize)>> {
    let mut begin = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let trimmed = line.trim_end();
        if begin.is_none() && trimmed.starts_with(BEGIN) {
            begin = Some(start);
        } else if let Some(begin) = begin
            && trimmed == END
        {
            return Ok(Some((begin, offset)));
        }
    }
    match begin {
        Some(_) => Err(Error::Other(format!(
            "a `{BEGIN}` line has no `{END}` line after it; remove the speq block by hand"
        ))),
        None => Ok(None),
    }
}

/// `text` with the block replaced, or appended after one blank line when it is missing.
pub fn with_block(text: &str) -> Result<String> {
    if let Some((begin, end)) = find(text)? {
        return Ok(format!("{}{BLOCK}{}", &text[..begin], &text[end..]));
    }
    let mut out = text.to_string();
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.ends_with("\n\n") {
            out.push('\n');
        }
    }
    out.push_str(BLOCK);
    Ok(out)
}

/// `text` without the block and the blank line put before it, or `None` when there is no block.
pub fn without_block(text: &str) -> Result<Option<String>> {
    let Some((begin, end)) = find(text)? else {
        return Ok(None);
    };
    let before = &text[..begin];
    let before = before
        .strip_suffix('\n')
        .filter(|b| b.ends_with('\n'))
        .unwrap_or(before);
    Ok(Some(format!("{before}{}", &text[end..])))
}

/// Write the block into each file whose agent folder exists (an agent that is not installed gets no folder), keeping
/// the rest of the file. Returns the files written.
pub fn install(files: &[PathBuf]) -> Result<Vec<String>> {
    let mut written = Vec::new();
    for file in files {
        if !file.parent().is_some_and(Path::is_dir) {
            continue;
        }
        let target = resolve(file)?;
        let text = read(&target)?.unwrap_or_default();
        let new = with_block(&text).map_err(|e| in_file(e, file))?;
        if new != text {
            write(&target, &new)?;
        }
        written.push(file.display().to_string());
    }
    Ok(written)
}

/// Remove the block from each file; a file left empty is deleted. Returns the files changed.
pub fn remove(files: &[PathBuf]) -> Result<Vec<String>> {
    let mut changed = Vec::new();
    for file in files {
        let target = resolve(file)?;
        let Some(text) = read(&target)? else {
            continue;
        };
        let Some(new) = without_block(&text).map_err(|e| in_file(e, file))? else {
            continue;
        };
        if new.trim().is_empty() {
            std::fs::remove_file(&target).ctx(format!("remove {}", target.display()))?;
        } else {
            write(&target, &new)?;
        }
        changed.push(file.display().to_string());
    }
    Ok(changed)
}

fn in_file(e: Error, file: &Path) -> Error {
    Error::Other(format!("{}: {e}", file.display()))
}

/// The file a symlink points to, so a dotfiles link is kept and its target is edited.
fn resolve(file: &Path) -> Result<PathBuf> {
    match std::fs::canonicalize(file) {
        Ok(target) => Ok(target),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(file.to_path_buf()),
        Err(e) => Err(e).ctx(format!("resolve {}", file.display())),
    }
}

fn read(file: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(file) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).ctx(format!("read {}", file.display())),
    }
}

/// Replace the file atomically and keep the permissions an existing file had.
fn write(file: &Path, text: &str) -> Result<()> {
    let permissions = std::fs::metadata(file).ok().map(|m| m.permissions());
    fsx::atomic_write(file, text.as_bytes(), Visibility::Shared)?;
    if let Some(permissions) = permissions {
        std::fs::set_permissions(file, permissions)
            .ctx(format!("restore permissions of {}", file.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_tells_the_agent_to_refresh_the_status_file_and_where_the_rules_are() {
        assert!(BLOCK.starts_with(BEGIN));
        assert!(BLOCK.ends_with(&format!("{END}\n")));
        for needle in [
            "development_status.md",
            "speq-write",
            "references/development-status.md",
            "`status`",
        ] {
            assert!(BLOCK.contains(needle), "{needle}");
        }
        assert_eq!(find(BLOCK).unwrap(), Some((0, BLOCK.len())));
    }

    #[test]
    fn every_agent_has_its_global_file_and_the_environment_moves_the_folders() {
        let home = Path::new("/home/ana");
        let unset = files(home, |_| None);
        assert_eq!(
            unset,
            [
                ".claude/CLAUDE.md",
                ".codex/AGENTS.md",
                ".gemini/GEMINI.md",
                ".config/opencode/AGENTS.md",
                ".copilot/copilot-instructions.md",
            ]
            .map(|p| home.join(p))
        );
        let empty = files(home, |_| Some(OsString::new()));
        assert_eq!(empty, unset, "an empty variable counts as unset");

        let moved = files(home, |var| Some(OsString::from(format!("/env/{var}"))));
        assert_eq!(
            moved,
            [
                "/env/CLAUDE_CONFIG_DIR/CLAUDE.md",
                "/env/CODEX_HOME/AGENTS.md",
                "/home/ana/.gemini/GEMINI.md",
                "/env/XDG_CONFIG_HOME/opencode/AGENTS.md",
                "/env/COPILOT_HOME/copilot-instructions.md",
            ]
            .map(PathBuf::from)
        );
    }

    #[test]
    fn the_block_is_appended_after_one_blank_line_and_the_text_before_is_kept() {
        assert_eq!(with_block("").unwrap(), BLOCK);
        assert_eq!(with_block("# Mine").unwrap(), format!("# Mine\n\n{BLOCK}"));
        assert_eq!(
            with_block("# Mine\n").unwrap(),
            format!("# Mine\n\n{BLOCK}")
        );
        assert_eq!(
            with_block("# Mine\n\n").unwrap(),
            format!("# Mine\n\n{BLOCK}")
        );
    }

    #[test]
    fn an_existing_block_is_replaced_in_place_and_writing_twice_changes_nothing() {
        let old = "<!-- speq:begin (old wording) -->\nold rules\n<!-- speq:end -->\n";
        let text = format!("# Before\n\n{old}\n# After\n");
        let once = with_block(&text).unwrap();
        assert_eq!(once, format!("# Before\n\n{BLOCK}\n# After\n"));
        assert_eq!(with_block(&once).unwrap(), once);
    }

    #[test]
    fn removing_the_block_gives_back_the_text_it_was_added_to() {
        for original in ["", "# Mine\n", "# Mine\n\nmore\n"] {
            let added = with_block(original).unwrap();
            assert_eq!(
                without_block(&added).unwrap().as_deref(),
                Some(original),
                "{original:?}"
            );
        }
        let between = format!("# Before\n\n{BLOCK}\n# After\n");
        assert_eq!(
            without_block(&between).unwrap().as_deref(),
            Some("# Before\n\n# After\n")
        );
        assert_eq!(without_block("# Mine\n").unwrap(), None);
    }

    #[test]
    fn a_begin_marker_without_an_end_is_an_error_and_the_text_is_left_alone() {
        let broken = "# Mine\n<!-- speq:begin -->\nhalf a block\n";
        assert!(with_block(broken).is_err());
        assert!(without_block(broken).is_err());
    }

    #[test]
    fn install_writes_only_where_the_agent_folder_exists_and_remove_undoes_it() {
        let root = tempfile::tempdir().unwrap();
        let claude = root.path().join(".claude/CLAUDE.md");
        let codex = root.path().join(".codex/AGENTS.md");
        std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
        std::fs::write(&claude, "# My rules\n").unwrap();
        let files = vec![claude.clone(), codex.clone()];

        assert_eq!(install(&files).unwrap(), vec![claude.display().to_string()]);
        assert_eq!(
            std::fs::read_to_string(&claude).unwrap(),
            format!("# My rules\n\n{BLOCK}")
        );
        assert!(
            !codex.parent().unwrap().exists(),
            "no folder is made for an agent that is not installed"
        );

        std::fs::create_dir_all(codex.parent().unwrap()).unwrap();
        install(&files).unwrap();
        install(&files).unwrap();
        assert_eq!(
            std::fs::read_to_string(&codex).unwrap(),
            BLOCK,
            "installing twice keeps one block"
        );

        assert_eq!(remove(&files).unwrap().len(), 2);
        assert_eq!(std::fs::read_to_string(&claude).unwrap(), "# My rules\n");
        assert!(
            !codex.exists(),
            "a file that held only the block is deleted"
        );
        assert!(remove(&files).unwrap().is_empty(), "removing twice is fine");
    }

    #[test]
    fn a_broken_block_fails_install_without_touching_the_file() {
        let root = tempfile::tempdir().unwrap();
        let claude = root.path().join("CLAUDE.md");
        let broken = "<!-- speq:begin -->\nmine\n";
        std::fs::write(&claude, broken).unwrap();
        let err = install(std::slice::from_ref(&claude))
            .unwrap_err()
            .to_string();
        assert!(err.contains("CLAUDE.md"), "{err}");
        assert_eq!(std::fs::read_to_string(&claude).unwrap(), broken);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_keeps_its_link_and_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("dotfiles/CLAUDE.md");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "# Mine\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        let link = root.path().join(".claude/CLAUDE.md");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        install(std::slice::from_ref(&link)).unwrap();

        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            format!("# Mine\n\n{BLOCK}")
        );
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
