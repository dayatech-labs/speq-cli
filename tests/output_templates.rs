//! The human success template: one layout for every command. Each test pins the exact plain text of one report,
//! so a change to the layout is a visible, deliberate edit here.
use speq::api::{Capabilities, ProjectSummary, RuleSet};
use speq::clone::{CloneReport, ListReport};
use speq::context::ContextReport;
use speq::dayaboard::{Connection, Revocation};
use speq::deinit::{AtRisk, DeinitReport};
use speq::output::{Human, login_prompt};
use speq::session::{LoginReport, LogoutReport};
use speq::skills::SkillsReport;
use speq::template::Theme;
use speq::uninstall::UninstallReport;
use speq::upgrade::{CheckReport, UpgradeReport};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn plain<T: Human>(report: &T) -> String {
    report.human(&Theme::new(false))
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn project(repo: Option<&str>, name: &str, roles: &[&str]) -> ProjectSummary {
    let rules = RuleSet {
        allow: vec![],
        deny: vec![],
    };
    ProjectSummary {
        project_id: "p1".into(),
        display_name: name.into(),
        repository_full_name: repo.map(Into::into),
        branch: "main".into(),
        dayaboard_project_key: None,
        roles: strings(roles),
        capabilities: Capabilities {
            create: rules.clone(),
            update: rules,
            can_delete: false,
            can_restore: false,
            can_manage: false,
        },
    }
}

#[test]
fn login_shows_the_account_and_the_next_command() {
    let report = LoginReport {
        user_id: "u1".into(),
        email: Some("ana@example.com".into()),
        name: Some("Ana".into()),
        projects: 2,
        dayaboard: None,
    };
    assert_eq!(
        plain(&report),
        "✔ Logged in\n\n  account  Ana <ana@example.com>\n  access   2 repositories\n\n  → Run `speq list` to see them, then `speq clone <repo>`."
    );
}

#[test]
fn login_without_repositories_points_at_the_admin_and_falls_back_to_the_user_id() {
    let report = LoginReport {
        user_id: "u1".into(),
        email: None,
        name: None,
        projects: 0,
        dayaboard: None,
    };
    assert_eq!(
        plain(&report),
        "✔ Logged in\n\n  account  u1\n  access   0 repositories\n\n  → Ask a platform admin to add you to a repository."
    );
}

#[test]
fn logout_has_three_outcomes() {
    let out = |was_logged_in, all_devices| {
        plain(&LogoutReport {
            was_logged_in,
            all_devices,
            dayaboard: None,
        })
    };
    assert_eq!(
        out(true, true),
        "✔ Logged out everywhere\n\n  Every session of this account was ended and local credentials were removed."
    );
    assert_eq!(
        out(true, false),
        "✔ Logged out\n\n  Local credentials were removed from this machine.\n\n  → The token stays valid until it expires; run `speq logout --all` to end it."
    );
    assert_eq!(
        out(false, false),
        "ℹ You were not logged in\n\n  Local state was cleaned."
    );
}

#[test]
fn list_is_a_table_with_a_next_step() {
    let report = ListReport {
        user_id: "u1".into(),
        expires_at: String::new(),
        web_url: None,
        projects: vec![
            project(Some("acme/payment-specs"), "Payment", &["editor", "viewer"]),
            project(None, "Orphan", &["viewer"]),
        ],
    };
    assert_eq!(
        plain(&report),
        "2 repositories you can use\n\n  REPOSITORY          PROJECT  ROLES\n  acme/payment-specs  Payment  editor, viewer\n  (not bound)         Orphan   viewer\n\n  → Run `speq clone <repo>` in your implementation repository."
    );
}

#[test]
fn an_empty_list_says_so_and_names_who_can_help() {
    let report = ListReport {
        user_id: "u1".into(),
        expires_at: String::new(),
        web_url: None,
        projects: vec![],
    };
    assert_eq!(
        plain(&report),
        "ℹ No repositories are available to you yet\n\n  → Ask a platform admin to add you."
    );
}

#[test]
fn clone_reports_the_folder_commit_author_and_the_git_way_forward() {
    let mut report = CloneReport {
        project_id: "p1".into(),
        repository: "acme/payment-specs".into(),
        path: "specs".into(),
        head_sha: Some(SHA.into()),
        author: "Ana <ana@example.com>".into(),
        ignored_in: Some("/work/app/.gitignore".into()),
    };
    assert_eq!(
        plain(&report),
        "✔ Cloned acme/payment-specs\n\n  folder      ./specs\n  commit      0123456789ab\n  author      Ana <ana@example.com>\n  ignored in  /work/app/.gitignore\n\n  It is a Git repository: use Git's commands through speq, from here or inside the folder.\n\n  → Edit, then `speq status`, `speq commit -am \"docs: ...\"` and `speq push`."
    );
    report.head_sha = None;
    report.ignored_in = None;
    let out = plain(&report);
    assert!(
        out.contains("  commit  (the remote branch is empty)\n"),
        "{out}"
    );
    assert!(!out.contains("ignored in"), "{out}");
}

#[test]
fn uninstall_is_clean_only_when_nothing_is_left_to_do() {
    let mut report = UninstallReport {
        credential_removed: true,
        data_dir: "/home/ana/.local/share/speq".into(),
        removed_binary: Some("/home/ana/.local/bin/speq".into()),
        removed_skills: strings(&[
            "/home/ana/.agents/skills/speq-write",
            "/home/ana/.claude/skills/speq-review",
            "/home/ana/.claude/skills/speq-write",
        ]),
        manual_steps: vec![],
    };
    assert_eq!(
        plain(&report),
        "✔ Removed speq's local data\n\n  credentials  removed\n  cached data  removed (/home/ana/.local/share/speq)\n  binary       removed (/home/ana/.local/bin/speq)\n  skills       removed (speq-review, speq-write)\n\n  Specs repositories were left untouched; `speq deinit` removes one."
    );

    report.removed_binary = None;
    report.removed_skills.clear();
    report.manual_steps = strings(&["Run `brew uninstall speq`"]);
    let out = plain(&report);
    assert!(
        out.starts_with("! Removed speq's local data, but some steps are left\n"),
        "{out}"
    );
    assert!(!out.contains("binary"), "{out}");
    assert!(!out.contains("skills"), "{out}");
    assert!(
        out.ends_with("  To do by hand (1)\n    → Run `brew uninstall speq`"),
        "{out}"
    );

    report.manual_steps.clear();
    report.credential_removed = false;
    let out = plain(&report);
    assert!(
        out.contains("credentials  not removed (see below)"),
        "{out}"
    );
    assert!(out.starts_with("! "), "{out}");
}

#[test]
fn context_stays_a_bare_list_of_paths_for_agents_and_scripts() {
    let report = ContextReport {
        project_id: Some("p1".into()),
        head_sha: SHA.into(),
        paths: strings(&["a/prd.md", "a/spec.md"]),
    };
    assert_eq!(report.human(&Theme::new(true)), "a/prd.md\na/spec.md");
}

#[test]
fn upgrade_check_suggests_the_upgrade_only_when_one_exists() {
    let report = |status| CheckReport {
        current: "0.1.0".into(),
        latest: "0.2.0".into(),
        status,
        target: "aarch64-apple-darwin",
    };
    assert_eq!(
        plain(&report("update-available")),
        "ℹ A newer version of speq is available\n\n  installed  0.1.0\n  latest     0.2.0\n  target     aarch64-apple-darwin\n\n  → Run `speq upgrade` to install it."
    );
    assert_eq!(
        plain(&report("up-to-date")),
        "✔ speq is up to date\n\n  installed  0.1.0\n  latest     0.2.0\n  target     aarch64-apple-darwin"
    );
}

#[test]
fn upgrade_shows_the_version_change_or_that_nothing_was_installed() {
    let done = UpgradeReport {
        from: "0.1.0".into(),
        to: "0.2.0".into(),
        installed: true,
        path: Some("/home/ana/.local/bin/speq".into()),
        skills_warning: None,
    };
    assert_eq!(
        plain(&done),
        "✔ speq was upgraded\n\n  version  0.1.0 → 0.2.0\n  path     /home/ana/.local/bin/speq"
    );
    let same = UpgradeReport {
        from: "0.1.0".into(),
        to: "0.1.0".into(),
        installed: false,
        path: None,
        skills_warning: None,
    };
    assert_eq!(plain(&same), "ℹ Nothing to install\n\n  installed  0.1.0");
}

#[test]
fn an_upgrade_that_could_not_refresh_the_skills_says_how_to_do_it() {
    let report = UpgradeReport {
        from: "0.1.0".into(),
        to: "0.2.0".into(),
        installed: true,
        path: None,
        skills_warning: Some("run `speq skills install`".into()),
    };
    assert_eq!(
        plain(&report),
        "✔ speq was upgraded\n\n  version  0.1.0 → 0.2.0\n\n  ! run `speq skills install`"
    );
}

#[test]
fn skills_install_names_the_skills_every_folder_and_how_to_use_them() {
    let report = SkillsReport {
        dirs: strings(&["/home/ana/.agents/skills", "/home/ana/.claude/skills"]),
        skills: strings(&["speq-cli", "speq-write"]),
    };
    assert_eq!(
        plain(&report),
        "✔ Installed the Speq skills for AI coding agents\n\n  skills  speq-cli, speq-write\n  folder  /home/ana/.agents/skills\n  folder  /home/ana/.claude/skills\n\n  → Start a new session of your coding agent (Claude Code, Codex, Cursor, Copilot, Gemini CLI, OpenCode), then ask it to use speq-cli or speq-write."
    );
}

#[test]
fn no_report_leaks_escape_codes_into_a_plain_theme() {
    let login = LoginReport {
        user_id: "u1".into(),
        email: None,
        name: None,
        projects: 1,
        dayaboard: None,
    };
    assert!(!plain(&login).contains('\x1b'));
    assert!(login.human(&Theme::new(true)).contains('\x1b'));
}

#[test]
fn the_login_prompt_shows_the_url_and_only_mentions_the_browser_when_it_failed() {
    assert_eq!(
        login_prompt("https://speq.example/approve?c=1", false, 10),
        "ℹ Approve this device in your browser\n\n    https://speq.example/approve?c=1\n\n  Waiting for approval; the request expires in 10 minutes..."
    );
    let failed = login_prompt("https://speq.example/approve?c=1", true, 10);
    assert!(
        failed.contains("\n  ! Could not open a browser automatically; open the URL above.\n"),
        "{failed}"
    );
}

#[test]
fn deinit_says_what_was_removed_what_was_lost_and_how_to_start_again() {
    let report = DeinitReport {
        removed: strings(&["/work/app/specs"]),
        lost: Some(AtRisk {
            uncommitted: 1,
            unpushed: 2,
            stashes: 0,
        }),
        was_inside: false,
        legacy: false,
    };
    assert_eq!(
        plain(&report),
        "✔ Specs repository removed\n\n  removed  /work/app/specs\n  lost     1 uncommitted change, 2 unpushed commits\n\n  Your login and the .gitignore entry were left in place.\n\n  → Run `speq clone <repo>` to start again."
    );
    let old = DeinitReport {
        removed: strings(&["/work/app/specs", "/work/app/.speq"]),
        lost: None,
        was_inside: true,
        legacy: true,
    };
    let out = plain(&old);
    assert!(
        out.starts_with(
            "✔ Older speq workspace removed\n\n  removed  /work/app/specs, /work/app/.speq\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("\n  ! Your shell was inside it: run `cd ..`.\n"),
        "{out}"
    );
    assert!(!out.contains("lost"), "{out}");
}

fn day(text: &str) -> time::OffsetDateTime {
    time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339).unwrap()
}

#[test]
fn login_shows_until_when_dayaboard_is_connected() {
    let report = LoginReport {
        user_id: "u1".into(),
        email: Some("ana@example.com".into()),
        name: None,
        projects: 1,
        dayaboard: Some(Connection::Connected {
            url: "https://dayaboard.example".into(),
            expires_at: day("2026-11-19T14:58:51Z"),
        }),
    };
    assert_eq!(
        plain(&report),
        "✔ Logged in\n\n  account    ana@example.com\n  access     1 repository\n  dayaboard  connected until 2026-11-19\n\n  → Run `speq list` to see them, then `speq clone <repo>`."
    );
}

#[test]
fn login_warns_when_dayaboard_is_not_connected_and_says_how_to_fix_it() {
    let report = LoginReport {
        user_id: "u1".into(),
        email: Some("ana@example.com".into()),
        name: None,
        projects: 1,
        dayaboard: Some(Connection::NotConnected {
            code: "dayaboard_refused".into(),
            message: "User is not registered in Dayaboard".into(),
        }),
    };
    assert_eq!(
        plain(&report),
        "✔ Logged in\n\n  account  ana@example.com\n  access   1 repository\n\n  ! Dayaboard is not connected: User is not registered in Dayaboard. Creating tickets needs `speq login` again.\n\n  → Run `speq list` to see them, then `speq clone <repo>`."
    );
}

#[test]
fn logout_mentions_dayaboard_only_when_its_key_could_not_be_revoked() {
    let out = |revoked| {
        plain(&LogoutReport {
            was_logged_in: true,
            all_devices: true,
            dayaboard: Some(Revocation {
                revoked,
                expires_at: day("2026-11-19T14:58:51Z"),
            }),
        })
    };
    assert_eq!(
        out(true),
        "✔ Logged out everywhere\n\n  Every session of this account was ended and local credentials were removed."
    );
    assert_eq!(
        out(false),
        "✔ Logged out everywhere\n\n  Every session of this account was ended and local credentials were removed.\n\n  ! The Dayaboard API key could not be revoked; it stops working on 2026-11-19."
    );
}
