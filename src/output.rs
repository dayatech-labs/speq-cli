//! Human-readable output (default) and `--json`. Machine output is the report struct itself.
//! Every text layout is built with the one template in [`crate::template`].
use crate::clone::{CloneReport, ListReport};
use crate::context::ContextReport;
use crate::deinit::DeinitReport;
use crate::error::{Error, exit};
use crate::session::{LoginReport, LogoutReport};
use crate::skills::SkillsReport;
use crate::template::{Item, Screen, Theme, Tone};
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

/// Print a report: JSON when requested, otherwise text. Returns the process exit code.
pub fn emit<T: Human>(json: bool, report: &T) -> u8 {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).expect("report serialises")
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
        let mut s = Screen::new(theme);
        s.title(Tone::Ok, "Logged in").facts(&[
            ("account", who),
            (
                "access",
                plural(self.projects, "repository", "repositories"),
            ),
        ]);
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
            let names: Vec<String> = self
                .removed_skills
                .iter()
                .map(|p| {
                    std::path::Path::new(p)
                        .file_name()
                        .map_or_else(|| p.clone(), |n| n.to_string_lossy().into_owned())
                })
                .collect();
            facts.push(("skills", format!("removed ({})", names.join(", "))));
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

pub fn login_prompt(url: &str, browser_failed: bool, expires_minutes: u64) -> String {
    let theme = Theme::stderr();
    let mut s = Screen::new(&theme);
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
        let mut s = Screen::new(theme);
        s.title(Tone::Ok, "Installed the Speq skills for Claude Code")
            .facts(&[
                ("skills", self.skills.join(", ")),
                ("folder", self.dir.clone()),
            ])
            .hint("Start a new Claude Code session, then use /speq-write to draft a spec document and /speq-review to review one.");
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
