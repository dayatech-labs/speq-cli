//! `speq impact <epic>[/<story>] <AC-NN|BR-NN>`: what a change to one acceptance criterion or business rule reaches,
//! read from the working tree (no network, nothing written): where it is written, the documents of the epic that
//! cite it, the tasks that implement it with their status and ticket, and the code lines that claim it.
//! Also the warning `speq push` prints when the pushed commits change acceptance criteria a `DONE` task still names.
use crate::context::{documents, shown};
use crate::error::{Error, Result};
use crate::git::Git;
use crate::paths::{is_folder_name, is_story_folder};
use crate::squash::UPSTREAM;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Code files larger than this are skipped: bundles and fixtures, not tests.
const MAX_CODE_FILE: u64 = 1 << 20;
/// A range such as `AC-10 sampai AC-14` expands only when it spans fewer criteria than this.
const MAX_RANGE: u32 = 100;
/// Words that join the two ends of a range of criteria.
const RANGE_WORDS: [&str; 6] = ["sampai", "s/d", "to", "–", "—", "-"];
/// Line excerpts in the report are cut to this many characters.
const EXCERPT: usize = 140;

/// One line of a document or of code.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Line {
    pub path: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct TaskImpact {
    pub path: String,
    pub status: Option<String>,
    pub ticket_id: Option<String>,
    /// What the task names: criteria as `st-NNN AC-NN`, and the rule itself.
    pub names: Vec<String>,
    /// A `DONE` task is reopened before what it implements changes.
    pub reopen: bool,
}

#[derive(Debug, Serialize)]
pub struct ImpactReport {
    /// The Speq project; absent when the folder is a plain clone that speq did not make.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub head_sha: String,
    /// `AC-NN` or `BR-NN`.
    pub id: String,
    /// The epic, or `<epic>/<story>` for a criterion.
    pub scope: String,
    /// Where the criterion or rule is written.
    pub sources: Vec<Line>,
    /// Other documents of the epic that cite it (tasks are listed apart).
    pub mentions: Vec<Line>,
    /// The criteria affected, as `st-NNN AC-NN`.
    pub criteria: Vec<String>,
    pub tasks: Vec<TaskImpact>,
    /// The code repositories searched for claims.
    pub code_dirs: Vec<String>,
    pub code: Vec<Line>,
    /// Code lines that name the story without an epic while another epic has a story with the same number: they may
    /// be that epic's. They still count as claims.
    pub code_no_epic: Vec<Line>,
    /// Criteria no code line claims; empty when no code was searched.
    pub unclaimed: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Id {
    Ac(u32),
    Br(u32),
}

impl Id {
    fn parse(id: &str) -> Result<Id> {
        let upper = id.to_ascii_uppercase();
        let number = |prefix: &str| {
            upper
                .strip_prefix(prefix)
                .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|n| n.parse().ok())
        };
        match (number("AC-"), number("BR-")) {
            (Some(n), _) => Ok(Id::Ac(n)),
            (_, Some(n)) => Ok(Id::Br(n)),
            _ => Err(Error::invalid(
                "the ID is an acceptance criterion `AC-NN` or a business rule `BR-NN`, for example `AC-04`",
            )),
        }
    }
}

fn ac(n: u32) -> String {
    format!("AC-{n:02}")
}

fn criterion((story, n): &(String, u32)) -> String {
    format!("{story} {}", ac(*n))
}

/// `st-NNN` from a story folder name `st-NNN-<name>`.
fn story_id(folder: &str) -> Option<&str> {
    let end = folder.match_indices('-').nth(1)?.0;
    Some(&folder[..end])
}

/// The story (`st-NNN`) a document path belongs to, if any.
fn story_of(path: &str) -> Option<&str> {
    let rest = path.split_once("/stories/")?.1;
    story_id(rest.split('/').next()?)
}

/// The digits at the start of `s` and where they end.
fn leading_number(s: &str) -> Option<(u32, usize)> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    Some((s[..end].parse().ok()?, end))
}

/// `word` occurs in `text` as a whole token: no letter or digit right before it, no digit right after.
fn mentions(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(|c| c.is_ascii_digit())
    })
}

/// The story ID (`st-NNN`) written right before position `at`, as in `st-004 AC-11`.
fn story_before(text: &str) -> Option<String> {
    let text = text.trim_end();
    let start = text
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .map_or(0, |i| {
            i + text[i..].chars().next().map_or(1, char::len_utf8)
        });
    let token = &text[start..];
    let digits = token.strip_prefix("st-")?;
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then(|| token.to_string())
}

/// Acceptance-criterion references in `text` as `(story, number)`. The story is the `st-NNN` written right before
/// the ID (a reference to another story's criterion); otherwise it is `own`, the story the text belongs to.
/// Ranges (`AC-10 sampai AC-14`, `AC-10–AC-14`) expand to every criterion in them.
fn ac_refs(text: &str, own: Option<&str>) -> BTreeSet<(String, u32)> {
    let mut refs = BTreeSet::new();
    for (at, _) in text.match_indices("AC-") {
        if text[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let Some((first, end)) = leading_number(&text[at + 3..]) else {
            continue;
        };
        let Some(story) = story_before(&text[..at]).or_else(|| own.map(str::to_string)) else {
            continue;
        };
        let after = text[at + 3 + end..].trim_start();
        let last = RANGE_WORDS
            .iter()
            .find_map(|word| after.strip_prefix(word))
            .and_then(|rest| rest.trim_start().strip_prefix("AC-"))
            .and_then(leading_number)
            .map(|(n, _)| n)
            .filter(|n| *n > first && n - first < MAX_RANGE)
            .unwrap_or(first);
        refs.extend((first..=last).map(|n| (story.clone(), n)));
    }
    refs
}

/// The criterion a heading line starts: `### AC-NN — ...`, or the older `**AC-NN: ...**`.
fn ac_heading(line: &str) -> Option<u32> {
    let rest = line
        .strip_prefix("### AC-")
        .or_else(|| line.strip_prefix("**AC-"))?;
    leading_number(rest).map(|(n, _)| n)
}

/// The sections of an acceptance-criteria document, by criterion number: the heading and its body, with trailing
/// spaces and blank lines dropped so only real edits count as a change.
fn ac_sections(text: &str) -> BTreeMap<u32, String> {
    let mut sections: BTreeMap<u32, String> = BTreeMap::new();
    let mut current = None;
    for line in text.lines() {
        if line.starts_with("## ") || line.starts_with("### ") || line.starts_with("**AC-") {
            current = ac_heading(line);
        }
        if let Some(n) = current {
            let section = sections.entry(n).or_default();
            section.push_str(line.trim_end());
            section.push('\n');
        }
    }
    for section in sections.values_mut() {
        section.truncate(section.trim_end().len());
    }
    sections
}

/// A key of the front matter (`status`, `ticket-id`).
fn front_matter(text: &str, key: &str) -> Option<String> {
    let mut lines = text.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    lines
        .take_while(|l| l.trim_end() != "---")
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn excerpt(line: &str) -> String {
    let line = line.trim();
    match line.char_indices().nth(EXCERPT) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_string(),
    }
}

fn is_task(path: &str) -> bool {
    path.contains("/tasks/")
}

/// What one text (a line, or a whole task) names among `criteria`, plus the rule when it mentions it.
fn named(
    text: &str,
    own: Option<&str>,
    criteria: &BTreeSet<(String, u32)>,
    rule: Option<&str>,
) -> Vec<String> {
    let mut names: Vec<String> = ac_refs(text, own)
        .intersection(criteria)
        .map(criterion)
        .collect();
    if let Some(rule) = rule.filter(|r| mentions(text, r)) {
        names.push(rule.to_string());
    }
    names
}

/// Lines in the code repositories that name a story and one of its criteria on the same line, as tests do:
/// `test('... (st-008 AC-05)')`, `describe('... st-009 AC-01, AC-02')`. Each claim is `(line, criterion)`, with
/// the path under the repository folder as the user named it.
/// Story numbers restart in every epic, so a line that names another epic and not this one is that epic's claim,
/// and a line that names no epic is marked unsure when another epic has a story with the same number.
fn code_claims(
    git: &Git,
    cwd: &Path,
    dirs: &[String],
    criteria: &BTreeSet<(String, u32)>,
    epic: &str,
    other_epics: &BTreeSet<&str>,
    other_stories: &BTreeSet<&str>,
) -> Result<Vec<(Line, String, bool)>> {
    let mut claims = Vec::new();
    for label in dirs {
        let dir = cwd.join(label);
        let listed = git.output(Some(&dir), ["ls-files", "-z"])?;
        if !listed.status.success() {
            return Err(Error::invalid(format!(
                "{} is not a Git repository; --code takes the folder of a code repository",
                dir.display()
            )));
        }
        for file in listed
            .stdout
            .split(|b| *b == 0)
            .filter_map(|p| std::str::from_utf8(p).ok())
        {
            let path = dir.join(file);
            if std::fs::metadata(&path).map_or(true, |m| !m.is_file() || m.len() > MAX_CODE_FILE) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue; // not UTF-8: not source code
            };
            for (i, line) in text.lines().enumerate().filter(|(_, l)| l.contains("AC-")) {
                // An epic is named the way claims write it, right before the story: `test-catalog st-009 AC-01`.
                let names = |e: &str| line.contains(&format!("{e} st-"));
                let names_this_epic = names(epic);
                if !names_this_epic && other_epics.iter().any(|e| names(e)) {
                    continue;
                }
                let numbers: BTreeSet<u32> = ac_refs(line, Some(""))
                    .into_iter()
                    .map(|(_, n)| n)
                    .collect();
                if let Some(c) = criteria
                    .iter()
                    .find(|(story, n)| numbers.contains(n) && mentions(line, story))
                {
                    let line = Line {
                        path: format!("{label}/{file}"),
                        line: i + 1,
                        text: excerpt(line),
                    };
                    // Naming no epic, `(st-008 AC-05)` may be about `st-008` of another epic just as well.
                    let unsure = !names_this_epic && other_stories.contains(c.0.as_str());
                    claims.push((line, criterion(c), unsure));
                }
            }
        }
    }
    Ok(claims)
}

/// The repository around the specs folder (`./specs` of an implementation repository), searched when no `--code`
/// folder is given.
fn surrounding_repo(git: &Git, root: &Path) -> Option<String> {
    let parent = root.parent()?;
    let top = git
        .text(Some(parent), ["rev-parse", "--show-toplevel"])
        .ok()?;
    Some(top.trim().to_string())
}

pub fn impact(
    git: &Git,
    root: &Path,
    project_id: Option<String>,
    cwd: &Path,
    target: &str,
    id: &str,
    code: &[String],
) -> Result<ImpactReport> {
    let id = Id::parse(id)?;
    let (epic, story) = match target.split('/').collect::<Vec<_>>()[..] {
        [epic] if is_folder_name(epic) => (epic, None),
        [epic, story] if is_folder_name(epic) && is_story_folder(story) => (epic, Some(story)),
        _ => {
            return Err(Error::invalid(
                "use `speq impact <epic>/<story> AC-NN` or `speq impact <epic> BR-NN`, for example `speq impact epic-payment/st-001-create-payment AC-04`",
            ));
        }
    };
    let files = documents(git, root)?;
    let epic_dir = format!("epics/{epic}/");
    let docs: Vec<(&String, String)> = files
        .iter()
        .filter(|p| p.starts_with(&epic_dir) && p.ends_with(".md"))
        .map(|p| (p, std::fs::read_to_string(root.join(p)).unwrap_or_default()))
        .collect();
    let show = |path: &str, line: usize, text: &str| Line {
        path: shown(root, cwd, path),
        line,
        text: excerpt(text),
    };

    // `(document, line)` of each source, to keep them out of the mentions.
    let mut source_lines: BTreeSet<(&str, usize)> = BTreeSet::new();
    let mut sources = Vec::new();
    let mut criteria = BTreeSet::new();
    let (scope, rule) = match (id, story) {
        (Id::Ac(n), Some(story)) => {
            let path = format!("{epic_dir}stories/{story}/acceptance-criteria.md");
            let heading = docs
                .iter()
                .find(|(p, _)| **p == path)
                .and_then(|(_, text)| {
                    text.lines()
                        .enumerate()
                        .find(|(_, l)| ac_heading(l) == Some(n))
                });
            let Some((i, line)) = heading else {
                return Err(Error::invalid(format!(
                    "no {} heading found in {path}",
                    ac(n)
                )));
            };
            sources.push(show(&path, i + 1, line));
            if let Some((p, _)) = docs.iter().find(|(p, _)| **p == path) {
                source_lines.insert((p.as_str(), i + 1));
            }
            let own = story_id(story).unwrap_or(story).to_string();
            criteria.insert((own, n));
            (format!("{epic}/{story}"), None)
        }
        (Id::Ac(_), None) => {
            return Err(Error::invalid(
                "a criterion belongs to a story: use `speq impact <epic>/<story> AC-NN`",
            ));
        }
        (Id::Br(n), _) => {
            let rule = format!("BR-{n:02}");
            let prd = format!("{epic_dir}prd.md");
            for (path, text) in docs.iter().filter(|(p, _)| **p == prd) {
                for (i, line) in text.lines().enumerate().filter(|(_, l)| mentions(l, &rule)) {
                    sources.push(show(path, i + 1, line));
                    source_lines.insert((path.as_str(), i + 1));
                }
            }
            if sources.is_empty() {
                return Err(Error::invalid(format!(
                    "{rule} is not in {prd}; business rules are numbered in the PRD"
                )));
            }
            for (path, text) in docs
                .iter()
                .filter(|(p, _)| p.ends_with("/acceptance-criteria.md"))
            {
                let Some(own) = story_of(path) else { continue };
                for (k, section) in ac_sections(text) {
                    if mentions(&section, &rule) {
                        criteria.insert((own.to_string(), k));
                    }
                }
            }
            (epic.to_string(), Some(rule))
        }
    };

    let mut mentions_found = Vec::new();
    let mut tasks = Vec::new();
    for (path, text) in &docs {
        let own = story_of(path);
        if is_task(path) {
            let names = named(text, own, &criteria, rule.as_deref());
            if !names.is_empty() {
                let status = front_matter(text, "status");
                tasks.push(TaskImpact {
                    path: shown(root, cwd, path),
                    reopen: status.as_deref() == Some("DONE"),
                    status,
                    ticket_id: front_matter(text, "ticket-id"),
                    names,
                });
            }
            continue;
        }
        // For a rule, its criteria are listed by ID already; their lines would only repeat them.
        if rule.is_some() && path.ends_with("/acceptance-criteria.md") {
            continue;
        }
        for (i, line) in text.lines().enumerate() {
            let at_source = source_lines.contains(&(path.as_str(), i + 1));
            if !at_source && !named(line, own, &criteria, rule.as_deref()).is_empty() {
                mentions_found.push(show(path, i + 1, line));
            }
        }
    }

    let code_dirs: Vec<String> = if code.is_empty() {
        surrounding_repo(git, root).into_iter().collect()
    } else {
        code.iter()
            .map(|d| d.trim_end_matches(['/', '\\']).to_string())
            .collect()
    };
    let other_epics: BTreeSet<&str> = files
        .iter()
        .filter_map(|p| p.strip_prefix("epics/")?.split_once('/'))
        .map(|(e, _)| e)
        .filter(|e| *e != epic)
        .collect();
    let other_stories: BTreeSet<&str> = files
        .iter()
        .filter(|p| !p.starts_with(&epic_dir))
        .filter_map(|p| story_of(p))
        .collect();
    let claims = code_claims(
        git,
        cwd,
        &code_dirs,
        &criteria,
        epic,
        &other_epics,
        &other_stories,
    )?;
    let claimed: BTreeSet<&String> = claims.iter().map(|(_, c, _)| c).collect();
    let unclaimed = if code_dirs.is_empty() {
        Vec::new()
    } else {
        criteria
            .iter()
            .map(criterion)
            .filter(|c| !claimed.contains(c))
            .collect()
    };
    let (unsure, sure): (Vec<_>, Vec<_>) = claims.into_iter().partition(|(_, _, unsure)| *unsure);
    let head = git.output(Some(root), ["rev-parse", "--verify", "-q", "HEAD"])?;
    Ok(ImpactReport {
        project_id,
        head_sha: String::from_utf8_lossy(&head.stdout).trim().to_string(),
        id: match id {
            Id::Ac(n) => ac(n),
            Id::Br(n) => format!("BR-{n:02}"),
        },
        scope,
        sources,
        mentions: mentions_found,
        criteria: criteria.iter().map(criterion).collect(),
        tasks,
        code: sure.into_iter().map(|(line, _, _)| line).collect(),
        code_no_epic: unsure.into_iter().map(|(line, _, _)| line).collect(),
        code_dirs,
        unclaimed,
    })
}

/// A file as committed at `rev`; empty when it is not there.
fn committed(git: &Git, dir: Option<&Path>, rev: &str, path: &str) -> String {
    git.output(dir, ["show", &format!("{rev}:{path}")])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// The lines of a PRD that name each business rule, by rule number: what tells whether a commit changed a rule.
fn rule_lines(prd: &str) -> BTreeMap<u32, Vec<&str>> {
    let mut rules: BTreeMap<u32, Vec<&str>> = BTreeMap::new();
    for line in prd.lines() {
        for (at, _) in line.match_indices("BR-") {
            if line[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
            {
                continue;
            }
            if let Some((n, _)) = leading_number(&line[at + 3..]) {
                rules.entry(n).or_default().push(line.trim_end());
            }
        }
    }
    rules
}

fn nul_separated(bytes: &[u8]) -> impl Iterator<Item = &str> {
    bytes
        .split(|b| *b == 0)
        .filter_map(|p| std::str::from_utf8(p).ok())
        .filter(|p| !p.is_empty())
}

/// The criteria and the rules a push changes in one epic.
type Changes = (BTreeSet<(String, u32)>, BTreeSet<String>);

/// For `speq push`: the `DONE` tasks of an epic that name an acceptance criterion or a business rule the unpushed
/// commits changed or removed, in any story of that epic. A changed rule also reaches every criterion that names it.
/// Best effort and silent on any failure: it only warns, the push always goes ahead.
pub fn push_warning(git: &Git, dir: Option<&Path>) -> Option<String> {
    let changed = git
        .bytes(dir, ["diff", "--name-only", "-z", UPSTREAM, "HEAD"])
        .ok()?;
    // Per epic: the criteria and the rules the commits changed.
    let mut epics: BTreeMap<&str, Changes> = BTreeMap::new();
    for path in nul_separated(&changed) {
        let Some((epic, rest)) = path.strip_prefix("epics/").and_then(|p| p.split_once('/')) else {
            continue;
        };
        let both = || {
            (
                committed(git, dir, UPSTREAM, path),
                committed(git, dir, "HEAD", path),
            )
        };
        if rest == "prd.md" {
            let (before, after) = both();
            let (before, after) = (rule_lines(&before), rule_lines(&after));
            let rules = before
                .iter()
                .filter(|(n, lines)| after.get(n) != Some(lines))
                .map(|(n, _)| format!("BR-{n:02}"));
            epics.entry(epic).or_default().1.extend(rules);
        } else if let (true, Some(own)) =
            (rest.ends_with("/acceptance-criteria.md"), story_of(path))
        {
            let (before, after) = both();
            let (before, after) = (ac_sections(&before), ac_sections(&after));
            let edited = before
                .iter()
                .filter(|(n, section)| after.get(n) != Some(section))
                .map(|(n, _)| (own.to_string(), *n));
            epics.entry(epic).or_default().0.extend(edited);
        }
    }

    let mut lines = Vec::new();
    for (epic, (mut criteria, rules)) in epics {
        if criteria.is_empty() && rules.is_empty() {
            continue;
        }
        let stories = format!("epics/{epic}/stories/");
        if !rules.is_empty() {
            let listed = git
                .bytes(
                    dir,
                    ["ls-tree", "-r", "-z", "--name-only", "HEAD", "--", &stories],
                )
                .ok()?;
            for path in nul_separated(&listed).filter(|p| p.ends_with("/acceptance-criteria.md")) {
                let Some(own) = story_of(path) else { continue };
                for (n, section) in ac_sections(&committed(git, dir, "HEAD", path)) {
                    if rules.iter().any(|r| mentions(&section, r)) {
                        criteria.insert((own.to_string(), n));
                    }
                }
            }
        }
        // Only the tasks that may be `DONE` are read; `git grep` exits 1 when there are none.
        let done = git
            .output(
                dir,
                [
                    "grep",
                    "-l",
                    "-z",
                    "-E",
                    "^status:[[:space:]]*DONE",
                    "HEAD",
                    "--",
                    &stories,
                ],
            )
            .ok()?;
        for task in nul_separated(&done.stdout)
            .filter_map(|p| p.strip_prefix("HEAD:"))
            .filter(|p| is_task(p) && p.ends_with(".md"))
        {
            let text = committed(git, dir, "HEAD", task);
            if front_matter(&text, "status").as_deref() != Some("DONE") {
                continue;
            }
            let mut names = named(&text, story_of(task), &criteria, None);
            names.extend(rules.iter().filter(|r| mentions(&text, r)).cloned());
            if !names.is_empty() {
                let ticket = front_matter(&text, "ticket-id").unwrap_or_else(|| "no ticket".into());
                lines.push(format!("  {task} ({ticket}): {}", names.join(", ")));
            }
        }
    }
    (!lines.is_empty()).then(|| {
        format!(
            "speq: this push changes acceptance criteria or business rules that tasks marked DONE still name:\n{}\nIf what they must do changed, set those tasks back to IN-PROGRESS and check the rest with `speq impact <epic>/<story> AC-NN` or `speq impact <epic> BR-NN`. The push goes ahead.",
            lines.join("\n")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(text: &str) -> Vec<String> {
        ac_refs(text, Some("st-001"))
            .iter()
            .map(criterion)
            .collect()
    }

    #[test]
    fn ids_are_criteria_or_rules_in_any_case() {
        assert_eq!(Id::parse("AC-04").unwrap(), Id::Ac(4));
        assert_eq!(Id::parse("br-12").unwrap(), Id::Br(12));
        for bad in ["", "AC-", "AC-x1", "FR-01", "AC04", "§9 aturan 1"] {
            assert!(Id::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn criteria_belong_to_the_own_story_unless_another_story_is_named_before_them() {
        assert_eq!(
            refs("[AC-01, AC-02](../acceptance-criteria.md)"),
            ["st-001 AC-01", "st-001 AC-02"]
        );
        assert_eq!(
            refs(
                "[st-004 AC-11](../../st-004-x/acceptance-criteria.md); [AC-03](../acceptance-criteria.md)"
            ),
            ["st-001 AC-03", "st-004 AC-11"]
        );
        assert!(
            ac_refs("AC-01", None).is_empty(),
            "no story to attach it to"
        );
    }

    #[test]
    fn ranges_expand_and_absurd_ranges_do_not() {
        assert_eq!(
            refs("AC-10 sampai AC-12, AC-20"),
            [
                "st-001 AC-10",
                "st-001 AC-11",
                "st-001 AC-12",
                "st-001 AC-20"
            ]
        );
        assert_eq!(refs("AC-03–AC-04"), ["st-001 AC-03", "st-001 AC-04"]);
        assert_eq!(
            refs("AC-05 sampai AC-02"),
            ["st-001 AC-02", "st-001 AC-05"],
            "backwards: both ends only"
        );
        assert_eq!(
            refs("AC-01 sampai AC-999").len(),
            2,
            "too wide: both ends only"
        );
    }

    #[test]
    fn ids_inside_other_words_or_numbers_are_not_references() {
        assert!(refs("XAC-01 and AC-x").is_empty());
        assert!(mentions("aturan BR-03.", "BR-03"));
        assert!(!mentions("BR-030", "BR-03"));
        assert!(!mentions("XBR-03", "BR-03"));
        assert!(mentions("(st-008 AC-05)", "st-008"));
        assert!(!mentions("st-0081", "st-008"));
    }

    #[test]
    fn stories_come_from_the_folder_name() {
        assert_eq!(story_id("st-008-login"), Some("st-008"));
        assert_eq!(
            story_of("epics/a/stories/st-008-login/tasks/be-001-x.md"),
            Some("st-008")
        );
        assert_eq!(story_of("epics/a/prd.md"), None);
    }

    #[test]
    fn sections_split_on_criterion_headings_and_ignore_trailing_space() {
        let text = "# AC\n\n### AC-01 — a (BR-01)\nGiven x  \n\n### AC-02 — b\nThen y\n\n## Verification\nnot a criterion\n";
        let sections = ac_sections(text);
        assert_eq!(sections.keys().copied().collect::<Vec<_>>(), [1, 2]);
        assert_eq!(sections[&1], "### AC-01 — a (BR-01)\nGiven x");
        assert_eq!(sections[&2], "### AC-02 — b\nThen y");
    }

    const STORY: &str = "epics/pay/stories/st-001-charge";

    /// A specs repository with one rule, two criteria, and tasks in two stories, plus another epic that reuses the
    /// same story number and rule number.
    fn specs(git: &Git) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let files = [
            (
                "epics/pay/prd.md",
                "## 6. Business rules\n\n- **BR-01** Charge once.\n- **BR-010** Other.\n",
            ),
            (
                &format!("{STORY}/README.md"),
                "## 9. Aturan\n\nBerlaku BR-01.\n",
            ),
            (
                &format!("{STORY}/acceptance-criteria.md"),
                "# AC\n\n### AC-01 — charged once (BR-01)\nGiven a card\n\n### AC-02 — declined (Perilaku 2)\nThen 402\n",
            ),
            (
                &format!("{STORY}/tasks/be-001-api.md"),
                "---\nticket-id: DEV-1\nstatus: DONE\n---\n\n| **AC** | [AC-01, AC-02](../acceptance-criteria.md) |\n",
            ),
            (
                &format!("{STORY}/tasks/fe-001-ui.md"),
                "---\nstatus: TODO\n---\n\n| **AC** | [AC-02](../acceptance-criteria.md) |\n",
            ),
            (
                "epics/pay/stories/st-002-refund/tasks/be-001-refund.md",
                "---\nstatus: IN-PROGRESS\n---\n\nReuses [st-001 AC-01](../../st-001-charge/acceptance-criteria.md).\n",
            ),
            (
                "epics/shop/stories/st-001-cart/acceptance-criteria.md",
                "### AC-01 — cart (BR-01)\n",
            ),
        ];
        for (path, text) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        git.text(Some(dir.path()), ["init", "-q"]).unwrap();
        dir
    }

    /// A code repository with a claim of `st-001 AC-01` naming no epic, one of the other epic's, and one of `pay`'s.
    fn code_repo(git: &Git) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("charge.test.ts"),
            "test('charges once (st-001 AC-01)')\ntest('cart (shop st-001 AC-01)')\ntest('retry (pay st-001 AC-01)')\n",
        )
        .unwrap();
        git.text(Some(dir.path()), ["init", "-q"]).unwrap();
        git.text(Some(dir.path()), ["add", "-A"]).unwrap();
        dir
    }

    fn run(git: &Git, specs: &Path, code: &Path, target: &str, id: &str) -> ImpactReport {
        let code = vec![code.to_string_lossy().into_owned()];
        impact(git, specs, None, specs, target, id, &code).unwrap()
    }

    fn task_names(report: &ImpactReport) -> Vec<(String, bool)> {
        report
            .tasks
            .iter()
            .map(|t| (format!("{} {}", t.path, t.names.join(",")), t.reopen))
            .collect()
    }

    #[test]
    fn a_rule_reaches_its_criteria_their_tasks_and_the_code_that_claims_them() {
        let git = Git::locate(None).unwrap();
        let (specs, code) = (specs(&git), code_repo(&git));
        let report = run(&git, specs.path(), code.path(), "pay", "br-1");

        assert_eq!(report.id, "BR-01");
        assert_eq!(
            report.sources.iter().map(|l| l.line).collect::<Vec<_>>(),
            [3],
            "BR-010 is another rule"
        );
        assert_eq!(report.mentions.len(), 1, "the story README cites it");
        assert_eq!(
            report.criteria,
            ["st-001 AC-01"],
            "only this epic's criteria"
        );
        assert_eq!(
            task_names(&report),
            [
                (format!("{STORY}/tasks/be-001-api.md st-001 AC-01"), true),
                (
                    "epics/pay/stories/st-002-refund/tasks/be-001-refund.md st-001 AC-01"
                        .to_string(),
                    false
                ),
            ]
        );
        assert_eq!(report.tasks[0].ticket_id.as_deref(), Some("DEV-1"));
        let lines = |code: &[Line]| code.iter().map(|l| l.line).collect::<Vec<_>>();
        assert_eq!(
            lines(&report.code),
            [3],
            "the other epic's claim is left out"
        );
        assert!(report.code[0].path.ends_with("/charge.test.ts"));
        assert_eq!(
            lines(&report.code_no_epic),
            [1],
            "shop has an st-001 too, so a claim naming no epic is unsure"
        );
        assert!(report.unclaimed.is_empty());
    }

    #[test]
    fn a_criterion_reaches_the_tasks_that_name_it_and_is_unclaimed_without_a_test() {
        let git = Git::locate(None).unwrap();
        let (specs, code) = (specs(&git), code_repo(&git));
        let report = run(
            &git,
            specs.path(),
            code.path(),
            "pay/st-001-charge",
            "AC-02",
        );

        assert_eq!(report.sources[0].text, "### AC-02 — declined (Perilaku 2)");
        assert_eq!(
            task_names(&report),
            [
                (format!("{STORY}/tasks/be-001-api.md st-001 AC-02"), true),
                (format!("{STORY}/tasks/fe-001-ui.md st-001 AC-02"), false),
            ]
        );
        assert_eq!(report.unclaimed, ["st-001 AC-02"]);
    }

    #[test]
    fn unknown_ids_and_targets_are_refused_with_the_expected_shape() {
        let git = Git::locate(None).unwrap();
        let (specs, code) = (specs(&git), code_repo(&git));
        let code = vec![code.path().to_string_lossy().into_owned()];
        let err = |target: &str, id: &str| {
            impact(&git, specs.path(), None, specs.path(), target, id, &code)
                .unwrap_err()
                .to_string()
        };
        assert!(err("pay/st-001-charge", "AC-09").contains("no AC-09 heading"));
        assert!(err("pay", "BR-07").contains("BR-07 is not in"));
        assert!(err("pay", "AC-01").contains("belongs to a story"));
        assert!(err("pay/nope", "AC-01").contains("speq impact <epic>/<story> AC-NN"));
    }

    #[test]
    fn a_push_warns_about_done_tasks_whose_criteria_changed_and_nothing_else() {
        let git = Git::locate(None).unwrap();
        let specs = specs(&git);
        let dir = Some(specs.path());
        let commit = |message: &str| {
            git.text(dir, ["add", "-A"]).unwrap();
            git.text(
                dir,
                [
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@example.com",
                    "commit",
                    "-qm",
                    message,
                ],
            )
            .unwrap();
        };
        commit("docs: first");
        git.text(dir, ["update-ref", UPSTREAM, "HEAD"]).unwrap();
        assert_eq!(push_warning(&git, dir), None, "nothing to push");

        let criteria = specs.path().join(STORY).join("acceptance-criteria.md");
        let text = std::fs::read_to_string(&criteria).unwrap();
        std::fs::write(&criteria, text.replace("Then 402", "Then 402   \n\n")).unwrap();
        commit("docs: whitespace");
        assert_eq!(
            push_warning(&git, dir),
            None,
            "trailing space is not a change"
        );

        std::fs::write(&criteria, text.replace("Then 402", "Then 409")).unwrap();
        commit("docs: change AC-02");
        let warning = push_warning(&git, dir).expect("a warning");
        assert!(
            warning.contains(&format!(
                "{STORY}/tasks/be-001-api.md (DEV-1): st-001 AC-02"
            )),
            "{warning}"
        );
        assert!(
            !warning.contains("fe-001"),
            "a TODO task needs no reopening: {warning}"
        );
    }

    #[test]
    fn a_push_warns_about_done_tasks_of_other_stories_and_about_changed_rules() {
        let git = Git::locate(None).unwrap();
        let specs = specs(&git);
        let dir = Some(specs.path());
        let commit = |message: &str| {
            git.text(dir, ["add", "-A"]).unwrap();
            git.text(
                dir,
                [
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@example.com",
                    "commit",
                    "-qm",
                    message,
                ],
            )
            .unwrap();
        };
        let edit = |path: &str, from: &str, to: &str| {
            let path = specs.path().join(path);
            let text = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, text.replacen(from, to, 1)).unwrap();
        };
        let refund = "epics/pay/stories/st-002-refund/tasks/be-001-refund.md";
        edit(refund, "IN-PROGRESS", "DONE");
        commit("docs: first");
        git.text(dir, ["update-ref", UPSTREAM, "HEAD"]).unwrap();

        edit(
            &format!("{STORY}/acceptance-criteria.md"),
            "Given a card",
            "Given a saved card",
        );
        commit("docs: change st-001 AC-01");
        let warning = push_warning(&git, dir).expect("a warning");
        assert!(
            warning.contains(&format!("{refund} (no ticket): st-001 AC-01")),
            "a task of another story that names the criterion: {warning}"
        );
        git.text(dir, ["update-ref", UPSTREAM, "HEAD"]).unwrap();

        edit("epics/pay/prd.md", "Charge once.", "Charge at most once.");
        commit("docs: change BR-01");
        let warning = push_warning(&git, dir).expect("a warning");
        assert!(
            warning.contains(&format!(
                "{STORY}/tasks/be-001-api.md (DEV-1): st-001 AC-01"
            )),
            "the rule reaches the criterion that names it: {warning}"
        );
        assert!(warning.contains(refund), "{warning}");
        git.text(dir, ["update-ref", UPSTREAM, "HEAD"]).unwrap();

        edit(
            "epics/pay/prd.md",
            "## 6. Business rules",
            "## 6. Aturan bisnis",
        );
        commit("docs: rename a heading");
        assert_eq!(
            push_warning(&git, dir),
            None,
            "a PRD line that names no rule changes no rule"
        );
    }

    #[test]
    fn front_matter_keys_are_read_only_from_the_front_matter() {
        let task = "---\r\nticket-id: EG-1\r\nstatus: DONE\r\n---\r\n\r\nstatus: TODO\r\n";
        assert_eq!(front_matter(task, "status").as_deref(), Some("DONE"));
        assert_eq!(front_matter(task, "ticket-id").as_deref(), Some("EG-1"));
        assert_eq!(
            front_matter("# no front matter\nstatus: DONE\n", "status"),
            None
        );
    }
}
