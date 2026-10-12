//! Human-readable output (default) and `--json`. Machine output is the report struct itself.
//! Every text layout is built with the one template in [`crate::template`].
use crate::context::ContextReport;
use crate::deinit::DeinitReport;
use crate::error::{Error, exit};
use crate::init::{InitReport, ListReport};
use crate::session::{LoginReport, LogoutReport};
use crate::sync::pull::{PullReport, UpdateReport};
use crate::sync::push::{PlannedChange, PushReport};
use crate::sync::resolve::ResolveReport;
use crate::sync::status::{DiffReport, StatusReport};
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
            s.hint("Run `speq list` to see them, then `speq init <repo>`.");
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
        s.title(Tone::Ok, "Workspace removed")
            .facts(&[
                ("workspace", self.root.clone()),
                ("removed", format!(".speq/ and {}/", self.specs_dir)),
            ])
            .text(".gitignore, your login, and the binary were left untouched.")
            .hint("Run `speq init <repo>` to set up again.");
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
        s.facts(&facts).text(
            "Workspaces (specs/ and .speq/) were left untouched; `speq deinit` removes them.",
        );
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
        .hint("Run `speq init <repo>` in your implementation repository.");
        s.finish()
    }
}

impl Human for InitReport {
    fn human(&self, theme: &Theme) -> String {
        let mut s = Screen::new(theme);
        s.title(
            Tone::Ok,
            if self.reconfigured {
                "Workspace reconfigured"
            } else {
                "Workspace initialized"
            },
        )
        .facts(&[
            ("repository", self.repository.clone()),
            ("branch", self.branch.clone()),
            ("folder", format!("./{}", self.local_dir)),
        ]);
        if self.sync_state_reset {
            s.note(
                Tone::Warn,
                "The source changed, so sync state was cleared; existing files in the folder count as untracked local files.",
            );
        }
        s.hint("Run `speq pull` to download the specs.");
        s.finish()
    }
}

impl Human for PullReport {
    fn human(&self, theme: &Theme) -> String {
        let changed = self.added.len()
            + self.updated.len()
            + self.deleted.len()
            + self.merged.len()
            + self.resolved.len();
        let mut s = Screen::new(theme);
        if !self.conflicts.is_empty() {
            s.title(
                Tone::Warn,
                &format!(
                    "Pulled with {}",
                    plural(self.conflicts.len(), "conflict", "conflicts")
                ),
            );
        } else if changed + self.adopted.len() == 0 {
            s.title(Tone::Ok, "Already up to date");
        } else {
            s.title(Tone::Ok, "Pulled the latest specs");
        }
        let mut summary: Vec<String> = [
            (self.added.len(), "added"),
            (self.updated.len(), "updated"),
            (self.deleted.len(), "deleted"),
            (self.adopted.len(), "adopted"),
            (self.merged.len(), "merged"),
            (self.resolved.len(), "resolved"),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, label)| format!("{n} {label}"))
        .collect();
        summary.push(format!("{} unchanged or kept", self.unchanged));
        s.facts(&[
            ("commit", short(&self.head_sha).to_string()),
            ("summary", summary.join(" · ")),
        ]);
        let items = |tone, glyph, paths: &[String]| {
            paths
                .iter()
                .map(|p| Item::new(tone, glyph, p))
                .collect::<Vec<_>>()
        };
        s.section("Added", items(Tone::Ok, "+", &self.added))
            .section("Updated", items(Tone::Info, "~", &self.updated))
            .section("Deleted", items(Tone::Warn, "-", &self.deleted))
            .section("Adopted", items(Tone::Info, "=", &self.adopted))
            .section("Merged", items(Tone::Info, "&", &self.merged))
            .section("Resolved", items(Tone::Ok, "*", &self.resolved))
            .section(
                "Conflicts",
                self.conflicts
                    .iter()
                    .map(|c| {
                        let detail = if c.markers {
                            format!("{} · conflict markers in the file", c.reason)
                        } else {
                            c.reason.clone()
                        };
                        Item::new(Tone::Warn, "!", &c.path).detail(&detail)
                    })
                    .collect(),
            );
        if !self.merged.is_empty() {
            s.hint(&format!(
                "{} combined your edits with the remote's. Review with `speq diff <path>` before `speq push`.",
                plural(self.merged.len(), "file", "files")
            ));
        }
        let marked = self.conflicts.iter().filter(|c| c.markers).count();
        if marked > 0 {
            s.hint(&format!(
                "Open {} in your editor, resolve each conflict block (VS Code: Accept Current, Incoming or Both), save, then run `speq resolve --all`; push stays blocked until then.",
                plural(marked, "marked file", "marked files")
            ));
        }
        let unmarked = self.conflicts.len() - marked;
        if unmarked > 0 {
            s.hint(&format!(
                "{} not applied. Run `speq diff <path>`, then `speq resolve <path>` to keep your file or add `--theirs` to take the remote's; push stays blocked until then.",
                plural(unmarked, "path was", "paths were")
            ));
        }
        s.finish()
    }
    fn exit_code(&self) -> u8 {
        if self.conflicts.is_empty() {
            exit::OK
        } else {
            exit::CONFLICT
        }
    }
}

impl Human for UpdateReport {
    fn human(&self, theme: &Theme) -> String {
        let mut s = Screen::new(theme);
        s.title(
            Tone::Ok,
            if self.result == "up-to-date" {
                "Already up to date"
            } else {
                "Applied the latest version"
            },
        )
        .facts(&[
            ("file", self.path.clone()),
            ("commit", short(&self.head_sha).to_string()),
        ]);
        s.finish()
    }
}

impl Human for ResolveReport {
    fn human(&self, theme: &Theme) -> String {
        let mut s = Screen::new(theme);
        let open = self.pending.len();
        if self.resolved.is_empty() && open == 0 {
            s.title(Tone::Ok, "No conflicts to resolve");
            return s.finish();
        }
        if open == 0 {
            s.title(Tone::Ok, "All conflicts resolved");
        } else {
            s.title(
                Tone::Warn,
                &format!(
                    "Resolved {}, {} still open",
                    self.resolved.len(),
                    plural(open, "conflict", "conflicts")
                ),
            );
        }
        let still: Vec<&String> = self
            .pending
            .iter()
            .filter(|p| !self.still_marked.contains(p) && !self.undecided.contains(p))
            .collect();
        s.section(
            "Resolved",
            self.resolved
                .iter()
                .map(|p| Item::new(Tone::Ok, "*", p))
                .collect(),
        )
        .section(
            "Still has conflict markers",
            self.still_marked
                .iter()
                .map(|p| Item::new(Tone::Warn, "!", p))
                .collect(),
        )
        .section(
            "Needs a decision",
            self.undecided
                .iter()
                .map(|p| Item::new(Tone::Warn, "?", p))
                .collect(),
        )
        .section(
            "Not resolved",
            still
                .into_iter()
                .map(|p| Item::new(Tone::Warn, "!", p))
                .collect(),
        );
        if !self.still_marked.is_empty() {
            s.hint(
                "Remove the conflict markers and save, then run `speq resolve <path>`; or take a side with `--ours` or `--theirs`.",
            );
        } else if !self.undecided.is_empty() {
            s.hint(
                "`--all` leaves conflicts without markers (a delete on one side) to you. See `speq diff <path>`, then `speq resolve <path>` keeps your side or add `--theirs` for the remote's.",
            );
        } else if open > 0 {
            s.hint("Name the rest, or run `speq resolve --all`.");
        } else if self.lock_advanced {
            s.hint("Review with `speq status`, then run `speq push`.");
        } else {
            s.hint("Run `speq pull` to finish before pushing.");
        }
        s.finish()
    }
    fn exit_code(&self) -> u8 {
        if self.pending.is_empty() {
            exit::OK
        } else {
            exit::CONFLICT
        }
    }
}

impl Human for StatusReport {
    fn human(&self, theme: &Theme) -> String {
        let changed: Vec<_> = self
            .files
            .iter()
            .filter(|f| f.state != "unchanged")
            .collect();
        let has_local = changed
            .iter()
            .any(|f| matches!(f.state, "local-only" | "deleted"));
        let conflicts = self.has_conflicts();

        let mut s = Screen::new(theme);
        if conflicts {
            s.title(Tone::Warn, "Workspace has conflicts");
        } else if changed.is_empty() && self.up_to_date {
            s.title(Tone::Ok, "Workspace is up to date");
        } else if changed.is_empty() {
            s.title(Tone::Info, "The remote has newer specs");
        } else {
            s.title(Tone::Info, "Workspace has changes");
        }
        s.facts(&[
            (
                "last sync",
                self.lock_head_sha
                    .as_deref()
                    .map_or("none", short)
                    .to_string(),
            ),
            ("remote", short(&self.remote_head_sha).to_string()),
        ]);
        s.section(
            "Changes",
            changed
                .iter()
                .map(|f| {
                    let (tone, glyph, detail) = match f.state {
                        "conflict" => (
                            Tone::Warn,
                            "!",
                            format!("conflict · local {}, remote {}", f.local, f.remote),
                        ),
                        "remote-only" => (
                            Tone::Info,
                            "↓",
                            format!("remote-only · remote {}", f.remote),
                        ),
                        "deleted" => (Tone::Info, "-", "deleted · removed locally".to_string()),
                        state => (Tone::Info, "↑", format!("{state} · local {}", f.local)),
                    };
                    Item::new(tone, glyph, &f.path).detail(detail)
                })
                .collect(),
        )
        .section(
            "Ignored",
            self.skipped
                .iter()
                .map(|p| Item::new(Tone::Info, "·", p))
                .collect(),
        )
        .section(
            "Unsafe",
            self.unsafe_paths
                .iter()
                .map(|p| {
                    Item::new(Tone::Warn, "!", p)
                        .detail("symbolic link or special file; never followed")
                })
                .collect(),
        );
        if !self.pending.is_empty() {
            s.note(
                Tone::Warn,
                &format!(
                    "{} pending from the last pull; `speq push` is blocked.",
                    plural(self.pending.len(), "conflict", "conflicts")
                ),
            );
        }
        if conflicts {
            s.hint(
                "Run `speq diff <path>` to inspect, edit the file, then `speq resolve <path>` (or `--ours` / `--theirs`).",
            );
        } else if !self.up_to_date {
            s.hint("Run `speq pull` to bring in the remote changes.");
        } else if has_local {
            s.hint("Run `speq push` to send your local changes.");
        }
        s.finish()
    }
    fn exit_code(&self) -> u8 {
        if self.has_conflicts() {
            exit::CONFLICT
        } else {
            exit::OK
        }
    }
}

impl Human for DiffReport {
    fn human(&self, theme: &Theme) -> String {
        if !self.identical {
            return theme.diff(&self.diff);
        }
        let mut s = Screen::new(theme);
        s.title(Tone::Ok, "No differences").facts(&[
            ("file", self.path.clone()),
            (
                "compared with",
                if self.against == "base" {
                    "the last synced copy".to_string()
                } else {
                    "the latest remote copy".to_string()
                },
            ),
        ]);
        s.finish()
    }
}

/// Shown on stderr while `speq login` waits for the browser approval.
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

/// What `speq push` is about to send, shown on stderr before anything leaves the machine.
pub fn push_preview(changes: &[PlannedChange]) -> String {
    let theme = Theme::stderr();
    let mut s = Screen::new(&theme);
    s.title(
        Tone::Info,
        &format!("Pushing {}", plural(changes.len(), "change", "changes")),
    )
    .section(
        "Changes",
        changes
            .iter()
            .map(|c| {
                let (tone, glyph) = match c.action {
                    "create" => (Tone::Ok, "+"),
                    "delete" => (Tone::Warn, "-"),
                    _ => (Tone::Info, "~"),
                };
                Item::new(tone, glyph, &c.path).detail(c.action)
            })
            .collect(),
    );
    s.finish()
}

impl Human for PushReport {
    fn human(&self, theme: &Theme) -> String {
        let mut s = Screen::new(theme);
        if self.nothing_to_push {
            s.title(Tone::Info, "Nothing to push")
                .text("There are no local changes since the last sync.");
            return s.finish();
        }
        s.title(
            Tone::Ok,
            &format!("Pushed {}", plural(self.changes.len(), "change", "changes")),
        )
        .facts(&[(
            "commit",
            self.commit_sha
                .as_deref()
                .map_or(String::new(), |sha| short(sha).to_string()),
        )]);
        if self.replayed {
            s.note(Tone::Info, "These changes were already applied earlier.");
        }
        s.finish()
    }
}

/// Paths only, one per line and nothing else: coding agents and scripts read this output as a file list.
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

impl Human for UpgradeReport {
    fn human(&self, theme: &Theme) -> String {
        let mut s = Screen::new(theme);
        if self.installed {
            let mut facts = vec![("version", format!("{} → {}", self.from, self.to))];
            if let Some(path) = &self.path {
                facts.push(("path", path.clone()));
            }
            s.title(Tone::Ok, "speq was upgraded").facts(&facts);
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
