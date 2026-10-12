//! The real binary: help, version, usage errors. None of these touch credentials or the network.
use std::process::Command;

fn speq(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_speq"))
        .args(args)
        .env_remove("CLICOLOR_FORCE")
        .env_remove("NO_COLOR")
        .output()
        .expect("run speq");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const COMMANDS: [&str; 14] = [
    "login",
    "logout",
    "list",
    "init",
    "deinit",
    "pull",
    "status",
    "diff",
    "update",
    "resolve",
    "push",
    "context",
    "upgrade",
    "uninstall",
];

#[test]
fn version_reports_semver_commit_and_target() {
    let (code, out, _) = speq(&["--version"]);
    assert_eq!(code, 0);
    assert!(out.starts_with(&format!("speq {}", env!("CARGO_PKG_VERSION"))));
    assert!(out.contains("commit: ") && out.contains("target: "));
}

#[test]
fn help_lists_every_command_with_a_summary_and_exit_codes() {
    let (code, out, _) = speq(&["help"]);
    assert_eq!(code, 0);
    for c in COMMANDS {
        assert!(out.contains(&format!("  {c} ")), "missing {c} in help");
    }
    assert!(out.contains("Exit codes:"));
}

#[test]
fn every_command_help_has_usage_arguments_flags_examples_and_exit_codes() {
    for c in COMMANDS {
        let (code, out, _) = speq(&[c, "--help"]);
        assert_eq!(code, 0, "{c}");
        assert!(out.contains("Usage: speq"), "{c}");
        assert!(out.contains("Examples:"), "{c}");
        assert!(out.contains("Exit codes:"), "{c}");
        assert!(out.contains("--json"), "{c}");
        // `speq help <command>` is equivalent.
        assert_eq!(speq(&["help", c]).1, out, "{c}");
    }
    let (_, init, _) = speq(&["init", "--help"]);
    assert!(
        init.contains("--local-dir") && init.contains("--reconfigure") && !init.contains("--type")
    );
    let (_, upgrade, _) = speq(&["upgrade", "--help"]);
    assert!(
        upgrade.contains("--check") && upgrade.contains("--yes") && upgrade.contains("--version")
    );
    let (_, diff, _) = speq(&["diff", "--help"]);
    assert!(diff.contains("--base"));
}

#[test]
fn usage_errors_exit_2_and_force_flags_do_not_exist() {
    assert_eq!(speq(&["pull", "--feature", "x"]).0, 2);
    assert_eq!(speq(&["update", "--force", "a.md"]).0, 2);
    assert_eq!(speq(&["nope"]).0, 2);
    assert_eq!(speq(&["diff"]).0, 2);
}

#[test]
fn commands_outside_a_workspace_exit_7_without_touching_the_network() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::new(env!("CARGO_BIN_EXE_speq"))
        .args(["status"])
        .current_dir(dir.path())
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(7));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("✖ Not a Speq workspace\n"));
    assert!(stderr.contains("→ Run `speq init <repo>`"));
    assert!(
        !stderr.contains('\x1b'),
        "no colors when stderr is not a terminal"
    );
}

#[test]
fn context_outside_a_workspace_reports_json_errors_when_asked() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::new(env!("CARGO_BIN_EXE_speq"))
        .args(["--json", "context", "a/b"])
        .current_dir(dir.path())
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(7));
    let body: serde_json::Value = serde_json::from_slice(&out.stderr).expect("json error");
    assert_eq!(body["error"]["code"], "not_initialized");
}

#[test]
fn upgrade_check_fails_non_zero_when_the_release_server_is_unreachable() {
    let out = Command::new(env!("CARGO_BIN_EXE_speq"))
        .args(["upgrade", "--check"])
        .env("SPEQ_RELEASE_URL", "http://127.0.0.1:1/releases")
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(4));
}

#[test]
fn uninstall_without_a_terminal_asks_for_yes_and_changes_nothing() {
    // Refuses before touching anything: with no terminal there is nobody to confirm.
    let (code, _, err) = speq(&["uninstall"]);
    assert_eq!(code, 7);
    assert!(err.contains("--yes"), "{err}");
    assert!(std::path::Path::new(env!("CARGO_BIN_EXE_speq")).exists());
}

#[test]
fn deinit_without_a_terminal_asks_for_yes_and_deletes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join(".speq")).unwrap();
    std::fs::write(
        dir.path().join(".speq/config.toml"),
        "[spec_source]\nproject_id = \"p\"\nrepository = \"acme/specs\"\nbranch = \"main\"\nlocal_dir = \"specs\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("specs")).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_speq"))
        .arg("deinit")
        .current_dir(dir.path())
        .output()
        .expect("run speq");

    assert_eq!(out.status.code(), Some(7));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--yes"));
    assert!(dir.path().join(".speq/config.toml").exists());
    assert!(dir.path().join("specs").exists());
}

fn speq_env(args: &[&str], key: &str, value: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_speq"))
        .args(args)
        .env_remove("CLICOLOR_FORCE")
        .env_remove("NO_COLOR")
        .env(key, value)
        .output()
        .expect("run speq");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn help_is_plain_when_piped_and_colored_when_forced() {
    let (_, plain, _) = speq(&["init", "--help"]);
    assert!(
        !plain.contains('\x1b'),
        "piped help must have no ANSI codes"
    );

    let colored = speq_env(&["init", "--help"], "CLICOLOR_FORCE", "1");
    assert!(colored.contains('\x1b'), "forced help should be colored");
    // Clap-generated parts and our hand-written sections are both painted.
    assert!(colored.contains("\x1b[1m\x1b[33mUsage:"), "{colored}");
    assert!(colored.contains("\x1b[1m\x1b[33mExamples:"), "{colored}");
    assert!(
        colored.contains("\x1b[1m\x1b[32mspeq init payment-specs"),
        "{colored}"
    );
    assert!(colored.contains("\x1b[36m<REPO>"), "{colored}");
    assert!(
        colored.contains("\x1b[1m\x1b[32m5\x1b[0m  conflict"),
        "{colored}"
    );

    let top = speq_env(&["help"], "CLICOLOR_FORCE", "1");
    assert!(top.contains("\x1b[1m\x1b[33mGet started:"), "{top}");
}

#[test]
fn no_color_wins_over_forced_color() {
    let out = Command::new(env!("CARGO_BIN_EXE_speq"))
        .args(["help"])
        .env("CLICOLOR_FORCE", "1")
        .env("NO_COLOR", "1")
        .output()
        .expect("run speq");
    assert!(!String::from_utf8_lossy(&out.stdout).contains('\x1b'));
}

#[test]
fn only_the_top_level_help_shows_the_speq_cli_banner() {
    let logo = r"\___ \| |_) |  _| | | | |  | |   | |    | | ";
    for args in [&["help"][..], &["--help"], &[]] {
        let (_, out, err) = speq(args);
        let text = format!("{out}{err}");
        assert!(
            text.starts_with(
                " ____  ____  _____  ___      ____ _     ___ 
"
            ),
            "{args:?}: {text}"
        );
        assert!(text.contains(logo), "{args:?}");
        assert!(text.contains("Usage: speq"), "{args:?}");
    }
    let (_, sub, _) = speq(&["pull", "--help"]);
    assert!(!sub.contains("_____"), "subcommand help has no banner");

    let colored = speq_env(&["help"], "CLICOLOR_FORCE", "1");
    assert!(
        colored.starts_with("\x1b[1m\x1b[36m ____  ____"),
        "{colored}"
    );
}

#[test]
fn resolve_rejects_contradictory_or_missing_targets() {
    for args in [
        vec!["resolve"],
        vec!["resolve", "--all", "a.md"],
        vec!["resolve", "--all", "--ours", "--theirs"],
    ] {
        let (code, _, stderr) = speq(&args);
        assert_eq!(code, 2, "{args:?}: {stderr}");
    }
}
