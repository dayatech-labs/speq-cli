//! The real binary: help, version, usage errors, and what reaches Git. None of these touch credentials or the
//! network.
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

const COMMANDS: [&str; 9] = [
    "login",
    "logout",
    "list",
    "clone",
    "deinit",
    "context",
    "skills",
    "upgrade",
    "uninstall",
];

#[test]
fn skills_install_writes_the_skills_for_every_agent_without_login() {
    let home = tempfile::tempdir().expect("tempdir");
    // Windows resolves the app-data folders under `%USERPROFILE%\AppData` and needs both to exist.
    for dir in ["Local", "Roaming"] {
        std::fs::create_dir_all(home.path().join("AppData").join(dir)).expect("app data");
    }
    let install = |claude_config: Option<&std::path::Path>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_speq"));
        command
            .args(["--json", "skills", "install"])
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env_remove("CLAUDE_CONFIG_DIR");
        if let Some(dir) = claude_config {
            command.env("CLAUDE_CONFIG_DIR", dir);
        }
        let out = command.output().expect("run");
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&out.stdout).expect("json report")
    };

    let body = install(None);
    assert_eq!(
        body["skills"],
        serde_json::json!(["speq-cli", "speq-write"])
    );
    for dir in [".agents/skills", ".claude/skills"] {
        for skill in ["speq-cli", "speq-write"] {
            assert!(
                home.path().join(dir).join(skill).join("SKILL.md").is_file(),
                "{dir}/{skill}"
            );
        }
    }

    let custom = home.path().join("claude-config");
    install(Some(&custom));
    for skill in ["speq-cli", "speq-write"] {
        assert!(
            custom.join("skills").join(skill).join("SKILL.md").is_file(),
            "{skill}"
        );
    }
}

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
    // The Git commands people need most, the push rules, and Git's manual are listed too.
    assert!(
        out.contains("Git commands:\n  speq status | diff | log | show "),
        "{out}"
    );
    for line in [
        "  speq add | restore | rm | mv ",
        "  speq commit ",
        "  speq pull | fetch ",
        "  speq push  ",
        "  speq push --squash ",
        "  speq switch | branch | stash | rebase ",
        "  speq help <git command> ",
    ] {
        assert!(out.contains(line), "missing {line:?} in help");
    }
    assert!(
        out.contains("Pushes go to main only: no force push"),
        "{out}"
    );
    // What Git commands are comes before the first steps.
    assert!(out.find("Git commands:") < out.find("Get started:"));
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
    let (_, clone, _) = speq(&["clone", "--help"]);
    assert!(
        clone.contains("<REPO>") && clone.contains("[DIR]"),
        "{clone}"
    );
    let (_, upgrade, _) = speq(&["upgrade", "--help"]);
    assert!(
        upgrade.contains("--check") && upgrade.contains("--yes") && upgrade.contains("--version")
    );
}

#[test]
fn usage_errors_of_speq_commands_exit_2() {
    assert_eq!(speq(&["clone"]).0, 2);
    assert_eq!(speq(&["upgrade", "--force"]).0, 2);
    assert_eq!(speq(&["context"]).0, 2);
    assert_eq!(speq(&["login", "extra"]).0, 2);
}

#[test]
fn git_commands_outside_a_specs_repository_exit_7_without_touching_the_network() {
    let dir = tempfile::tempdir().expect("tempdir");
    for args in [&["status"][..], &["nope"], &["commit", "-m", "x"]] {
        let out = Command::new(env!("CARGO_BIN_EXE_speq"))
            .args(args)
            .current_dir(dir.path())
            .env("GIT_CEILING_DIRECTORIES", dir.path())
            .output()
            .expect("run");
        assert_eq!(out.status.code(), Some(7), "{args:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.starts_with("✖ Not a Speq specs repository\n"),
            "{stderr}"
        );
        assert!(stderr.contains("→ Run `speq clone <repo>` (see `speq list`)"));
    }
}

#[test]
fn without_git_speq_says_how_to_install_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::new(env!("CARGO_BIN_EXE_speq"))
        .args(["status"])
        .current_dir(dir.path())
        .env("PATH", dir.path())
        .env_remove("SPEQ_GIT")
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("✖ Git is required\n"), "{stderr}");
    assert!(stderr.contains("→ "), "{stderr}");
}

#[test]
fn json_errors_also_cover_git_commands() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::new(env!("CARGO_BIN_EXE_speq"))
        .args(["--json", "status"])
        .current_dir(dir.path())
        .env("GIT_CEILING_DIRECTORIES", dir.path())
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(7));
    let body: serde_json::Value = serde_json::from_slice(&out.stderr).expect("json error");
    assert_eq!(body["error"]["code"], "not_initialized");
}

#[test]
fn help_and_version_never_need_git_or_a_repository() {
    let dir = tempfile::tempdir().expect("tempdir");
    for args in [
        &["--version"][..],
        &["help"],
        &["help", "clone"],
        &["clone", "--help"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_speq"))
            .args(args)
            .current_dir(dir.path())
            .env("PATH", dir.path())
            .output()
            .expect("run");
        assert_eq!(out.status.code(), Some(0), "{args:?}");
    }
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
    let (_, plain, _) = speq(&["clone", "--help"]);
    assert!(
        !plain.contains('\x1b'),
        "piped help must have no ANSI codes"
    );

    let colored = speq_env(&["clone", "--help"], "CLICOLOR_FORCE", "1");
    assert!(colored.contains('\x1b'), "forced help should be colored");
    // Clap-generated parts and our hand-written sections are both painted.
    assert!(colored.contains("\x1b[1m\x1b[33mUsage:"), "{colored}");
    assert!(colored.contains("\x1b[1m\x1b[33mExamples:"), "{colored}");
    assert!(
        colored.contains("\x1b[1m\x1b[32mspeq clone payment-specs"),
        "{colored}"
    );
    assert!(colored.contains("\x1b[36m<REPO>"), "{colored}");
    assert!(
        colored.contains("\x1b[1m\x1b[32m5\x1b[0m  conflict"),
        "{colored}"
    );

    let top = speq_env(&["help"], "CLICOLOR_FORCE", "1");
    assert!(top.contains("\x1b[1m\x1b[33mGet started:"), "{top}");
    assert!(top.contains("\x1b[1m\x1b[33mGit commands:"), "{top}");
    // Only the command is painted; its description after the column gap stays plain.
    assert!(
        top.contains(
            "\x1b[1m\x1b[32mspeq commit\x1b[0m                              record staged changes"
        ),
        "{top}"
    );
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
    let (_, sub, _) = speq(&["clone", "--help"]);
    assert!(!sub.contains("_____"), "subcommand help has no banner");

    let colored = speq_env(&["help"], "CLICOLOR_FORCE", "1");
    assert!(
        colored.starts_with("\x1b[1m\x1b[36m ____  ____"),
        "{colored}"
    );
}
