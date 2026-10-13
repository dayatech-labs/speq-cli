//! End to end through the real binary, real Git, and the fake API: `speq clone`, Git commands run through speq,
//! `git-remote-speq` pushes with identical SHAs, and the refusals and conflicts users meet on the way.
mod support;

use std::path::Path;
use support::{Machine, PROJECT, STORY};

const PRD: &str = "epics/epic-payment/prd.md";

fn head(m: &Machine, dir: &Path) -> String {
    m.run(dir, &["rev-parse", "HEAD"])
        .ok()
        .stdout
        .trim()
        .to_string()
}

fn commit(m: &Machine, specs: &Path, path: &str, content: &str, message: &str) {
    m.write(&specs.join(path), content);
    m.run(specs, &["add", path]).ok();
    m.run(specs, &["commit", "-q", "-m", message]).ok();
}

#[tokio::test]
async fn clone_makes_a_git_repository_wired_to_speq_and_ignored_by_the_app() {
    let m = Machine::new().await;
    let app = m.dir("app");
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&app)
            .status()
            .unwrap()
            .success()
    );

    let out = m.run(&app, &["clone", "acme/payment-specs"]).ok();
    assert!(
        out.stdout.contains("Cloned acme/payment-specs"),
        "{}",
        out.stdout
    );
    let specs = app.join("specs");
    assert_eq!(
        std::fs::read_to_string(specs.join(PRD)).unwrap(),
        "# PRD\n\nPayments.\n"
    );
    // The checkout is GitHub's own commit: same SHA.
    assert_eq!(head(&m, &specs), m.api.head());

    let config = |key: &str| m.run(&specs, &["config", "--get-all", key]).ok().stdout;
    assert_eq!(
        config("remote.origin.pushurl").trim(),
        format!("speq::{}/v1/projects/{PROJECT}", m.api.url)
    );
    assert_eq!(
        config("remote.origin.url").trim(),
        format!("{}/v1/projects/{PROJECT}/git", m.api.url)
    );
    assert_eq!(
        config(&format!("credential.{}.helper", m.api.url)),
        "\nspeq\n"
    );
    assert_eq!(config("user.email").trim(), "user_a@example.com");
    assert_eq!(config("core.autocrlf").trim(), "false");
    assert_eq!(config("pull.rebase").trim(), "true");
    // The token stays in the credential store: Git's config never holds it.
    let raw_config = std::fs::read_to_string(specs.join(".git/config")).unwrap();
    assert!(!raw_config.contains("at_"), "{raw_config}");

    let ignore = std::fs::read_to_string(app.join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|l| l == "/specs/"), "{ignore}");
    assert!(m.run(&app, &["status", "--short"]).ok().stdout.is_empty());
}

#[tokio::test]
async fn clone_outside_a_repository_uses_the_repo_name_and_fails_cleanly() {
    let m = Machine::new().await;
    let work = m.dir("work");
    m.run(&work, &["clone", "cart-specs"]).ok();
    assert!(work.join("cart-specs/.git").is_dir());
    assert!(!work.join(".gitignore").exists());

    let taken = m.run(&work, &["clone", "acme/payment-specs", "cart-specs"]);
    assert_eq!(taken.code, 7, "{}", taken.stderr);
    assert!(taken.stderr.contains("already exists"));

    let ambiguous = m.run(&work, &["clone", "payment-specs"]);
    assert_eq!(ambiguous.code, 7);
    assert!(ambiguous.stderr.contains("ambiguous"));

    m.run(&work, &["logout"]).ok();
    let logged_out = m.run(&work, &["clone", "acme/payment-specs", "fresh"]);
    assert_eq!(logged_out.code, 3, "{}", logged_out.stderr);
    assert!(
        !work.join("fresh").exists(),
        "a failed clone leaves nothing behind"
    );
}

#[tokio::test]
async fn edit_commit_push_lands_on_the_remote_with_the_same_sha() {
    let m = Machine::new().await;
    let (app, specs) = m.clone_into_app();
    let before = m.api.head();

    // Typed from the implementation repository: speq runs Git in ./specs.
    m.write(&specs.join(PRD), "# PRD\n\nRefunds within 30 days.\n");
    m.run(&app, &["commit", "-q", "-am", "docs: add the refund rule"])
        .ok();
    m.run(&app, &["push"]).ok();

    let local = head(&m, &specs);
    assert_eq!(
        m.api.head(),
        local,
        "the remote branch points at the very commit made here"
    );
    assert_eq!(
        m.api.file(PRD).as_deref(),
        Some("# PRD\n\nRefunds within 30 days.\n")
    );
    let pushed = m.api.with(|s| s.pushes.clone());
    assert_eq!(pushed.len(), 1);
    assert_eq!(pushed[0]["old_head"], before.as_str());
    assert_eq!(pushed[0]["commits"][0]["base"], before.as_str());
    assert_eq!(
        pushed[0]["commits"][0]["changes"],
        serde_json::json!([{ "path": PRD, "content": "# PRD\n\nRefunds within 30 days.\n" }])
    );
    let status = m.run(&specs, &["status", "-sb"]).ok().stdout;
    assert!(status.starts_with("## main...origin/main\n"), "{status}");

    // Nothing new: Git does not even call the helper.
    m.run(&app, &["push"]).ok();
    assert_eq!(m.api.with(|s| s.pushes.len()), 1);
}

#[tokio::test]
async fn a_push_behind_the_remote_is_rejected_then_pull_rebases_and_push_succeeds() {
    let m = Machine::new().await;
    let (_app, specs) = m.clone_into_app();
    commit(&m, &specs, PRD, "# PRD mine\n", "docs: mine");
    let theirs = m.api.external_change("llms.txt", Some("# llms (theirs)\n"));

    // The remote has a commit this clone does not: refused before anything is sent, with Git's advice to pull.
    let rejected = m.run(&specs, &["push"]);
    assert_ne!(rejected.code, 0);
    assert!(
        rejected.stderr.contains("fetch first"),
        "{}",
        rejected.stderr
    );
    assert_eq!(m.api.with(|s| s.pushes.len()), 0);

    // Fetched but not integrated: not a fast-forward, also refused locally.
    m.run(&specs, &["fetch", "-q"]).ok();
    let diverged = m.run(&specs, &["push"]);
    assert_ne!(diverged.code, 0);
    assert!(
        diverged.stderr.contains("non-fast-forward"),
        "{}",
        diverged.stderr
    );
    assert_eq!(m.api.with(|s| s.pushes.len()), 0);

    m.run(&specs, &["pull"]).ok();
    m.run(&specs, &["push"]).ok();
    assert_eq!(m.api.head(), head(&m, &specs));
    assert_eq!(m.api.log()[1], theirs, "rebased: history stays linear");
    assert_eq!(m.api.file("llms.txt").as_deref(), Some("# llms (theirs)\n"));
    assert_eq!(m.api.file(PRD).as_deref(), Some("# PRD mine\n"));
}

#[tokio::test]
async fn a_race_lost_after_listing_comes_back_as_fetch_first() {
    let m = Machine::new().await;
    let (_app, specs) = m.clone_into_app();
    commit(&m, &specs, PRD, "# PRD mine\n", "docs: mine");
    m.api
        .with(|s| s.external_before_push = Some(("llms.txt".into(), "# raced\n".into())));

    let lost = m.run(&specs, &["push"]);
    assert_ne!(lost.code, 0);
    assert!(lost.stderr.contains("fetch first"), "{}", lost.stderr);
    assert_ne!(m.api.head(), head(&m, &specs));

    m.run(&specs, &["pull"]).ok();
    m.run(&specs, &["push"]).ok();
    assert_eq!(m.api.head(), head(&m, &specs));
}

#[tokio::test]
async fn conflicting_edits_stop_the_pull_and_are_resolved_like_git() {
    let m = Machine::new().await;
    let (app, specs) = m.clone_into_app();
    commit(&m, &specs, PRD, "# PRD\n\nMine.\n", "docs: mine");
    m.api.external_change(PRD, Some("# PRD\n\nTheirs.\n"));

    let pulled = m.run(&app, &["pull"]);
    assert_ne!(pulled.code, 0);
    assert!(
        format!("{}{}", pulled.stdout, pulled.stderr).contains("CONFLICT"),
        "{pulled:?}"
    );
    let marked = std::fs::read_to_string(specs.join(PRD)).unwrap();
    assert!(
        marked.contains("<<<<<<<") && marked.contains("|||||||") && marked.contains(">>>>>>>"),
        "{marked}"
    );

    m.write(&specs.join(PRD), "# PRD\n\nMine and theirs.\n");
    m.run(&app, &["add", PRD]).ok();
    m.run(&app, &["rebase", "--continue"]).ok();
    m.run(&app, &["push"]).ok();
    assert_eq!(
        m.api.file(PRD).as_deref(),
        Some("# PRD\n\nMine and theirs.\n")
    );
    assert_eq!(m.api.head(), head(&m, &specs));
}

#[tokio::test]
async fn a_merge_is_pushed_with_its_remote_parent_as_base() {
    let m = Machine::new().await;
    let (_app, specs) = m.clone_into_app();
    commit(&m, &specs, PRD, "# PRD mine\n", "docs: mine");
    let theirs = m.api.external_change("llms.txt", Some("# llms (theirs)\n"));

    m.run(&specs, &["pull", "--no-rebase", "--no-edit"]).ok();
    m.run(&specs, &["push"]).ok();
    let merge = head(&m, &specs);
    assert_eq!(m.api.head(), merge);
    let pushed = m.api.with(|s| s.pushes[0].clone());
    let last = &pushed["commits"][1];
    assert_eq!(last["sha"], merge.as_str());
    assert_eq!(
        last["base"],
        theirs.as_str(),
        "judged against the parent already on the remote"
    );
    assert_eq!(
        last["changes"],
        serde_json::json!([{ "path": PRD, "content": "# PRD mine\n" }])
    );
}

#[tokio::test]
async fn a_denied_path_rejects_the_whole_push_after_a_warning() {
    let m = Machine::new().await;
    m.api.with(|s| s.forbid_paths = vec![PRD.into()]);
    let (_app, specs) = m.clone_into_app();
    let before = m.api.head();
    commit(&m, &specs, PRD, "# PRD by me\n", "docs: not mine to change");
    let snapshots = m.api.count("GET /v1/projects");

    let denied = m.run(&specs, &["push"]);
    assert_ne!(denied.code, 0);
    assert!(
        denied.stderr.contains(&format!(
            "warning: your role may not be allowed to change {PRD}"
        )),
        "{}",
        denied.stderr
    );
    assert!(
        denied
            .stderr
            .contains(&format!("not allowed: update {PRD}")),
        "{}",
        denied.stderr
    );
    assert!(
        denied.stderr.contains("remote rejected"),
        "{}",
        denied.stderr
    );
    assert_eq!(m.api.head(), before);
    // A denial may mean the role changed: the cached capabilities were refreshed once.
    assert!(m.api.count("GET /v1/projects") > snapshots);
}

#[tokio::test]
async fn too_many_commits_point_to_squash_and_squash_pushes_one_commit() {
    let m = Machine::new().await;
    let (_app, specs) = m.clone_into_app();
    let before = m.api.head();
    for i in 0..21 {
        commit(
            &m,
            &specs,
            PRD,
            &format!("# PRD {i}\n"),
            &format!("docs: step {i}"),
        );
    }

    let refused = m.run(&specs, &["push"]);
    assert_ne!(refused.code, 0);
    assert!(
        refused.stderr.contains("speq push --squash"),
        "{}",
        refused.stderr
    );
    assert_eq!(
        m.api.with(|s| s.pushes.len()),
        0,
        "refused before anything was sent"
    );

    let squashed = m.run(&specs, &["push", "--squash"]).ok();
    assert!(
        squashed.stderr.contains("squashed 21 commits into one"),
        "{}",
        squashed.stderr
    );
    assert_eq!(m.api.head(), head(&m, &specs));
    assert_eq!(
        m.api.log()[1],
        before,
        "exactly one new commit on the remote"
    );
    let message = m.run(&specs, &["log", "-1", "--format=%B"]).ok().stdout;
    assert!(
        message.starts_with("Squash 21 commits\n\n* docs: step 0\n"),
        "{message}"
    );
    assert_eq!(m.api.file(PRD).as_deref(), Some("# PRD 20\n"));
}

#[tokio::test]
async fn squash_refuses_when_it_could_lose_work() {
    let m = Machine::new().await;
    let (_app, specs) = m.clone_into_app();
    commit(&m, &specs, PRD, "# a\n", "docs: a");
    commit(&m, &specs, PRD, "# b\n", "docs: b");
    let tip = head(&m, &specs);

    m.write(&specs.join("llms.txt"), "# staged\n");
    m.run(&specs, &["add", "llms.txt"]).ok();
    let staged = m.run(&specs, &["push", "--squash"]);
    assert_eq!(staged.code, 5, "{}", staged.stderr);
    assert!(staged.stderr.contains("staged changes"));
    assert_eq!(head(&m, &specs), tip);
    m.run(&specs, &["reset", "-q"]).ok();

    m.api.external_change("llms.txt", Some("# theirs\n"));
    m.run(&specs, &["fetch", "-q"]).ok();
    let behind = m.run(&specs, &["push", "--squash"]);
    assert_eq!(behind.code, 5, "{}", behind.stderr);
    assert!(behind.stderr.contains("speq pull"));
    assert_eq!(head(&m, &specs), tip);
}

#[tokio::test]
async fn force_pushes_other_branches_and_deletes_are_refused() {
    let m = Machine::new().await;
    let (_app, specs) = m.clone_into_app();
    commit(&m, &specs, PRD, "# one\n", "docs: one");
    m.run(&specs, &["push"]).ok();
    let remote = m.api.head();

    m.run(
        &specs,
        &["commit", "-q", "--amend", "-m", "docs: rewritten"],
    )
    .ok();
    let forced = m.run(&specs, &["push", "--force"]);
    assert_ne!(forced.code, 0);
    assert!(
        forced.stderr.contains("cannot be rewritten"),
        "{}",
        forced.stderr
    );
    m.run(&specs, &["reset", "-q", "--hard", "origin/main"])
        .ok();

    m.run(&specs, &["switch", "-q", "-c", "draft"]).ok();
    commit(&m, &specs, PRD, "# draft\n", "docs: draft");
    let branch = m.run(&specs, &["push", "origin", "draft"]);
    assert_ne!(branch.code, 0);
    assert!(
        branch.stderr.contains("speq push origin draft:main"),
        "{}",
        branch.stderr
    );
    let delete = m.run(&specs, &["push", "origin", ":main"]);
    assert_ne!(delete.code, 0);
    assert_eq!(m.api.head(), remote);

    m.run(&specs, &["push", "origin", "draft:main"]).ok();
    assert_eq!(m.api.head(), head(&m, &specs));
}

#[tokio::test]
async fn commits_by_another_identity_are_refused_with_how_to_fix() {
    let m = Machine::new().await;
    let (_app, specs) = m.clone_into_app();
    m.write(&specs.join(PRD), "# PRD\n\nby someone else\n");
    m.run(
        &specs,
        &[
            "-c",
            "user.email=intruder@example.com",
            "commit",
            "-q",
            "-am",
            "docs: x",
        ],
    )
    .ok();

    let refused = m.run(&specs, &["push"]);
    assert_ne!(refused.code, 0);
    assert!(
        refused
            .stderr
            .contains("commits must be authored as user_a@example.com"),
        "{}",
        refused.stderr
    );
    assert!(
        refused
            .stderr
            .contains("speq commit --amend --no-edit --reset-author"),
        "{}",
        refused.stderr
    );

    m.run(
        &specs,
        &["commit", "-q", "--amend", "--no-edit", "--reset-author"],
    )
    .ok();
    m.run(&specs, &["push"]).ok();
}

#[tokio::test]
async fn files_that_are_not_documents_are_refused_before_sending() {
    let m = Machine::new().await;
    let (_app, specs) = m.clone_into_app();
    commit(&m, &specs, ".github/CODEOWNERS", "* @me\n", "chore: owners");
    let refused = m.run(&specs, &["push"]);
    assert_ne!(refused.code, 0);
    assert!(
        refused
            .stderr
            .contains(".github/CODEOWNERS is not a Speq document path"),
        "{}",
        refused.stderr
    );
    assert_eq!(m.api.with(|s| s.pushes.len()), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn symlinks_are_refused_before_sending() {
    let m = Machine::new().await;
    let (_app, specs) = m.clone_into_app();
    std::os::unix::fs::symlink(
        "prd.md",
        specs.join("epics/epic-payment/architecture.md.tmp"),
    )
    .unwrap();
    std::fs::remove_file(specs.join("epics/epic-payment/architecture.md")).unwrap();
    std::fs::rename(
        specs.join("epics/epic-payment/architecture.md.tmp"),
        specs.join("epics/epic-payment/architecture.md"),
    )
    .unwrap();
    m.run(&specs, &["commit", "-q", "-am", "docs: link"]).ok();
    let refused = m.run(&specs, &["push"]);
    assert_ne!(refused.code, 0);
    assert!(
        refused.stderr.contains("must be a regular file"),
        "{}",
        refused.stderr
    );
}

#[tokio::test]
async fn git_runs_in_the_specs_repository_from_wherever_you_are() {
    let m = Machine::new().await;
    let (app, specs) = m.clone_into_app();
    let top = |dir: &Path| {
        m.run(dir, &["rev-parse", "--show-toplevel"])
            .ok()
            .stdout
            .trim()
            .to_string()
    };
    assert_eq!(Path::new(&top(&app)), specs);
    assert_eq!(Path::new(&top(&specs.join("epics/epic-payment"))), specs);
    // Inside the folder, relative paths mean what you see.
    let listed = m
        .run(&specs.join("epics/epic-payment"), &["ls-files", "prd.md"])
        .ok()
        .stdout;
    assert_eq!(listed.trim(), "prd.md");
    // Git's exit codes come through unchanged.
    assert_eq!(
        m.run(&specs, &["rev-parse", "--verify", "-q", "nope"]).code,
        1
    );

    let elsewhere = m.dir("elsewhere");
    let out = m.run(&elsewhere, &["status"]);
    assert_eq!(out.code, 7, "{}", out.stderr);
    assert!(out.stderr.contains("Not a Speq specs repository"));
    assert!(out.stderr.contains("speq clone <repo>"));
}

#[tokio::test]
async fn workspaces_of_the_older_speq_are_explained() {
    let m = Machine::new().await;
    let old = m.dir("old");
    std::fs::create_dir_all(old.join(".speq")).unwrap();
    std::fs::write(old.join(".speq/config.toml"), "[spec_source]\n").unwrap();
    let out = m.run(&old, &["status"]);
    assert_eq!(out.code, 7);
    assert!(out.stderr.contains("older speq"), "{}", out.stderr);
}

#[tokio::test]
async fn context_lists_documents_relative_to_where_you_are() {
    let m = Machine::new().await;
    let (app, specs) = m.clone_into_app();
    m.write(
        &specs.join("epics/epic-payment/stories/st-001-create-payment/layout.html"),
        "<main></main>\n",
    );
    m.write(
        &specs.join("epics/epic-payment/stories/st-001-create-payment/notes.txt"),
        "not a document\n",
    );

    let from_app = m.run(&app, &["context", STORY]).ok().stdout;
    assert_eq!(
        from_app.lines().collect::<Vec<_>>(),
        [
            "specs/llms.txt",
            "specs/epics/epic-payment/briefs/br-001-overview.md",
            "specs/epics/epic-payment/prd.md",
            "specs/epics/epic-payment/architecture.md",
            "specs/epics/epic-payment/stories/st-001-create-payment/README.md",
            "specs/epics/epic-payment/stories/st-001-create-payment/layout.html",
            "specs/epics/epic-payment/stories/st-001-create-payment/tasks/be-001-api.md",
        ]
    );
    let from_specs = m.run(&specs, &["context", STORY]).ok().stdout;
    assert!(from_specs.starts_with("llms.txt\n"), "{from_specs}");

    let json: serde_json::Value =
        serde_json::from_str(&m.run(&app, &["--json", "context", STORY]).ok().stdout).unwrap();
    assert_eq!(json["project_id"], PROJECT);
    assert_eq!(json["head_sha"], head(&m, &specs).as_str());

    for bad in [
        "epic-payment",
        "epic-payment/nope",
        "epic-payment/st-009-missing",
    ] {
        assert_eq!(m.run(&app, &["context", bad]).code, 7, "{bad}");
    }
}

#[tokio::test]
async fn the_credential_helper_answers_only_for_the_speq_origin() {
    let m = Machine::new().await;
    m.clone_into_app();
    let helper = Path::new(env!("CARGO_BIN_EXE_speq")).with_file_name(if cfg!(windows) {
        "git-credential-speq.exe"
    } else {
        "git-credential-speq"
    });
    let ask = |input: String| {
        use std::io::Write;
        // The same isolated environment `speq` runs with, under the helper's name.
        let speq = m.command(Path::new("."), &[]);
        let mut child = std::process::Command::new(&helper)
            .arg("get")
            .envs(
                speq.get_envs()
                    .filter_map(|(k, v)| v.map(|v| (k.to_owned(), v.to_owned()))),
            )
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let host = m.api.url.trim_start_matches("http://").to_string();
    let answer = ask(format!("protocol=http\nhost={host}\n\n"));
    assert!(
        answer.starts_with("username=speq\npassword=at_"),
        "{answer}"
    );
    // Another host never gets the token (the store is keyed by origin), and Git is told not to prompt.
    assert_eq!(
        ask("protocol=https\nhost=github.com\n\n".into()),
        "quit=1\n"
    );

    m.run(Path::new("."), &["logout"]).ok();
    assert_eq!(ask(format!("protocol=http\nhost={host}\n\n")), "quit=1\n");
}

#[tokio::test]
async fn deinit_deletes_the_specs_repository_and_clone_starts_again() {
    let m = Machine::new().await;
    let (app, specs) = m.clone_into_app();

    // No terminal and no --yes: refused, nothing deleted.
    let asked = m.run(&app, &["deinit"]);
    assert_eq!(asked.code, 7, "{}", asked.stderr);
    assert!(asked.stderr.contains("--yes"), "{}", asked.stderr);
    assert!(specs.join(".git").is_dir());

    let out = m.run(&app, &["deinit", "--yes"]).ok().stdout;
    assert!(out.starts_with("✔ Specs repository removed\n"), "{out}");
    assert!(
        out.contains(&format!("removed  {}", specs.display())),
        "{out}"
    );
    assert!(
        out.contains("lost     nothing (everything was pushed)"),
        "{out}"
    );
    assert!(
        out.contains("→ Run `speq clone <repo>` to start again."),
        "{out}"
    );
    assert!(!specs.exists());
    // The implementation repository, its ignore entry, and the login stay.
    assert!(app.join(".git").is_dir());
    assert!(
        std::fs::read_to_string(app.join(".gitignore"))
            .unwrap()
            .contains("/specs/")
    );
    m.run(&app, &["list"]).ok();

    let gone = m.run(&app, &["status"]);
    assert_eq!(gone.code, 7);
    m.run(&app, &["clone", "acme/payment-specs"]).ok();
    assert_eq!(head(&m, &specs), m.api.head());
}

#[tokio::test]
async fn deinit_reports_the_work_it_throws_away() {
    let m = Machine::new().await;
    let (app, specs) = m.clone_into_app();
    commit(&m, &specs, PRD, "# PRD local\n", "docs: never pushed");
    m.run(&specs, &["switch", "-q", "-c", "draft"]).ok();
    commit(&m, &specs, "llms.txt", "# draft\n", "docs: draft only");
    m.write(&specs.join("llms.txt"), "# stashed\n");
    m.run(&specs, &["stash", "-q"]).ok();
    m.write(
        &specs.join("epics/epic-payment/briefs/br-009-new.md"),
        "# New\n",
    );

    let json: serde_json::Value =
        serde_json::from_str(&m.run(&app, &["--json", "deinit", "--yes"]).ok().stdout).unwrap();
    assert_eq!(
        json["lost"],
        serde_json::json!({ "uncommitted": 1, "unpushed": 2, "stashes": 1 })
    );
    assert_eq!(json["legacy"], false);
    assert!(!specs.exists());
}

#[tokio::test]
async fn deinit_from_inside_the_folder_steps_out_and_says_so() {
    let m = Machine::new().await;
    let (_app, specs) = m.clone_into_app();
    let out = m
        .run(&specs.join("epics/epic-payment"), &["deinit", "--yes"])
        .ok()
        .stdout;
    assert!(
        out.contains("! Your shell was inside it: run `cd ..`."),
        "{out}"
    );
    assert!(!specs.exists());
}

#[tokio::test]
async fn deinit_removes_a_workspace_of_the_older_speq() {
    let m = Machine::new().await;
    let old = m.dir("old");
    std::fs::create_dir_all(old.join(".speq/base")).unwrap();
    std::fs::write(
        old.join(".speq/config.toml"),
        "[spec_source]\nproject_id = \"p\"\nrepository = \"acme/payment-specs\"\nbranch = \"main\"\nlocal_dir = \"docs/specs\"\n",
    )
    .unwrap();
    m.write(&old.join("docs/specs/llms.txt"), "# old\n");
    m.write(&old.join("docs/keep.md"), "not speq's\n");

    let hinted = m.run(&old, &["status"]);
    assert!(
        hinted.stderr.contains("run `speq deinit` to remove it"),
        "{}",
        hinted.stderr
    );

    let out = m.run(&old.join("docs"), &["deinit", "--yes"]).ok().stdout;
    assert!(out.starts_with("✔ Older speq workspace removed\n"), "{out}");
    assert!(!out.contains("lost"), "{out}");
    assert!(!old.join(".speq").exists() && !old.join("docs/specs").exists());
    assert!(
        old.join("docs/keep.md").exists(),
        "only the specs folder and .speq/ go"
    );
}

#[tokio::test]
async fn deinit_outside_any_specs_folder_changes_nothing() {
    let m = Machine::new().await;
    let elsewhere = m.dir("elsewhere");
    std::fs::write(elsewhere.join("file.txt"), "keep").unwrap();
    let out = m.run(&elsewhere, &["deinit", "--yes"]);
    assert_eq!(out.code, 7, "{}", out.stderr);
    assert!(out.stderr.contains("Not a Speq specs repository"));
    assert!(elsewhere.join("file.txt").exists());
}

const ORIGIN_WARNING: &str = "speq: origin no longer points to this Speq project";

/// Runs the commands speq printed to restore `origin`, exactly as a user would copy them.
fn run_printed_repair(m: &Machine, specs: &Path, stderr: &str) {
    let commands: Vec<&str> = stderr
        .lines()
        .skip_while(|l| !l.starts_with(ORIGIN_WARNING))
        .skip(1)
        .take_while(|l| l.starts_with("  speq "))
        .collect();
    assert!(!commands.is_empty(), "{stderr}");
    for command in commands {
        let args: Vec<&str> = command.split_whitespace().skip(1).collect();
        m.run(specs, &args).ok();
    }
}

#[tokio::test]
async fn a_changed_origin_is_explained_and_the_printed_commands_restore_it() {
    let m = Machine::new().await;
    let (app, specs) = m.clone_into_app();
    let address = format!("{}/v1/projects/{PROJECT}", m.api.url);
    assert_eq!(
        m.run(&specs, &["config", "speq.address"])
            .ok()
            .stdout
            .trim(),
        address
    );
    // Untouched origin: no advice.
    assert!(
        !m.run(&app, &["status"])
            .ok()
            .stderr
            .contains(ORIGIN_WARNING)
    );

    // Pointing the push somewhere else is answered by the very command that does it.
    let elsewhere = "http://127.0.0.1:1/acme/payment-specs.git";
    let changed = m
        .run(
            &specs,
            &["remote", "set-url", "--push", "origin", elsewhere],
        )
        .ok();
    assert!(
        changed.stderr.contains(ORIGIN_WARNING),
        "{}",
        changed.stderr
    );
    assert!(
        changed.stderr.contains(&format!(
            "  speq remote set-url --push origin speq::{address}\n"
        )),
        "{}",
        changed.stderr
    );
    assert!(
        !changed.stderr.contains("set-url origin "),
        "{}",
        changed.stderr
    );

    // The folder is still a specs repository: every command keeps working and keeps saying how to fix it.
    m.write(&specs.join(PRD), "# PRD\n\nRefunds within 30 days.\n");
    m.run(
        &specs,
        &["commit", "-q", "-am", "docs: add the refund rule"],
    )
    .ok();
    let push = m.run(&specs, &["push"]);
    assert_ne!(push.code, 0);
    assert!(push.stderr.contains(ORIGIN_WARNING), "{}", push.stderr);
    assert!(m.api.with(|s| s.pushes.is_empty()));

    run_printed_repair(&m, &specs, &changed.stderr);
    let pushed = m.run(&app, &["push"]).ok();
    assert!(!pushed.stderr.contains(ORIGIN_WARNING), "{}", pushed.stderr);
    assert_eq!(m.api.head(), head(&m, &specs));
}

#[tokio::test]
async fn a_removed_origin_can_be_added_back_with_the_printed_commands() {
    let m = Machine::new().await;
    let (app, specs) = m.clone_into_app();

    let removed = m.run(&app, &["remote", "remove", "origin"]).ok();
    assert!(
        removed.stderr.contains(&format!(
            "  speq remote add -t main origin {}/v1/projects/{PROJECT}/git\n",
            m.api.url
        )),
        "{}",
        removed.stderr
    );
    // Not locked out: speq still finds the repository, from the app too.
    m.run(&app, &["log", "--oneline"]).ok();

    m.api
        .external_change(PRD, Some("# PRD\n\nChanged on the web.\n"));
    run_printed_repair(&m, &specs, &removed.stderr);
    let status = m.run(&specs, &["status", "-sb"]).ok();
    assert!(
        status
            .stdout
            .starts_with("## main...origin/main [behind 1]\n"),
        "{}",
        status.stdout
    );
    assert!(!status.stderr.contains(ORIGIN_WARNING), "{}", status.stderr);

    m.run(&app, &["pull", "-q"]).ok();
    commit(&m, &specs, PRD, "# PRD\n\nBack online.\n", "docs: reword");
    m.run(&app, &["push"]).ok();
    assert_eq!(m.api.head(), head(&m, &specs));
}

#[tokio::test]
async fn deinit_still_works_without_origin() {
    let m = Machine::new().await;
    let (app, specs) = m.clone_into_app();
    m.run(&app, &["remote", "remove", "origin"]).ok();
    m.run(&app, &["deinit", "--yes"]).ok();
    assert!(!specs.exists());
}

#[tokio::test]
async fn a_repository_cloned_before_speq_address_is_still_recognised() {
    let m = Machine::new().await;
    let (app, specs) = m.clone_into_app();
    m.run(&specs, &["config", "--unset", "speq.address"]).ok();
    let out = m.run(&app, &["status"]).ok();
    assert!(!out.stderr.contains(ORIGIN_WARNING), "{}", out.stderr);
}
