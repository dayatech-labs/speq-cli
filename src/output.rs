//! Human-readable output (default) and `--json`. Machine output is the report struct itself.
//! Every text layout is built with the one template in [`crate::template`].
use crate::api::forges_output;
use crate::clone::{CloneReport, ListReport};
use crate::context::ContextReport;
use crate::dayaboard::{Connection, Revocation};
use crate::deinit::DeinitReport;
use crate::error::{Error, exit};
use crate::impact::{ImpactReport, Line as ImpactLine};
use crate::info::{DayaboardState, InfoReport, PlatformRole};
use crate::session::{LoginReport, LogoutReport};
use crate::skills::SkillsReport;
use crate::template::{Item, Screen, Theme, Tone};
use crate::ui::{DesignReport, PersonasReport, UiPageReport, terminal_text};
use crate::uninstall::UninstallReport;
use crate::upgrade::{CheckReport, UpgradeReport};
use serde::Serialize;
use std::io::IsTerminal;

pub trait Human: Serialize {
    fn human(&self, theme: &Theme) -> String;
    /// Non-zero when the command succeeded but found something the user must resolve.
    fn exit_code(&self) -> u8 {
        exit::OK
    }
}

/// `json` made safe to show on a terminal. serde_json escapes only the C0 controls, quotes and backslashes, so the C1
/// controls (U+009B is a one-character CSI), DEL and the invisible characters (bidirectional overrides, zero-width
/// marks, the Unicode tag block) would reach the terminal raw. The JSON structure is ASCII, so each of them sits
/// inside a string: writing it as `\uXXXX` keeps the document valid and gives back the exact text once parsed.
pub fn terminal_safe_json(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        // The line breaks of the pretty-printed document are its own (a string never holds a raw one).
        if c != '\n' && forges_output(c) {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Print a report: JSON when requested, otherwise text. Returns the process exit code.
pub fn emit<T: Human>(json: bool, report: &T) -> u8 {
    if json {
        println!(
            "{}",
            terminal_safe_json(&serde_json::to_string_pretty(report).expect("report serialises"))
        );
    } else {
        let text = report.human(&Theme::stdout());
        if !text.is_empty() {
            println!("{}", text.trim_end());
        }
    }
    report.exit_code()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Commit ids are shown short, as in `git log --oneline`.
fn short(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

impl Human for LoginReport {
    fn human(&self, theme: &Theme) -> String {
        let who = match (&self.name, &self.email) {
            (Some(n), Some(e)) => format!("{n} <{e}>"),
            (None, Some(e)) => e.clone(),
            _ => self.user_id.clone(),
        };
        let mut facts = vec![
            ("account", who),
            (
                "access",
                plural(self.projects, "repository", "repositories"),
            ),
        ];
        if let Some(Connection::Connected { expires_at, .. }) = &self.dayaboard {
            facts.push((
                "dayaboard",
                format!("connected until {}", expires_at.date()),
            ));
        }
        let mut s = Screen::new(theme);
        s.title(Tone::Ok, "Logged in").facts(&facts);
        if let Some(Connection::NotConnected { message, .. }) = &self.dayaboard {
            s.note(
                Tone::Warn,
                &format!("Dayaboard is not connected: {message}. Creating tickets needs `speq login` again."),
            );
        }
        if self.projects == 0 {
            s.hint("Ask a platform admin to add you to a repository.");
        } else {
            s.hint("Run `speq list` to see them, then `speq clone <repo>`.");
        }
        s.finish()
    }
}

impl Human for LogoutReport {
    fn human(&self, theme: &Theme) -> String {
        let mut s = Screen::new(theme);
        if self.was_logged_in && self.all_devices {
            s.title(Tone::Ok, "Logged out everywhere").text(
                "Every session of this account was ended and local credentials were removed.",
            );
        } else if self.was_logged_in {
            s.title(Tone::Ok, "Logged out")
                .text("Local credentials were removed from this machine.")
                .hint("The token stays valid until it expires; run `speq logout --all` to end it.");
        } else {
            s.title(Tone::Info, "You were not logged in")
                .text("Local state was cleaned.");
        }
        if let Some(Revocation {
            revoked: false,
            expires_at,
        }) = &self.dayaboard
        {
            s.note(
                Tone::Warn,
                &format!(
                    "The Dayaboard API key could not be revoked; it stops working on {}.",
                    expires_at.date()
                ),
            );
        }
        s.finish()
    }
}

impl Human for DeinitReport {
    fn human(&self, theme: &Theme) -> String {
        let mut s = Screen::new(theme);
        s.title(
            Tone::Ok,
            if self.legacy {
                "Older speq workspace removed"
            } else {
                "Specs repository removed"
            },
        );
        let mut facts = vec![("removed", self.removed.join(", "))];
        if let Some(lost) = &self.lost {
            facts.push((
                "lost",
                match lost.describe() {
                    d if d.is_empty() => "nothing (everything was pushed)".to_string(),
                    d => d,
                },
            ));
        }
        s.facts(&facts)
            .text("Your login and the .gitignore entry were left in place.");
        if self.was_inside {
            s.note(Tone::Warn, "Your shell was inside it: run `cd ..`.");
        }
        s.hint("Run `speq clone <repo>` to start again.");
        s.finish()
    }
}

impl Human for UninstallReport {
    fn human(&self, theme: &Theme) -> String {
        let clean = self.credential_removed && self.manual_steps.is_empty();
        let mut s = Screen::new(theme);
        if clean {
            s.title(Tone::Ok, "Removed speq's local data");
        } else {
            s.title(
                Tone::Warn,
                "Removed speq's local data, but some steps are left",
            );
        }
        let mut facts = vec![
            (
                "credentials",
                if self.credential_removed {
                    "removed".to_string()
                } else {
                    "not removed (see below)".to_string()
                },
            ),
            ("cached data", format!("removed ({})", self.data_dir)),
        ];
        if let Some(binary) = &self.removed_binary {
            facts.push(("binary", format!("removed ({binary})")));
        }
        if !self.removed_skills.is_empty() {
            // The same skill sits in several agents' folders: name each once.
            let names: std::collections::BTreeSet<String> = self
                .removed_skills
                .iter()
                .map(|p| {
                    std::path::Path::new(p)
                        .file_name()
                        .map_or_else(|| p.clone(), |n| n.to_string_lossy().into_owned())
                })
                .collect();
            let names: Vec<String> = names.into_iter().collect();
            facts.push(("skills", format!("removed ({})", names.join(", "))));
        }
        if !self.removed_instructions.is_empty() {
            facts.push((
                "instructions",
                format!(
                    "speq block removed ({})",
                    self.removed_instructions.join(", ")
                ),
            ));
        }
        s.facts(&facts)
            .text("Specs repositories were left untouched; `speq deinit` removes one.");
        s.section(
            "To do by hand",
            self.manual_steps
                .iter()
                .map(|step| Item::new(Tone::Warn, "→", step))
                .collect(),
        );
        s.finish()
    }
}

impl Human for ListReport {
    fn human(&self, theme: &Theme) -> String {
        let mut s = Screen::new(theme);
        if self.projects.is_empty() {
            s.title(Tone::Info, "No repositories are available to you yet")
                .hint("Ask a platform admin to add you.");
            return s.finish();
        }
        let rows: Vec<Vec<String>> = self
            .projects
            .iter()
            .map(|p| {
                vec![
                    p.repository_full_name
                        .clone()
                        .unwrap_or_else(|| "(not bound)".into()),
                    p.display_name.clone(),
                    p.roles.join(", "),
                ]
            })
            .collect();
        s.heading(&format!(
            "{} you can use",
            plural(rows.len(), "repository", "repositories")
        ))
        .table(&["REPOSITORY", "PROJECT", "ROLES"], &rows)
        .hint("Run `speq clone <repo>` in your implementation repository.");
        s.finish()
    }
}

impl Human for InfoReport {
    fn human(&self, theme: &Theme) -> String {
        let account = &self.account;
        let who = match (&account.name, &account.email) {
            (Some(n), Some(e)) => format!("{n} <{e}>"),
            (None, Some(e)) => e.clone(),
            _ => account.user_id.clone(),
        };
        let left = (self.session.expires_at - crate::snapshot::now())
            .whole_days()
            .max(0);
        let mut facts = vec![("account", who), ("user id", account.user_id.clone())];
        match account.platform_role {
            Some(PlatformRole::Superadmin) => facts.push(("platform", "Super Admin".into())),
            Some(PlatformRole::PlatformAdmin) => facts.push(("platform", "Platform Admin".into())),
            Some(PlatformRole::User) | None => {}
        }
        facts.push((
            "session",
            format!(
                "until {} ({})",
                self.session.expires_at.date(),
                plural(usize::try_from(left).unwrap_or(0), "day left", "days left")
            ),
        ));
        facts.push((
            "dayaboard",
            match &self.dayaboard {
                DayaboardState::Connected { expires_at } => {
                    format!("connected until {}", expires_at.date())
                }
                DayaboardState::Expired { expires_at } => {
                    format!("key expired on {}", expires_at.date())
                }
                DayaboardState::NotConnected => "not connected".to_string(),
            },
        ));
        facts.push(("api", self.api_url.clone()));

        let mut s = Screen::new(theme);
        s.title(Tone::Info, "Signed in").facts(&facts);
        if !matches!(self.dayaboard, DayaboardState::Connected { .. }) {
            s.note(
                Tone::Warn,
                "Dayaboard is not connected: creating tickets needs `speq login` again.",
            );
        }
        if self.projects.is_empty() {
            s.hint("You hold no repository yet. Ask a platform admin to add you.");
            return s.finish();
        }
        let rows: Vec<Vec<String>> = self
            .projects
            .iter()
            .map(|p| {
                let here = self.current_project_id.as_deref() == Some(p.project_id.as_str());
                let repo = p
                    .repository_full_name
                    .clone()
                    .unwrap_or_else(|| "(not bound)".into());
                vec![
                    if here { format!("{repo} (here)") } else { repo },
                    p.display_name.clone(),
                    if p.roles.is_empty() {
                        "(no roles)".to_string()
                    } else {
                        p.roles.join(", ")
                    },
                ]
            })
            .collect();
        s.heading("Your roles")
            .table(&["REPOSITORY", "PROJECT", "ROLES"], &rows);
        s.finish()
    }
}

pub fn login_prompt(
    theme: &Theme,
    url: &str,
    browser_failed: bool,
    expires_minutes: u64,
) -> String {
    let mut s = Screen::new(theme);
    s.title(Tone::Info, "Approve this device in your browser")
        .text(&format!("  {url}"));
    if browser_failed {
        s.note(
            Tone::Warn,
            "Could not open a browser automatically; open the URL above.",
        );
    }
    s.text(&format!(
        "Waiting for approval; the request expires in {expires_minutes} minutes..."
    ));
    s.finish()
}

impl Human for CloneReport {
    fn human(&self, theme: &Theme) -> String {
        let mut s = Screen::new(theme);
        s.title(Tone::Ok, &format!("Cloned {}", self.repository));
        let mut facts = vec![
            ("folder", format!("./{}", self.path)),
            (
                "commit",
                self.head_sha
                    .as_deref()
                    .map(short)
                    .unwrap_or("(the remote branch is empty)")
                    .to_string(),
            ),
            ("author", self.author.clone()),
        ];
        if let Some(ignore) = &self.ignored_in {
            facts.push(("ignored in", ignore.clone()));
        }
        s.facts(&facts).text(
            "It is a Git repository: use Git's commands through speq, from here or inside the folder.",
        );
        s.hint("Edit, then `speq status`, `speq commit -am \"docs: ...\"` and `speq push`.");
        s.finish()
    }
}

impl Human for ContextReport {
    fn human(&self, _theme: &Theme) -> String {
        self.paths.join("\n")
    }
}

/// The page itself and nothing else, so a pipe or an agent reads plain HTML; hostile control characters are defanged.
impl Human for UiPageReport {
    fn human(&self, _theme: &Theme) -> String {
        terminal_text(&self.content)
    }
}

/// The design document and nothing else, so it can be redirected to a file; neutralised like a page. Nothing is
/// printed when the repository has no `DESIGN.md` (the command says so on stderr).
impl Human for DesignReport {
    fn human(&self, _theme: &Theme) -> String {
        self.content
            .as_deref()
            .map(terminal_text)
            .unwrap_or_default()
    }
}

/// One block per persona, separated by a blank line. Every text comes from a repository: it is neutralised like a page,
/// and kept on one line (a line break would let a name forge another line of the listing).
impl Human for PersonasReport {
    fn human(&self, _theme: &Theme) -> String {
        if self.personas.is_empty() {
            return "no personas declared".to_string();
        }
        let line = |text: &str| terminal_text(text).replace(['\n', '\t'], " ");
        let blocks: Vec<String> = self
            .personas
            .iter()
            .map(|p| {
                let mut b = format!("{}  {}\n", line(&p.id), line(&p.name));
                b.push_str(&format!(
                    "  start: {}\n",
                    p.start.as_deref().map_or("(none)".to_string(), line)
                ));
                b.push_str(&format!(
                    "  pages: {}\n",
                    if p.pages.is_empty() {
                        "(none)".to_string()
                    } else {
                        p.pages
                            .iter()
                            .map(|x| line(x))
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                ));
                if let Some(note) = &p.note {
                    b.push_str(&format!("  note:  {}\n", line(note)));
                }
                b.trim_end().to_string()
            })
            .collect();
        blocks.join("\n\n")
    }
}

/// Plain sections, like `speq context`: read by people and by agents that did not ask for `--json`.
impl Human for ImpactReport {
    fn human(&self, _theme: &Theme) -> String {
        let lines = |title: &str, lines: &[ImpactLine]| {
            let mut s = format!("{title}\n");
            for l in lines {
                s.push_str(&format!("  {}:{}  {}\n", l.path, l.line, l.text));
            }
            if lines.is_empty() {
                s.push_str("  (none)\n");
            }
            s
        };
        let mut out = format!("Impact of {} in {}\n\n", self.id, self.scope);
        out.push_str(&lines("Written in", &self.sources));
        out.push_str(&lines("Cited by", &self.mentions));
        out.push_str(&format!(
            "Criteria\n  {}\n",
            if self.criteria.is_empty() {
                "(none)".to_string()
            } else {
                self.criteria.join(", ")
            }
        ));
        out.push_str("Tasks\n");
        for t in &self.tasks {
            out.push_str(&format!(
                "  {:<11} {:<10} {}  ({}){}\n",
                t.status.as_deref().unwrap_or("no status"),
                t.ticket_id.as_deref().unwrap_or("no ticket"),
                t.path,
                t.names.join(", "),
                if t.reopen {
                    "  reopen before the change"
                } else {
                    ""
                }
            ));
        }
        if self.tasks.is_empty() {
            out.push_str("  (none)\n");
        }
        if self.code_dirs.is_empty() {
            out.push_str("Code\n  not searched: pass --code <repository>\n");
        } else {
            out.push_str(&lines(
                &format!("Code (in {})", self.code_dirs.join(", ")),
                &self.code,
            ));
            if !self.code_no_epic.is_empty() {
                out.push_str(&lines(
                    "Code naming no epic (another epic has a story with the same number: check each)",
                    &self.code_no_epic,
                ));
            }
            if !self.unclaimed.is_empty() {
                out.push_str(&format!(
                    "  no code line names: {}\n",
                    self.unclaimed.join(", ")
                ));
            }
        }
        out
    }
}

impl Human for CheckReport {
    fn human(&self, theme: &Theme) -> String {
        let mut s = Screen::new(theme);
        if self.status == "update-available" {
            s.title(Tone::Info, "A newer version of speq is available");
        } else {
            s.title(Tone::Ok, "speq is up to date");
        }
        s.facts(&[
            ("installed", self.current.clone()),
            ("latest", self.latest.clone()),
            ("target", self.target.to_string()),
        ]);
        if self.status == "update-available" {
            s.hint("Run `speq upgrade` to install it.");
        }
        s.finish()
    }
}

impl Human for SkillsReport {
    fn human(&self, theme: &Theme) -> String {
        let mut facts = vec![("skills", self.skills.join(", "))];
        facts.extend(self.dirs.iter().map(|d| ("folder", d.clone())));
        facts.extend(
            self.instructions
                .iter()
                .map(|f| ("instructions", f.clone())),
        );
        let mut s = Screen::new(theme);
        s.title(Tone::Ok, "Installed the Speq skills for AI coding agents")
            .facts(&facts)
            .hint(&format!(
                "Start a new session of your coding agent (Claude Code, Codex, Cursor, Copilot, Gemini CLI, OpenCode), then ask it to use {}.",
                self.skills.join(" or ")
            ));
        s.finish()
    }
}

impl Human for UpgradeReport {
    fn human(&self, theme: &Theme) -> String {
        let mut s = Screen::new(theme);
        if self.installed {
            let mut facts = vec![("version", format!("{} → {}", self.from, self.to))];
            if let Some(path) = &self.path {
                facts.push(("path", path.clone()));
            }
            s.title(Tone::Ok, "speq was upgraded").facts(&facts);
            if let Some(warning) = &self.skills_warning {
                s.note(Tone::Warn, warning);
            }
        } else {
            s.title(Tone::Info, "Nothing to install")
                .facts(&[("installed", self.from.clone())]);
        }
        s.finish()
    }
}

/// Colors for stderr only on a terminal, and never when `NO_COLOR` is set.
pub fn stderr_color() -> bool {
    std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

/// The error side of the template: title, what happened, what to do next, and the technical details.
///
/// ```text
/// ✖ Title
///
///   What happened.
///
///   → What to do next
///
///   code <code> · HTTP <status> · request <id> · exit <n>
/// ```
///
/// The hint and the HTTP status and request id are shown only when the error has them.
pub fn render_error(error: &Error, color: bool) -> String {
    let theme = Theme::new(color);
    let mut meta = vec![format!("code {}", error.code())];
    if let Some(status) = error.api_status() {
        meta.push(format!("HTTP {status}"));
    }
    if let Some(id) = error.request_id() {
        meta.push(format!("request {id}"));
    }
    meta.push(format!("exit {}", error.exit_code()));

    let mut s = Screen::new(&theme);
    s.title(Tone::Fail, error.title()).text(&error.detail());
    if let Some(hint) = error.hint() {
        s.hint(&hint);
    }
    s.meta(&meta.join(" · "));
    s.finish()
}

#[cfg(test)]
mod tests {
    use super::terminal_safe_json;
    use serde_json::{Value, json};

    /// Text a hostile page could hold: C1 controls (U+009B is a one-character CSI), DEL, a bidirectional override,
    /// zero-width and soft-hyphen marks, and a Unicode tag character, which lies outside the Basic Multilingual Plane.
    const HOSTILE: &str = "a\u{9b}31mb\u{7f}c\u{202e}d\u{200b}e\u{ad}f\u{e0041}g\u{2028}h\u{feff}i";

    fn safe(value: &Value) -> String {
        terminal_safe_json(&serde_json::to_string_pretty(value).unwrap())
    }

    #[test]
    fn leaves_no_raw_control_or_invisible_character_for_the_terminal() {
        let out = safe(&json!({ "content": HOSTILE }));
        assert!(out.is_ascii(), "{out:?}");
        assert!(!out.chars().any(|c| c.is_control() && c != '\n'), "{out:?}");
    }

    #[test]
    fn keeps_the_text_exact_once_parsed_even_outside_the_basic_plane() {
        let value = json!({ "content": HOSTILE, "nested": [HOSTILE, { "k": HOSTILE }], "n": 7 });
        let parsed: Value = serde_json::from_str(&safe(&value)).expect("still JSON");
        assert_eq!(parsed, value);
        // The tag character is written as a surrogate pair.
        assert!(
            safe(&json!(HOSTILE)).contains("\\ud800\\udc")
                || safe(&json!(HOSTILE)).contains("\\udb40\\udc41")
        );
    }

    #[test]
    fn leaves_text_people_write_as_it_is() {
        let value = json!({ "content": "héllo — ✓ 日本 😀 \u{fe0f}\n\t\"quoted\" back\\slash" });
        let out = safe(&value);
        assert!(out.contains("héllo — ✓ 日本 😀"), "{out}");
        assert_eq!(serde_json::from_str::<Value>(&out).unwrap(), value);
    }

    #[test]
    fn a_design_report_shows_its_text_defanged_and_nothing_when_there_is_none() {
        use super::Human;
        use crate::template::Theme;
        use crate::ui::DesignReport;
        let report = |content: Option<&str>| DesignReport {
            ui_product_id: "ui_1".into(),
            head_sha: "a".repeat(40),
            problem: None,
            content: content.map(str::to_string),
        };
        let theme = Theme::stdout();
        assert_eq!(
            report(Some("# D\u{200b}x\u{1b}[0m\n")).human(&theme),
            "# D<U+200B>x [0m\n"
        );
        assert_eq!(report(Some("")).human(&theme), "");
        assert_eq!(report(None).human(&theme), "");
        let json = serde_json::to_value(report(None)).unwrap();
        assert_eq!(json["content"], Value::Null);
        assert!(json.get("problem").is_none());
    }

    #[test]
    fn does_not_touch_the_structure_of_the_document() {
        let value = json!({ "a": [1, 2, { "b": null }], "c": true });
        let plain = serde_json::to_string_pretty(&value).unwrap();
        assert_eq!(terminal_safe_json(&plain), plain);
    }
}
