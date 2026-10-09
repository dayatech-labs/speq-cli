//! The human success template: one layout for every command. Each test pins the exact plain text of one report,
//! so a change to the layout is a visible, deliberate edit here.
use speq::api::{Capabilities, ProjectSummary, RuleSet};
use speq::context::ContextReport;
use speq::deinit::DeinitReport;
use speq::init::{InitReport, ListReport};
use speq::output::{Human, login_prompt, push_preview};
use speq::session::{LoginReport, LogoutReport};
use speq::sync::plan::State;
use speq::sync::pull::{Conflict, PullReport, UpdateReport};
use speq::sync::push::{PlannedChange, PushReport};
use speq::sync::status::{DiffReport, PathStatus, StatusReport};
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

fn pull_report() -> PullReport {
    PullReport {
        head_sha: SHA.into(),
        added: vec![],
        updated: vec![],
        deleted: vec![],
        adopted: vec![],
        conflicts: vec![],
        unchanged: 0,
        lock_advanced: true,
    }
}

fn path_status(path: &str, state: State, local: &'static str, remote: &'static str) -> PathStatus {
    PathStatus {
        path: path.into(),
        state: state.as_str(),
        local,
        remote,
    }
}

fn status_report(files: Vec<PathStatus>, up_to_date: bool) -> StatusReport {
    StatusReport {
        project_id: "p1".into(),
        lock_head_sha: Some(SHA.into()),
        remote_head_sha: SHA.into(),
        up_to_date,
        pending: vec![],
        files,
        skipped: vec![],
        unsafe_paths: vec![],
    }
}

#[test]
fn login_shows_the_account_and_the_next_command() {
    let report = LoginReport {
        user_id: "u1".into(),
        email: Some("ana@example.com".into()),
        name: Some("Ana".into()),
        projects: 2,
    };
    assert_eq!(
        plain(&report),
        "✔ Logged in\n\n  account  Ana <ana@example.com>\n  access   2 repositories\n\n  → Run `speq list` to see them, then `speq init <repo>`."
    );
}

#[test]
fn login_without_repositories_points_at_the_admin_and_falls_back_to_the_user_id() {
    let report = LoginReport {
        user_id: "u1".into(),
        email: None,
        name: None,
        projects: 0,
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
        projects: vec![
            project(Some("acme/payment-specs"), "Payment", &["editor", "viewer"]),
            project(None, "Orphan", &["viewer"]),
        ],
    };
    assert_eq!(
        plain(&report),
        "2 repositories you can use\n\n  REPOSITORY          PROJECT  ROLES\n  acme/payment-specs  Payment  editor, viewer\n  (not bound)         Orphan   viewer\n\n  → Run `speq init <repo>` in your implementation repository."
    );
}

#[test]
fn an_empty_list_says_so_and_names_who_can_help() {
    let report = ListReport {
        user_id: "u1".into(),
        expires_at: String::new(),
        projects: vec![],
    };
    assert_eq!(
        plain(&report),
        "ℹ No repositories are available to you yet\n\n  → Ask a platform admin to add you."
    );
}

#[test]
fn init_reports_the_workspace_and_warns_when_sync_state_was_reset() {
    let mut report = InitReport {
        project_id: "p1".into(),
        repository: "acme/payment-specs".into(),
        branch: "main".into(),
        local_dir: "specs".into(),
        reconfigured: false,
        sync_state_reset: false,
    };
    assert_eq!(
        plain(&report),
        "✔ Workspace initialized\n\n  repository  acme/payment-specs\n  branch      main\n  folder      ./specs\n\n  → Run `speq pull` to download the specs."
    );
    report.reconfigured = true;
    report.sync_state_reset = true;
    let out = plain(&report);
    assert!(out.starts_with("✔ Workspace reconfigured\n"), "{out}");
    assert!(
        out.contains("\n  ! The source changed, so sync state was cleared;"),
        "{out}"
    );
}

#[test]
fn deinit_lists_what_was_removed_and_what_stayed() {
    let report = DeinitReport {
        root: "/work/app".into(),
        specs_dir: "specs".into(),
    };
    assert_eq!(
        plain(&report),
        "✔ Workspace removed\n\n  workspace  /work/app\n  removed    .speq/ and specs/\n\n  .gitignore, your login, and the binary were left untouched.\n\n  → Run `speq init <repo>` to set up again."
    );
}

#[test]
fn uninstall_is_clean_only_when_nothing_is_left_to_do() {
    let mut report = UninstallReport {
        credential_removed: true,
        data_dir: "/home/ana/.local/share/speq".into(),
        removed_binary: Some("/home/ana/.local/bin/speq".into()),
        manual_steps: vec![],
    };
    assert_eq!(
        plain(&report),
        "✔ Removed speq's local data\n\n  credentials  removed\n  cached data  removed (/home/ana/.local/share/speq)\n  binary       removed (/home/ana/.local/bin/speq)\n\n  Workspaces (specs/ and .speq/) were left untouched; `speq deinit` removes them."
    );

    report.removed_binary = None;
    report.manual_steps = strings(&["Run `brew uninstall speq`"]);
    let out = plain(&report);
    assert!(
        out.starts_with("! Removed speq's local data, but some steps are left\n"),
        "{out}"
    );
    assert!(!out.contains("binary"), "{out}");
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
fn a_pull_without_changes_is_already_up_to_date() {
    let mut report = pull_report();
    report.unchanged = 4;
    assert_eq!(
        plain(&report),
        "✔ Already up to date\n\n  commit   0123456789ab\n  summary  4 unchanged or kept"
    );
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn a_pull_groups_paths_by_what_happened_and_hides_empty_groups() {
    let mut report = pull_report();
    report.added = strings(&["a.md", "b.md"]);
    report.updated = strings(&["c.md"]);
    report.adopted = strings(&["e.md"]);
    report.unchanged = 7;
    assert_eq!(
        plain(&report),
        "✔ Pulled the latest specs\n\n  commit   0123456789ab\n  summary  2 added · 1 updated · 1 adopted · 7 unchanged or kept\n\n  Added (2)\n    + a.md\n    + b.md\n\n  Updated (1)\n    ~ c.md\n\n  Adopted (1)\n    = e.md"
    );
}

#[test]
fn a_pull_with_conflicts_warns_exits_5_and_says_how_to_resolve() {
    let mut report = pull_report();
    report.deleted = strings(&["gone.md"]);
    report.conflicts = vec![Conflict {
        path: "prd.md".into(),
        reason: "changed on both sides".into(),
    }];
    let out = plain(&report);
    assert!(out.starts_with("! Pulled with 1 conflict\n"), "{out}");
    assert!(out.contains("  Deleted (1)\n    - gone.md\n"), "{out}");
    assert!(
        out.contains("  Conflicts (1)\n    ! prd.md  changed on both sides\n"),
        "{out}"
    );
    assert!(
        out.ends_with("  → 1 path was not applied. Run `speq diff <path>`, edit the file, then pull again; push stays blocked until then."),
        "{out}"
    );
    assert_eq!(report.exit_code(), 5);
}

#[test]
fn update_tells_whether_anything_changed() {
    let report = |result| UpdateReport {
        path: "epics/a/prd.md".into(),
        result,
        head_sha: SHA.into(),
    };
    assert_eq!(
        plain(&report("up-to-date")),
        "✔ Already up to date\n\n  file    epics/a/prd.md\n  commit  0123456789ab"
    );
    assert!(plain(&report("updated")).starts_with("✔ Applied the latest version\n"));
}

#[test]
fn a_clean_status_is_one_line_of_good_news_and_two_commits() {
    let report = status_report(
        vec![path_status("a.md", State::Unchanged, "none", "none")],
        true,
    );
    assert_eq!(
        plain(&report),
        "✔ Workspace is up to date\n\n  last sync  0123456789ab\n  remote     0123456789ab"
    );
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn status_marks_each_change_by_direction_and_suggests_the_next_command() {
    let report = status_report(
        vec![
            path_status("a.md", State::Unchanged, "none", "none"),
            path_status("b.md", State::LocalOnly, "modified", "none"),
            path_status("c.md", State::Deleted, "deleted", "none"),
        ],
        true,
    );
    assert_eq!(
        plain(&report),
        "ℹ Workspace has changes\n\n  last sync  0123456789ab\n  remote     0123456789ab\n\n  Changes (2)\n    ↑ b.md  local-only · local modified\n    - c.md  deleted · removed locally\n\n  → Run `speq push` to send your local changes."
    );

    let behind = status_report(
        vec![path_status("d.md", State::RemoteOnly, "none", "added")],
        false,
    );
    let out = plain(&behind);
    assert!(
        out.contains("    ↓ d.md  remote-only · remote added\n"),
        "{out}"
    );
    assert!(
        out.ends_with("→ Run `speq pull` to bring in the remote changes."),
        "{out}"
    );
}

#[test]
fn status_with_a_conflict_warns_exits_5_and_shows_both_sides() {
    let mut report = status_report(
        vec![path_status("a.md", State::Conflict, "modified", "modified")],
        false,
    );
    report.lock_head_sha = None;
    report.pending = strings(&["a.md"]);
    report.skipped = strings(&["notes.tmp"]);
    report.unsafe_paths = strings(&["link.md"]);
    let out = plain(&report);
    assert!(out.starts_with("! Workspace has conflicts\n"), "{out}");
    assert!(out.contains("  last sync  none\n"), "{out}");
    assert!(
        out.contains("    ! a.md  conflict · local modified, remote modified\n"),
        "{out}"
    );
    assert!(out.contains("  Ignored (1)\n    · notes.tmp\n"), "{out}");
    assert!(
        out.contains("    ! link.md  symbolic link or special file; never followed\n"),
        "{out}"
    );
    assert!(
        out.contains("  ! 1 conflict pending from the last pull; `speq push` is blocked.\n"),
        "{out}"
    );
    assert!(
        out.ends_with("  → Run `speq diff <path>` to inspect, edit the file, then `speq pull`."),
        "{out}"
    );
    assert_eq!(report.exit_code(), 5);
}

#[test]
fn a_newer_remote_without_file_changes_is_reported_as_such() {
    let out = plain(&status_report(vec![], false));
    assert!(out.starts_with("ℹ The remote has newer specs\n"), "{out}");
}

#[test]
fn diff_keeps_the_unified_format_and_only_adds_color_on_request() {
    let diff = "--- remote/a.md\n+++ local/a.md\n@@ -1 +1 @@\n-old\n+new\n";
    let report = DiffReport {
        path: "a.md".into(),
        against: "remote",
        identical: false,
        diff: diff.into(),
    };
    assert_eq!(plain(&report), diff.trim_end());
    let colored = report.human(&Theme::new(true));
    assert!(colored.contains("\x1b[31m-old"), "{colored}");
    assert!(colored.contains("\x1b[32m+new"), "{colored}");
}

#[test]
fn identical_files_say_what_they_were_compared_with() {
    let report = |against| DiffReport {
        path: "a.md".into(),
        against,
        identical: true,
        diff: String::new(),
    };
    assert_eq!(
        plain(&report("remote")),
        "✔ No differences\n\n  file           a.md\n  compared with  the latest remote copy"
    );
    assert!(plain(&report("base")).ends_with("the last synced copy"));
}

#[test]
fn push_previews_before_sending_and_reports_the_commit_after() {
    let changes = vec![
        PlannedChange {
            action: "create",
            path: "new.md".into(),
        },
        PlannedChange {
            action: "update",
            path: "edited.md".into(),
        },
        PlannedChange {
            action: "delete",
            path: "old.md".into(),
        },
    ];
    // Stderr is not a terminal under `cargo test`, so the preview is plain.
    assert_eq!(
        push_preview(&changes),
        "ℹ Pushing 3 changes\n\n  Changes (3)\n    + new.md     create\n    ~ edited.md  update\n    - old.md     delete"
    );
    let report = PushReport {
        changes,
        commit_sha: Some(SHA.into()),
        head_sha: Some(SHA.into()),
        replayed: false,
        nothing_to_push: false,
    };
    assert_eq!(
        plain(&report),
        "✔ Pushed 3 changes\n\n  commit  0123456789ab"
    );
}

#[test]
fn push_edge_cases_are_explained() {
    let empty = PushReport {
        changes: vec![],
        commit_sha: None,
        head_sha: None,
        replayed: false,
        nothing_to_push: true,
    };
    assert_eq!(
        plain(&empty),
        "ℹ Nothing to push\n\n  There are no local changes since the last sync."
    );
    let replayed = PushReport {
        changes: vec![PlannedChange {
            action: "update",
            path: "a.md".into(),
        }],
        commit_sha: Some(SHA.into()),
        head_sha: Some(SHA.into()),
        replayed: true,
        nothing_to_push: false,
    };
    let out = plain(&replayed);
    assert!(out.starts_with("✔ Pushed 1 change\n"), "{out}");
    assert!(
        out.ends_with("  ℹ These changes were already applied earlier."),
        "{out}"
    );
}

#[test]
fn context_stays_a_bare_list_of_paths_for_agents_and_scripts() {
    let report = ContextReport {
        project_id: "p1".into(),
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
    };
    assert_eq!(plain(&same), "ℹ Nothing to install\n\n  installed  0.1.0");
}

#[test]
fn no_report_leaks_escape_codes_into_a_plain_theme() {
    let login = LoginReport {
        user_id: "u1".into(),
        email: None,
        name: None,
        projects: 1,
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
