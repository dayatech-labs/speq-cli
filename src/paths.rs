//! Canonical document paths, mirroring the API's validation. Remote paths are never trusted:
//! every path that is read from the API or written locally passes through here first.
use std::path::{Path, PathBuf};

const MAX_PATH_LENGTH: usize = 300;
const MAX_SEGMENTS: usize = 8;
const ASSET_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];
const EPIC_FILES: [(&str, DocumentKind); 2] = [
    ("prd.md", DocumentKind::Prd),
    ("architecture.md", DocumentKind::EpicArchitecture),
];
/// Files directly inside `epics/<epic>/stories/<story>/`, in the order `speq context` prints them.
pub const STORY_FILES: [&str; 4] = [
    "README.md",
    "acceptance-criteria.md",
    "DESIGN.md",
    "layout.html",
];
/// Design documents: Markdown rules and HTML component prototypes.
const DESIGN_EXTENSIONS: [&str; 2] = [".md", ".html"];

/// What a canonical path is, mirroring the API's `DocumentKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Llms,
    DevelopmentStatus,
    RootReadme,
    ProductArchitecture,
    Design,
    Brief,
    Prd,
    EpicArchitecture,
    Decision,
    StoryReadme,
    AcceptanceCriteria,
    StoryDesign,
    StoryLayout,
    StoryTask,
}

fn is_lower_alnum(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit()
}

/// `<prefix><digits>-<kebab>.<ext>` shared by briefs, decisions and tasks: at least `min_digits`
/// digits, then a dash, then a kebab name of at most `max_name` characters.
fn is_numbered_name(rest: &str, min_digits: usize, max_name: usize) -> bool {
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let Some(name) = rest[digits..].strip_prefix('-') else {
        return false;
    };
    let mut chars = name.chars();
    digits >= min_digits
        && matches!(chars.next(), Some(c) if is_lower_alnum(c))
        && name.len() <= max_name
        && chars.all(|c| is_lower_alnum(c) || c == '-')
}

/// `^dr-[0-9]{2,}-[a-z0-9][a-z0-9-]{0,80}\.md$`
fn is_decision_file(s: &str) -> bool {
    s.strip_prefix("dr-")
        .and_then(|r| r.strip_suffix(".md"))
        .is_some_and(|rest| is_numbered_name(rest, 2, 81))
}

/// `^br-[0-9]{3,}-[a-z0-9][a-z0-9-]{0,80}\.md$`
fn is_brief_file(s: &str) -> bool {
    s.strip_prefix("br-")
        .and_then(|r| r.strip_suffix(".md"))
        .is_some_and(|rest| is_numbered_name(rest, 3, 81))
}

/// `^st-[0-9]{3,}-[a-z0-9][a-z0-9-]{0,50}$`
pub fn is_story_folder(s: &str) -> bool {
    s.strip_prefix("st-")
        .is_some_and(|rest| is_numbered_name(rest, 3, 51))
}

/// `^[a-z]{2,8}-[0-9]{3,}-[a-z0-9][a-z0-9-]{0,80}\.md$`
pub fn is_task_file(s: &str) -> bool {
    let Some(rest) = s.strip_suffix(".md") else {
        return false;
    };
    let kind = rest.chars().take_while(char::is_ascii_lowercase).count();
    (2..=8).contains(&kind)
        && rest[kind..]
            .strip_prefix('-')
            .is_some_and(|number| is_numbered_name(number, 3, 81))
}

/// `^[a-z0-9][a-z0-9_-]{0,63}$`
pub fn is_folder_name(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if is_lower_alnum(c))
        && s.len() <= 64
        && chars.all(|c| is_lower_alnum(c) || c == '_' || c == '-')
}

/// `^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$`
fn is_file_segment(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && s.len() <= 100
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// Classify a path as one of the text documents the API manages (`llms.txt`, Markdown and the HTML
/// prototypes), or `None` when it is not canonical. Assets are not synced.
pub fn document_kind(path: &str) -> Option<DocumentKind> {
    if path.is_empty()
        || path.len() > MAX_PATH_LENGTH
        || path.starts_with('/')
        || path.ends_with('/')
    {
        return None;
    }
    if !path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
    {
        return None;
    }
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() > MAX_SEGMENTS || segments.iter().any(|s| s.is_empty() || s.starts_with('.'))
    {
        return None;
    }
    let file = segments[segments.len() - 1];
    let extension = file.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if ASSET_EXTENSIONS.contains(&extension) {
        return None;
    }
    match path {
        "llms.txt" => return Some(DocumentKind::Llms),
        "development_status.md" => return Some(DocumentKind::DevelopmentStatus),
        "README.md" => return Some(DocumentKind::RootReadme),
        "architecture.md" => return Some(DocumentKind::ProductArchitecture),
        _ => {}
    }
    if segments[0] == "design" {
        let valid = segments.len() >= 2
            && DESIGN_EXTENSIONS
                .iter()
                .any(|ext| file.ends_with(ext) && file.len() > ext.len())
            && segments.iter().all(|s| is_file_segment(s));
        return valid.then_some(DocumentKind::Design);
    }
    // Epics live under `epics/<epic>/`; an epic folder at the repository root is not canonical.
    if segments[0] != "epics" || segments.len() < 3 || !is_folder_name(segments[1]) {
        return None;
    }
    match (segments.len(), segments[2]) {
        (3, _) => EPIC_FILES
            .iter()
            .find(|(name, _)| *name == file)
            .map(|(_, kind)| *kind),
        (4, "briefs") => is_brief_file(file).then_some(DocumentKind::Brief),
        (4, "decisions") => is_decision_file(file).then_some(DocumentKind::Decision),
        (5, "stories") | (6, "stories") if !is_story_folder(segments[3]) => None,
        (5, "stories") => match file {
            "README.md" => Some(DocumentKind::StoryReadme),
            "acceptance-criteria.md" => Some(DocumentKind::AcceptanceCriteria),
            "DESIGN.md" => Some(DocumentKind::StoryDesign),
            "layout.html" => Some(DocumentKind::StoryLayout),
            _ => None,
        },
        (6, "stories") if segments[4] == "tasks" => {
            is_task_file(file).then_some(DocumentKind::StoryTask)
        }
        _ => None,
    }
}

/// True for the text documents the API manages: `llms.txt`, Markdown, and HTML prototypes.
pub fn is_canonical_document(path: &str) -> bool {
    document_kind(path).is_some()
}

/// A role-policy path pattern, as the API matches it: `*` is exactly one segment, a trailing `**` one or more.
pub fn matches_rule(pattern: &str, path: &str) -> bool {
    let patterns: Vec<&str> = pattern.split('/').collect();
    let segments: Vec<&str> = path.split('/').collect();
    for (index, p) in patterns.iter().enumerate() {
        if *p == "**" {
            return segments.len() > index;
        }
        match segments.get(index) {
            Some(s) if *p == "*" || p == s => {}
            _ => return false,
        }
    }
    patterns.len() == segments.len()
}

/// Join a canonical `a/b/c.md` path onto a directory using the platform separator.
pub fn join_canonical(base: &Path, canonical: &str) -> PathBuf {
    canonical
        .split('/')
        .fold(base.to_path_buf(), |acc, segment| acc.join(segment))
}

/// `local_dir` must stay inside the workspace: relative, no `..`, no hidden or `.speq` segments.
pub fn is_valid_local_dir(dir: &str) -> bool {
    !dir.is_empty()
        && dir.len() <= 100
        && !dir.starts_with('/')
        && !dir.contains('\\')
        && dir.split('/').all(|s| {
            !s.is_empty()
                && !s.starts_with('.')
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long(prefix: &str, name_len: usize, suffix: &str) -> String {
        format!("{prefix}{}{suffix}", "a".repeat(name_len))
    }

    #[test]
    fn accepts_canonical_documents_with_their_kind() {
        use DocumentKind::*;
        for (path, kind) in [
            ("llms.txt", Llms),
            ("development_status.md", DevelopmentStatus),
            ("README.md", RootReadme),
            ("architecture.md", ProductArchitecture),
            ("design/DESIGN.md", Design),
            ("design/tokens.md", Design),
            ("design/components/button.md", Design),
            ("design/components/button.html", Design),
            ("design/a/b/c/d.html", Design),
            ("epics/epic-payment/prd.md", Prd),
            ("epics/epic-payment/architecture.md", EpicArchitecture),
            ("epics/epic_1/prd.md", Prd),
            ("epics/epic-payment/briefs/br-001-checkout-flow.md", Brief),
            ("epics/epic-payment/briefs/br-1000-big-number.md", Brief),
            (
                "epics/epic-payment/decisions/dr-01-use-postgres.md",
                Decision,
            ),
            (
                "epics/epic-payment/stories/st-001-create-payment/README.md",
                StoryReadme,
            ),
            (
                "epics/epic-payment/stories/st-001-create-payment/acceptance-criteria.md",
                AcceptanceCriteria,
            ),
            ("epics/epic-payment/stories/st-002-x/DESIGN.md", StoryDesign),
            (
                "epics/epic-payment/stories/st-002-x/layout.html",
                StoryLayout,
            ),
            (
                "epics/epic-payment/stories/st-002-x/tasks/fe-001-form.md",
                StoryTask,
            ),
            (
                "epics/epic-payment/stories/st-002-x/tasks/be-002-api.md",
                StoryTask,
            ),
            (
                "epics/epic-payment/stories/st-002-x/tasks/mobile-010-x.md",
                StoryTask,
            ),
        ] {
            assert_eq!(document_kind(path), Some(kind), "{path}");
            assert!(is_canonical_document(path), "{path}");
        }
    }

    #[test]
    fn rejects_the_previous_structure() {
        for old in [
            "epics/epic-payment/product-brief.md",
            "epics/epic-payment/features/create-payment/README.md",
            "epics/epic-payment/features/create-payment/acceptance-criteria.md",
            "epics/epic-payment/features/create-payment/DESIGN.md",
            "epics/epic-payment/features/create-payment/frontend.md",
            "epics/epic-payment/features/create-payment/backend.md",
        ] {
            assert!(!is_canonical_document(old), "{old}");
        }
    }

    #[test]
    fn rejects_malformed_names_and_unknown_files() {
        for bad in [
            // epic level
            "epics/epic-payment/notes.md",
            "epics/epic-payment/prd.html",
            "epics/epic-payment/PRD.md",
            "epics/epic-payment/prd.md/",
            "epics/prd.md",
            "epics/Epic/prd.md",
            "epic-payment/prd.md",
            "epic-payment/stories/st-001-x/README.md",
            "epics/-epic/prd.md",
            // root level
            "readme.md",
            "Architecture.md",
            "docs/architecture.md",
            "llms.md",
            "Development_status.md",
            "development-status.md",
            "docs/development_status.md",
            "epics/epic-payment/development_status.md",
            "layout.html",
            // briefs: prefix, digits (3+), name, extension, depth
            "epics/epic-payment/briefs/br-01-x.md",
            "epics/epic-payment/briefs/br-001.md",
            "epics/epic-payment/briefs/br-001-.md",
            "epics/epic-payment/briefs/br-001-Upper.md",
            "epics/epic-payment/briefs/br-001-x.html",
            "epics/epic-payment/briefs/brief-001-x.md",
            "epics/epic-payment/briefs/001-x.md",
            "epics/epic-payment/briefs/x.md",
            "epics/epic-payment/briefs/sub/br-001-x.md",
            "epics/epic-payment/brief/br-001-x.md",
            // decisions
            "epics/epic-payment/decisions/use-postgres.md",
            "epics/epic-payment/decisions/dr-1-x.md",
            "epics/epic-payment/decisions/dr-01-x.html",
            "epics/epic-payment/decisions/old/dr-01-x.md",
            // story folder and files
            "epics/epic-payment/stories/create-payment/README.md",
            "epics/epic-payment/stories/st-1-x/README.md",
            "epics/epic-payment/stories/st-01-x/README.md",
            "epics/epic-payment/stories/st-001/README.md",
            "epics/epic-payment/stories/st-001-/README.md",
            "epics/epic-payment/stories/st-001-Upper/README.md",
            "epics/epic-payment/stories/st-001-a_b/README.md",
            "epics/epic-payment/stories/st-001-x/notes.md",
            "epics/epic-payment/stories/st-001-x/layout.md",
            "epics/epic-payment/stories/st-001-x/README.html",
            "epics/epic-payment/stories/st-001-x/DESIGN.html",
            "epics/epic-payment/stories/st-001-x/design.md",
            "epics/epic-payment/stories/st-001-x/frontend.md",
            "epics/epic-payment/stories/st-001-x/backend.md",
            "epics/epic-payment/stories/st-001-x/y/README.md",
            "epics/epic-payment/story/st-001-x/README.md",
            "epics/epic-payment/stories/README.md",
            // tasks
            "epics/epic-payment/stories/st-001-x/tasks/fe.md",
            "epics/epic-payment/stories/st-001-x/tasks/fe-1-x.md",
            "epics/epic-payment/stories/st-001-x/tasks/fe-01-x.md",
            "epics/epic-payment/stories/st-001-x/tasks/f-001-x.md",
            "epics/epic-payment/stories/st-001-x/tasks/toolongtype-001-x.md",
            "epics/epic-payment/stories/st-001-x/tasks/FE-001-x.md",
            "epics/epic-payment/stories/st-001-x/tasks/fe1-001-x.md",
            "epics/epic-payment/stories/st-001-x/tasks/fe-001-x.html",
            "epics/epic-payment/stories/st-001-x/tasks/fe-001-X.md",
            "epics/epic-payment/stories/st-001-x/tasks/sub/fe-001-x.md",
            "epics/epic-payment/stories/st-001-x/task/fe-001-x.md",
            "epics/epic-payment/stories/create-payment/tasks/fe-001-x.md",
            // design: extension, length, segments
            "design/",
            "design",
            "design/.md",
            "design/.html",
            "design/x.txt",
            "design/x.htm",
            "design/x.png",
            "design/a b.md",
            "design/-x.md",
            "design/components/_x.html",
            "designs/x.md",
            // traversal, encoding, hidden, separators, assets
            "",
            "/etc/passwd",
            "/design/x.md",
            "../x.md",
            "design/../x.md",
            "epics/epic/../prd.md",
            "epics//epic-payment/prd.md",
            "./README.md",
            ".hidden/prd.md",
            "epics/epic-payment/.prd.md",
            "design/.hidden/x.md",
            "epics/epic-payment/stories/.st-001-x/README.md",
            "epics/epic payment/prd.md",
            "epics/epic-payment/prd%2e.md",
            "epics/epic-payment/prd.md\n",
            "epics/epic-payment\\prd.md",
            "epics/épic/prd.md",
            "epics/epic-payment/images/flow.png",
            "design/components/preview.png",
        ] {
            assert!(!is_canonical_document(bad), "{bad:?}");
        }
    }

    #[test]
    fn enforces_name_length_limits_at_the_boundary() {
        // story folder: 1 + up to 50 after `st-001-`
        let story = |n: usize| long("epics/e/stories/st-001-", n, "/README.md");
        assert!(is_canonical_document(&story(51)));
        assert!(!is_canonical_document(&story(52)));
        // brief and decision names: first char + 80
        assert!(is_canonical_document(&long(
            "epics/e/briefs/br-001-",
            81,
            ".md"
        )));
        assert!(!is_canonical_document(&long(
            "epics/e/briefs/br-001-",
            82,
            ".md"
        )));
        assert!(is_canonical_document(&long(
            "epics/e/decisions/dr-01-",
            81,
            ".md"
        )));
        assert!(!is_canonical_document(&long(
            "epics/e/decisions/dr-01-",
            82,
            ".md"
        )));
        // task names
        let task = |n: usize| long("epics/e/stories/st-001-x/tasks/fe-001-", n, ".md");
        assert!(is_canonical_document(&task(81)));
        assert!(!is_canonical_document(&task(82)));
        // task type: 2-8 letters
        assert!(is_canonical_document(
            "epics/e/stories/st-001-x/tasks/abcdefgh-001-x.md"
        ));
        assert!(!is_canonical_document(
            "epics/e/stories/st-001-x/tasks/abcdefghi-001-x.md"
        ));
        // epic folder: 64 characters
        assert!(is_canonical_document(&format!(
            "epics/{}/prd.md",
            "a".repeat(64)
        )));
        assert!(!is_canonical_document(&format!(
            "epics/{}/prd.md",
            "a".repeat(65)
        )));
        // whole path: 300 characters
        assert!(!is_canonical_document(&format!(
            "design/{}.md",
            "a".repeat(300)
        )));
        // design: 100 characters per segment, 8 segments at most
        assert!(is_canonical_document(&format!(
            "design/{}.html",
            "a".repeat(95)
        )));
        assert!(!is_canonical_document(&format!(
            "design/{}.html",
            "a".repeat(96)
        )));
        assert!(is_canonical_document("design/a/b/c/d/e/f/g.md"));
        assert!(!is_canonical_document("design/a/b/c/d/e/f/g/h.md"));
    }

    #[test]
    fn rule_patterns_match_like_the_api() {
        let story = "epics/pay/stories/st-001-a/README.md";
        assert!(matches_rule("epics/*/stories/*/README.md", story));
        assert!(matches_rule("epics/*/stories/**", story));
        assert!(matches_rule("**", "llms.txt"));
        assert!(matches_rule("llms.txt", "llms.txt"));
        assert!(!matches_rule("epics/*/prd.md", story));
        assert!(
            !matches_rule("epics/*", "epics/pay/prd.md"),
            "* is one segment"
        );
        assert!(
            !matches_rule("epics/pay/**", "epics/pay"),
            "** needs at least one segment"
        );
        assert!(!matches_rule("design/**", "designs/a.md"));
    }

    #[test]
    fn story_folder_rule() {
        for ok in [
            "st-001-create-payment",
            "st-1000-x",
            "st-001-1",
            "st-001-a-b",
        ] {
            assert!(is_story_folder(ok), "{ok}");
        }
        for bad in [
            "",
            "st-001",
            "st-01-x",
            "create-payment",
            "ST-001-x",
            "st-001-X",
            "st-001--x",
            "st-001-x_y",
            "st-001-x/y",
            "st-001-x ",
            "../st-001-x",
        ] {
            assert!(!is_story_folder(bad), "{bad:?}");
        }
    }

    #[test]
    fn local_dir_rules() {
        assert!(is_valid_local_dir("specs"));
        assert!(is_valid_local_dir("docs/specs"));
        for bad in [
            "",
            "/abs",
            "../up",
            "a/../b",
            ".speq",
            "docs/.hidden",
            "a\\b",
            "a//b",
        ] {
            assert!(!is_valid_local_dir(bad), "{bad}");
        }
    }
}
