//! Human-readable output (default) and `--json`. Machine output is the report struct itself.
use crate::context::ContextReport;
use crate::deinit::DeinitReport;
use crate::error::{Error, exit};
use crate::init::{InitReport, ListReport};
use crate::session::{LoginReport, LogoutReport};
use crate::sync::pull::{PullReport, UpdateReport};
use crate::sync::push::PushReport;
use crate::sync::status::{DiffReport, StatusReport};
use crate::uninstall::UninstallReport;
use crate::upgrade::{CheckReport, UpgradeReport};
use clap::builder::styling::{AnsiColor, Effects, Style};
use serde::Serialize;
use std::io::IsTerminal;

pub trait Human: Serialize {
    fn human(&self) -> String;
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
        let text = report.human();
        if !text.is_empty() {
            println!("{}", text.trim_end());
        }
    }
    report.exit_code()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl Human for LoginReport {
    fn human(&self) -> String {
        let who = match (&self.name, &self.email) {
            (Some(n), Some(e)) => format!("{n} <{e}>"),
            (None, Some(e)) => e.clone(),
            _ => self.user_id.clone(),
        };
        format!(
            "Logged in as {who}. You can access {}.",
            plural(self.projects, "repository", "repositories")
        )
    }
}

impl Human for LogoutReport {
    fn human(&self) -> String {
        if self.was_logged_in && self.all_devices {
            "Logged out everywhere. Every session of this account was ended and local credentials were removed.".into()
        } else if self.was_logged_in {
            "Logged out. Local credentials were removed; the token itself stays valid until it expires (use `speq logout --all` to end it).".into()
        } else {
            "You were not logged in; local state was cleaned.".into()
        }
    }
}

impl Human for DeinitReport {
    fn human(&self) -> String {
        format!(
            "Removed .speq/ and {}/ from {}.\n.gitignore, your login, and the binary were left untouched. Run `speq init` to set up again.",
            self.specs_dir, self.root
        )
    }
}

impl Human for UninstallReport {
    fn human(&self) -> String {
        let mut out = String::from(if self.credential_removed {
            "Removed your local credentials and cached data."
        } else {
            "Removed your cached data."
        });
        out.push_str(
            "\nWorkspaces (specs/ and .speq/) were left untouched; `speq deinit` removes them.",
        );
        if let Some(binary) = &self.removed_binary {
            out.push_str(&format!("\nRemoved {binary}."));
        }
        for step in &self.manual_steps {
            out.push_str(&format!("\n- {step}"));
        }
        out
    }
}

impl Human for ListReport {
    fn human(&self) -> String {
        if self.projects.is_empty() {
            return "No repositories are available to you yet. Ask a platform admin to add you."
                .into();
        }
        let width = self
            .projects
            .iter()
            .map(|p| {
                p.repository_full_name
                    .as_deref()
                    .unwrap_or("(not bound)")
                    .len()
            })
            .max()
            .unwrap_or(0);
        self.projects
            .iter()
            .map(|p| {
                format!(
                    "{:<width$}  {}  [{}]",
                    p.repository_full_name.as_deref().unwrap_or("(not bound)"),
                    p.display_name,
                    p.roles.join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Human for InitReport {
    fn human(&self) -> String {
        let mut out = format!(
            "{} {} (branch {}) into ./{}\nNext: run `speq pull`.",
            if self.reconfigured {
                "Reconfigured"
            } else {
                "Initialized"
            },
            self.repository,
            self.branch,
            self.local_dir
        );
        if self.sync_state_reset {
            out.push_str("\nnote: the source changed, so sync state was cleared; existing files in the folder count as untracked local files.");
        }
        out
    }
}

impl Human for PullReport {
    fn human(&self) -> String {
        let mut out = String::new();
        for (label, paths) in [
            ("added", &self.added),
            ("updated", &self.updated),
            ("deleted", &self.deleted),
            ("adopted", &self.adopted),
        ] {
            for p in paths {
                out.push_str(&format!("{label:<8}{p}\n"));
            }
        }
        for c in &self.conflicts {
            out.push_str(&format!("conflict {} ({})\n", c.path, c.reason));
        }
        out.push_str(&format!(
            "Pulled {}: {} added, {} updated, {} deleted, {} unchanged or kept.\n",
            &self.head_sha[..self.head_sha.len().min(12)],
            self.added.len(),
            self.updated.len(),
            self.deleted.len(),
            self.unchanged
        ));
        if self.conflicts.is_empty() {
            out.push_str("Workspace is clean.");
        } else {
            out.push_str(&format!("{} not applied; resolve with `speq diff <path>`, then edit the file and pull again. Push stays blocked until then.", plural(self.conflicts.len(), "path was", "paths were")));
        }
        out
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
    fn human(&self) -> String {
        match self.result {
            "up-to-date" => format!("{} is already up to date.", self.path),
            _ => format!(
                "Applied the latest version of {} ({}).",
                self.path,
                &self.head_sha[..self.head_sha.len().min(12)]
            ),
        }
    }
}

impl Human for StatusReport {
    fn human(&self) -> String {
        let mut out = format!(
            "lock {}  remote {}  {}\n",
            self.lock_head_sha
                .as_deref()
                .map(|s| &s[..s.len().min(12)])
                .unwrap_or("none"),
            &self.remote_head_sha[..self.remote_head_sha.len().min(12)],
            if self.up_to_date {
                "(up to date)"
            } else {
                "(run `speq pull`)"
            }
        );
        let mut any = false;
        for f in self.files.iter().filter(|f| f.state != "unchanged") {
            any = true;
            let detail = match f.state {
                "conflict" => format!("local {}, remote {}", f.local, f.remote),
                "remote-only" => format!("remote {}", f.remote),
                _ => format!("local {}", f.local),
            };
            out.push_str(&format!("{:<12}{}  ({detail})\n", f.state, f.path));
        }
        if !any {
            out.push_str("All files are unchanged.\n");
        }
        for p in &self.skipped {
            out.push_str(&format!("ignored     {p}\n"));
        }
        for p in &self.unsafe_paths {
            out.push_str(&format!(
                "unsafe      {p} (symbolic link or special file; never followed)\n"
            ));
        }
        if !self.pending.is_empty() {
            out.push_str(&format!(
                "{} pending from the last pull; `speq push` is blocked.\n",
                plural(self.pending.len(), "conflict", "conflicts")
            ));
        }
        out
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
    fn human(&self) -> String {
        if self.identical {
            format!(
                "No differences between {} and the {} copy.",
                self.path, self.against
            )
        } else {
            self.diff.clone()
        }
    }
}

impl Human for PushReport {
    fn human(&self) -> String {
        if self.nothing_to_push {
            return "Nothing to push.".into();
        }
        let sha = self
            .commit_sha
            .as_deref()
            .map(|s| &s[..s.len().min(12)])
            .unwrap_or("");
        format!(
            "Pushed {} in commit {sha}{}.",
            plural(self.changes.len(), "change", "changes"),
            if self.replayed {
                " (already applied earlier)"
            } else {
                ""
            }
        )
    }
}

impl Human for ContextReport {
    fn human(&self) -> String {
        self.paths.join("\n")
    }
}

impl Human for CheckReport {
    fn human(&self) -> String {
        format!(
            "installed {}  latest {}  target {}  status: {}",
            self.current, self.latest, self.target, self.status
        )
    }
}

impl Human for UpgradeReport {
    fn human(&self) -> String {
        if self.installed {
            format!("Upgraded speq {} -> {}.", self.from, self.to)
        } else {
            format!("speq {} is already the latest version.", self.from)
        }
    }
}

/// Colors for stderr only on a terminal, and never when `NO_COLOR` is set.
pub fn stderr_color() -> bool {
    std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

/// The one template for human error output:
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
    let red = AnsiColor::Red.on_default().effects(Effects::BOLD);
    let yellow = AnsiColor::Yellow.on_default().effects(Effects::BOLD);
    let literal = AnsiColor::Green.on_default().effects(Effects::BOLD);
    let dim = AnsiColor::BrightBlack.on_default();
    let paint = |style: &Style, text: &str| {
        if color {
            format!("{style}{text}{style:#}")
        } else {
            text.to_string()
        }
    };
    // `commands` in backticks keep their backticks and get the same green as in `--help`.
    let paint_spans = |text: &str| {
        text.split('`')
            .enumerate()
            .map(|(i, part)| {
                if i % 2 == 1 {
                    format!("`{}`", paint(&literal, part))
                } else {
                    part.to_string()
                }
            })
            .collect::<String>()
    };

    let mut meta = vec![format!("code {}", error.code())];
    if let Some(status) = error.api_status() {
        meta.push(format!("HTTP {status}"));
    }
    if let Some(id) = error.request_id() {
        meta.push(format!("request {id}"));
    }
    meta.push(format!("exit {}", error.exit_code()));

    let mut out = format!("{} {}\n", paint(&red, "✖"), paint(&red, error.title()));
    out.push('\n');
    for line in error.detail().lines() {
        out.push_str(&format!("  {}\n", paint_spans(line)));
    }
    if let Some(hint) = error.hint() {
        out.push_str(&format!(
            "\n  {} {}\n",
            paint(&yellow, "→"),
            paint_spans(&hint)
        ));
    }
    out.push_str(&format!("\n  {}", paint(&dim, &meta.join(" · "))));
    out
}
