//! What a `<product>-ui` folder may expose, with the same rules the Speq API applies to the repository
//! (`speq-api/src/services/canonical-path.ts`, `ui-personas.ts`, `ui-design.ts`), so the local preview shows a
//! prototype the way the hosted app will: a page name, `personas.json` or `DESIGN.md` that the API would refuse is
//! refused here too. Change them together.
use crate::api::Persona;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashSet;

pub const MAX_PERSONAS_BYTES: u64 = 64 * 1024;
pub const MAX_DESIGN_BYTES: u64 = 256 * 1024;
/// `MAX_SEGMENTS` (8) of a repository path, less `pages/`: the deepest page slug.
pub const MAX_SLUG_SEGMENTS: usize = 7;
const MAX_PERSONAS: usize = 20;
const MAX_PERSONA_PAGES: usize = 50;
const MAX_PERSONA_NAME: usize = 80;
const MAX_PERSONA_NOTE: usize = 200;
const MAX_ID: usize = 64;
const MAX_SEGMENT: usize = 100;
const HTML: &str = ".html";

/// One segment of a page path: a letter or digit, then letters, digits, `.`, `_` and `-`; so never a hidden name.
pub fn is_file_segment(segment: &str) -> bool {
    (1..=MAX_SEGMENT).contains(&segment.len())
        && segment.starts_with(|c: char| c.is_ascii_alphanumeric())
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// `pages/<folders>/<name>.html` with plain segments, within the depth the API allows.
pub fn is_page_path(path: &str) -> bool {
    let Some(slug_path) = path.strip_prefix("pages/") else {
        return false;
    };
    let segments: Vec<&str> = slug_path.split('/').collect();
    segments.len() <= MAX_SLUG_SEGMENTS
        && segments.iter().all(|s| is_file_segment(s))
        && segments
            .last()
            .is_some_and(|file| file.len() > HTML.len() && file.ends_with(HTML))
}

/// A slug (`admin/orders`) as `personas.json` writes a page; the last segment of a pattern may be `*`.
fn is_slug(value: &str, allow_star: bool) -> bool {
    let segments: Vec<&str> = value.split('/').collect();
    let last = segments.len() - 1;
    segments.len() <= MAX_SLUG_SEGMENTS
        && segments
            .iter()
            .enumerate()
            .all(|(i, s)| is_file_segment(s) || (allow_star && i == last && *s == "*"))
}

/// A persona id: a letter or digit, then up to 63 of letters, digits, `_`, `.`, `:` and `-`.
fn is_persona_id(id: &str) -> bool {
    (1..=MAX_ID).contains(&id.len())
        && id.starts_with(|c: char| c.is_ascii_alphanumeric())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
}

/// The characters a persona's name or note may not hold: controls, and what is invisible (the API's own list).
fn is_forbidden(c: char) -> bool {
    matches!(
        c,
        '\u{0000}'..='\u{001F}'
            | '\u{007F}'..='\u{009F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

/// Text that has none of the forbidden characters, trimmed, and at most `max` UTF-16 units long (what JavaScript's
/// `.length` counts, which the API's limit is written in).
fn plain_text(value: &Value, max: usize) -> Option<String> {
    let text = value.as_str()?;
    if text.chars().any(is_forbidden) {
        return None;
    }
    let text = text.trim();
    (text.encode_utf16().count() <= max).then(|| text.to_string())
}

/// Why `personas.json` or `DESIGN.md` is not used; its text never leaves this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Problem {
    Invalid,
    TooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonasResult {
    pub problem: Option<Problem>,
    pub personas: Vec<Persona>,
}

impl PersonasResult {
    pub fn none() -> Self {
        PersonasResult {
            problem: None,
            personas: Vec::new(),
        }
    }

    pub fn problem(problem: Problem) -> Self {
        PersonasResult {
            problem: Some(problem),
            personas: Vec::new(),
        }
    }
}

fn only_keys(object: &Map<String, Value>, allowed: &[&str]) -> bool {
    object.keys().all(|key| allowed.contains(&key.as_str()))
}

fn persona(entry: &Value) -> Option<Persona> {
    let object = entry.as_object()?;
    if !only_keys(object, &["id", "name", "note", "start", "pages"]) {
        return None;
    }
    let id = object.get("id")?.as_str().filter(|id| is_persona_id(id))?;
    let name = plain_text(object.get("name")?, MAX_PERSONA_NAME).filter(|name| !name.is_empty())?;
    let note = match object.get("note") {
        Some(value) => Some(plain_text(value, MAX_PERSONA_NOTE)?),
        None => None,
    };
    let start = match object.get("start") {
        Some(value) => Some(value.as_str().filter(|s| is_slug(s, false))?.to_string()),
        None => None,
    };
    let pages = match object.get("pages") {
        Some(value) => {
            let list = value.as_array().filter(|l| l.len() <= MAX_PERSONA_PAGES)?;
            list.iter()
                .map(|p| p.as_str().filter(|s| is_slug(s, true)).map(str::to_string))
                .collect::<Option<Vec<_>>>()?
        }
        None => Vec::new(),
    };
    Some(Persona {
        id: id.to_string(),
        name,
        note,
        start,
        pages,
    })
}

/// The personas of a file's bytes, or `invalid` for anything that is not exactly the documented shape: an object with
/// only `personas`, up to 20 entries with unique ids, each with only `id`, `name`, `note`, `start` and `pages`.
pub fn parse_personas(bytes: &[u8]) -> PersonasResult {
    let invalid = || PersonasResult::problem(Problem::Invalid);
    let Ok(text) = std::str::from_utf8(bytes) else {
        return invalid();
    };
    let Ok(json) = serde_json::from_str::<Value>(text.strip_prefix('\u{FEFF}').unwrap_or(text))
    else {
        return invalid();
    };
    let Some(root) = json.as_object().filter(|o| only_keys(o, &["personas"])) else {
        return invalid();
    };
    let Some(entries) = root
        .get("personas")
        .and_then(Value::as_array)
        .filter(|l| l.len() <= MAX_PERSONAS)
    else {
        return invalid();
    };
    let Some(personas) = entries.iter().map(persona).collect::<Option<Vec<_>>>() else {
        return invalid();
    };
    let ids: HashSet<&str> = personas.iter().map(|p| p.id.as_str()).collect();
    if ids.len() != personas.len() {
        return invalid();
    }
    PersonasResult {
        problem: None,
        personas,
    }
}

/// `DESIGN.md` as text without a leading byte-order mark; `None` when it is not UTF-8.
pub fn parse_design(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    Some(text.strip_prefix('\u{FEFF}').unwrap_or(text).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_paths_follow_the_api_rules() {
        for good in [
            "pages/index.html",
            "pages/admin/orders.html",
            "pages/a.b_c-d/v1.2.html",
            "pages/a/b/c/d/e/f/g.html",
        ] {
            assert!(is_page_path(good), "{good}");
        }
        for bad in [
            "",
            "index.html",
            "pages/",
            "pages/.html",
            "pages/index",
            "pages/index.htm",
            "pages/.hidden.html",
            "pages/_x.html",
            "pages/a//b.html",
            "pages/../x.html",
            "pages/a/./b.html",
            "pages/a b.html",
            "pages/é.html",
            "pages/a\\b.html",
            "/pages/index.html",
            "pages/a/b/c/d/e/f/g/h.html",
        ] {
            assert!(!is_page_path(bad), "{bad:?}");
        }
        assert!(!is_page_path(&format!("pages/{}.html", "a".repeat(100))));
        assert!(is_page_path(&format!("pages/{}.html", "a".repeat(95))));
    }

    fn personas(json: &str) -> PersonasResult {
        parse_personas(json.as_bytes())
    }

    #[test]
    fn reads_the_documented_shape() {
        let result = personas(
            r#"{"personas":[
                {"id":"tamu","name":"  Tamu ","note":"Belum login","start":"index"},
                {"id":"admin","name":"Admin","start":"admin/orders","pages":["admin/*","index"]}
            ]}"#,
        );
        assert_eq!(result.problem, None);
        assert_eq!(result.personas.len(), 2);
        assert_eq!(result.personas[0].name, "Tamu");
        assert_eq!(result.personas[0].note.as_deref(), Some("Belum login"));
        assert_eq!(result.personas[0].pages, Vec::<String>::new());
        assert_eq!(result.personas[1].note, None);
        assert_eq!(result.personas[1].start.as_deref(), Some("admin/orders"));
        assert_eq!(result.personas[1].pages, ["admin/*", "index"]);
    }

    #[test]
    fn an_empty_list_is_valid() {
        assert_eq!(personas(r#"{"personas":[]}"#), PersonasResult::none());
    }

    #[test]
    fn refuses_whatever_the_api_refuses() {
        let long = "x".repeat(81);
        let long_note = "x".repeat(201);
        let many_pages = format!(
            r#"{{"personas":[{{"id":"a","name":"A","pages":[{}]}}]}}"#,
            vec![r#""x""#; 51].join(",")
        );
        let many = format!(
            r#"{{"personas":[{}]}}"#,
            (0..21)
                .map(|i| format!(r#"{{"id":"p{i}","name":"P"}}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        let deep = r#"{"personas":[{"id":"a","name":"A","start":"a/b/c/d/e/f/g/h"}]}"#;
        for (what, json) in [
            ("not json", "personas".to_string()),
            ("not an object", "[]".to_string()),
            (
                "extra top-level key",
                r#"{"personas":[],"x":1}"#.to_string(),
            ),
            ("no personas key", "{}".to_string()),
            (
                "extra persona key",
                r#"{"personas":[{"id":"a","name":"A","email":"x"}]}"#.to_string(),
            ),
            ("no id", r#"{"personas":[{"name":"A"}]}"#.to_string()),
            (
                "id with a space",
                r#"{"personas":[{"id":"a b","name":"A"}]}"#.to_string(),
            ),
            (
                "id starting with a dot",
                r#"{"personas":[{"id":".a","name":"A"}]}"#.to_string(),
            ),
            ("no name", r#"{"personas":[{"id":"a"}]}"#.to_string()),
            (
                "blank name",
                r#"{"personas":[{"id":"a","name":"   "}]}"#.to_string(),
            ),
            (
                "name too long",
                format!(r#"{{"personas":[{{"id":"a","name":"{long}"}}]}}"#),
            ),
            (
                "note too long",
                format!(r#"{{"personas":[{{"id":"a","name":"A","note":"{long_note}"}}]}}"#),
            ),
            (
                "null note",
                r#"{"personas":[{"id":"a","name":"A","note":null}]}"#.to_string(),
            ),
            (
                "control character in name",
                r#"{"personas":[{"id":"a","name":"A\u001b[31m"}]}"#.to_string(),
            ),
            (
                "bidi override in note",
                r#"{"personas":[{"id":"a","name":"A","note":"x\u202ey"}]}"#.to_string(),
            ),
            (
                "start with a star",
                r#"{"personas":[{"id":"a","name":"A","start":"admin/*"}]}"#.to_string(),
            ),
            (
                "start climbing out",
                r#"{"personas":[{"id":"a","name":"A","start":"../x"}]}"#.to_string(),
            ),
            ("start too deep", deep.to_string()),
            (
                "star in the middle",
                r#"{"personas":[{"id":"a","name":"A","pages":["*/x"]}]}"#.to_string(),
            ),
            (
                "pages not a list",
                r#"{"personas":[{"id":"a","name":"A","pages":"*"}]}"#.to_string(),
            ),
            ("too many pages", many_pages),
            ("too many personas", many),
            (
                "duplicate ids",
                r#"{"personas":[{"id":"a","name":"A"},{"id":"a","name":"B"}]}"#.to_string(),
            ),
        ] {
            assert_eq!(
                personas(&json),
                PersonasResult::problem(Problem::Invalid),
                "{what}"
            );
        }
    }

    #[test]
    fn counts_the_length_of_a_name_the_way_the_api_does() {
        // 80 UTF-16 units: 40 astral characters are 80 units, 41 are 82.
        let ok = "😀".repeat(40);
        let over = "😀".repeat(41);
        assert_eq!(
            personas(&format!(r#"{{"personas":[{{"id":"a","name":"{ok}"}}]}}"#)).problem,
            None
        );
        assert_eq!(
            personas(&format!(r#"{{"personas":[{{"id":"a","name":"{over}"}}]}}"#)).problem,
            Some(Problem::Invalid)
        );
    }

    #[test]
    fn accepts_a_byte_order_mark_and_refuses_bytes_that_are_not_text() {
        let mut with_bom = "\u{FEFF}".as_bytes().to_vec();
        with_bom.extend(br#"{"personas":[]}"#);
        assert_eq!(parse_personas(&with_bom).problem, None);
        assert_eq!(
            parse_personas(&[0xff, 0xfe, b'{']).problem,
            Some(Problem::Invalid)
        );
    }

    #[test]
    fn design_drops_a_leading_byte_order_mark_and_nothing_else() {
        assert_eq!(
            parse_design("\u{FEFF}# T\n".as_bytes()).as_deref(),
            Some("# T\n")
        );
        assert_eq!(parse_design(b"  # T  \n").as_deref(), Some("  # T  \n"));
        assert_eq!(parse_design(b"").as_deref(), Some(""));
        assert_eq!(parse_design(&[0xff, 0xfe]), None);
    }
}
