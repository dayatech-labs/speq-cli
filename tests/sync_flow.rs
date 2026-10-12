mod support;

use speq::context::context;
use speq::error::{Error, exit};
use speq::hashing::git_blob_sha;
use speq::sync::{push, status};
use support::{Fixture, PROJECT};

const PRD: &str = "epics/epic-payment/prd.md";
const README: &str = "epics/epic-payment/stories/st-001-create-payment/README.md";
const NEW_DOC: &str = "epics/epic-payment/stories/st-001-create-payment/tasks/fe-001-form.md";

#[tokio::test]
async fn first_pull_downloads_everything_and_advances_the_lock() {
    let f = Fixture::new().await;
    let report = f.pull().await.unwrap();
    assert_eq!(report.added.len(), 6);
    assert!(report.lock_advanced && report.conflicts.is_empty());
    assert_eq!(f.read(PRD).as_deref(), Some("# PRD\n\nPayments.\n"));
    let lock = f.ws().load_lock().unwrap();
    assert_eq!(lock.head_sha.as_deref(), Some(f.api.head().as_str()));
    // The baseline copy and manifest entry exist for every applied path.
    assert_eq!(f.ws().load_manifest().unwrap().files.len(), 6);
    assert_eq!(
        f.ws().read_base(PRD).unwrap().unwrap(),
        b"# PRD\n\nPayments.\n"
    );

    // A second pull is a no-op.
    let again = f.pull().await.unwrap();
    assert!(again.added.is_empty() && again.updated.is_empty() && again.deleted.is_empty());
}

#[tokio::test]
async fn pull_applies_remote_changes_but_keeps_local_only_edits() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.write(README, "# mine\n"); // local-only edit
    f.api.external_change(PRD, Some("# PRD v2\n"));
    f.api.external_change(
        "epics/epic-payment/stories/st-001-create-payment/DESIGN.md",
        Some("# New\n"),
    );
    f.api
        .external_change("epics/epic-payment/architecture.md", None);

    let report = f.pull().await.unwrap();
    assert_eq!(report.updated, vec![PRD.to_string()]);
    assert_eq!(
        report.added,
        vec!["epics/epic-payment/stories/st-001-create-payment/DESIGN.md".to_string()]
    );
    assert_eq!(
        report.deleted,
        vec!["epics/epic-payment/architecture.md".to_string()]
    );
    assert_eq!(f.read(README).as_deref(), Some("# mine\n"));
    assert_eq!(f.read(PRD).as_deref(), Some("# PRD v2\n"));
    assert!(f.read("epics/epic-payment/architecture.md").is_none());
    assert!(report.lock_advanced);
}

#[tokio::test]
async fn conflicts_are_never_overwritten_and_block_push() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    let before_lock = f.ws().load_lock().unwrap();

    f.write(PRD, "# local PRD\n"); // both changed
    f.api.external_change(PRD, Some("# remote PRD\n"));
    f.write(
        "epics/epic-payment/briefs/br-001-overview.md",
        "# local brief\n",
    ); // remote delete over local edit
    f.api
        .external_change("epics/epic-payment/briefs/br-001-overview.md", None);
    f.write(NEW_DOC, "# my frontend doc\n"); // remote add over untracked local file
    f.api
        .external_change(NEW_DOC, Some("# remote frontend doc\n"));

    let report = f.pull().await.unwrap();
    let mut conflicted: Vec<&str> = report.conflicts.iter().map(|c| c.path.as_str()).collect();
    conflicted.sort();
    assert_eq!(
        conflicted,
        vec![
            "epics/epic-payment/briefs/br-001-overview.md",
            PRD,
            "epics/epic-payment/stories/st-001-create-payment/tasks/fe-001-form.md",
        ]
    );
    assert!(!report.lock_advanced);
    // Both-sides content is kept inside conflict markers, never dropped.
    assert_eq!(
        f.read(PRD).as_deref(),
        Some(
            "<<<<<<< local\n# local PRD\n||||||| base\n# PRD\n\nPayments.\n=======\n# remote PRD\n>>>>>>> remote\n"
        )
    );
    assert_eq!(
        f.read(NEW_DOC).as_deref(),
        Some(
            "<<<<<<< local\n# my frontend doc\n||||||| base\n=======\n# remote frontend doc\n>>>>>>> remote\n"
        )
    );
    // A delete has nothing to mark: the local edit stays exactly as it was.
    assert_eq!(
        f.read("epics/epic-payment/briefs/br-001-overview.md")
            .as_deref(),
        Some("# local brief\n")
    );
    let has_markers = |p: &str| {
        report
            .conflicts
            .iter()
            .find(|c| c.path == p)
            .unwrap()
            .markers
    };
    assert!(has_markers(PRD) && has_markers(NEW_DOC));
    assert!(!has_markers("epics/epic-payment/briefs/br-001-overview.md"));
    let lock = f.ws().load_lock().unwrap();
    assert_eq!(lock.head_sha, before_lock.head_sha);
    assert_eq!(lock.pending.len(), 3);
    assert_eq!(lock.marked.len(), 2);
    assert_eq!(lock.marked[PRD], git_blob_sha(b"# remote PRD\n"));

    // Push is refused while conflicts are pending, and no request is sent.
    let commits = f.api.commits();
    let err = push::push(&f.device.env, &f.ws()).await.unwrap_err();
    assert_eq!(err.exit_code(), exit::CONFLICT);
    assert_eq!(f.api.commits(), commits);
    // So is context, which needs a clean lock.
    assert!(context(&f.ws(), "epic-payment/st-001-create-payment").is_err());
}

/// A pulled workspace where `PRD` changed on both sides, so the file now holds conflict markers.
async fn marked_prd_conflict() -> Fixture {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.write(PRD, "# local PRD\n");
    f.api.external_change(PRD, Some("# remote PRD\n"));
    let report = f.pull().await.unwrap();
    assert_eq!(report.conflicts.len(), 1);
    f
}

#[tokio::test]
async fn pulling_again_leaves_unresolved_markers_untouched() {
    let f = marked_prd_conflict().await;
    let marked = f.read(PRD).unwrap();
    let lock = f.ws().load_lock().unwrap();

    let again = f.pull().await.unwrap();
    assert_eq!(again.conflicts.len(), 1);
    assert!(again.conflicts[0].markers && !again.lock_advanced);
    assert_eq!(
        f.read(PRD).unwrap(),
        marked,
        "markers are never wrapped in markers"
    );
    assert_eq!(f.ws().load_lock().unwrap(), lock);

    // The remote moving on does not rewrite a file the user is still editing.
    f.api.external_change(PRD, Some("# remote PRD v3\n"));
    let moved = f.pull().await.unwrap();
    assert_eq!(moved.conflicts.len(), 1);
    assert_eq!(f.read(PRD).unwrap(), marked);
    assert_eq!(
        f.ws().load_lock().unwrap().marked[PRD],
        git_blob_sha(b"# remote PRD\n"),
        "the markers still describe the remote they were written against"
    );
}

#[tokio::test]
async fn an_edit_that_removes_the_markers_resolves_the_conflict_and_can_be_pushed() {
    let f = marked_prd_conflict().await;
    f.write(PRD, "# merged PRD\n");

    let report = f.pull().await.unwrap();
    assert_eq!(report.resolved, vec![PRD.to_string()]);
    assert!(report.conflicts.is_empty() && report.lock_advanced);
    assert_eq!(
        f.read(PRD).as_deref(),
        Some("# merged PRD\n"),
        "the edit is kept"
    );
    assert_eq!(f.ws().read_base(PRD).unwrap().unwrap(), b"# remote PRD\n");
    let lock = f.ws().load_lock().unwrap();
    assert!(lock.pending.is_empty() && lock.marked.is_empty());

    let status = status::status(&f.device.env, &f.ws()).await.unwrap();
    let state = |p: &str| status.files.iter().find(|x| x.path == p).unwrap().state;
    assert_eq!(state(PRD), "local-only");

    let pushed = push::push(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(pushed.changes.len(), 1);
    assert_eq!(f.api.file(PRD).as_deref(), Some("# merged PRD\n"));
}

#[tokio::test]
async fn accepting_the_remote_side_is_adopted_with_nothing_to_push() {
    let f = marked_prd_conflict().await;
    f.write(PRD, "# remote PRD\n");

    let report = f.pull().await.unwrap();
    assert_eq!(report.adopted, vec![PRD.to_string()]);
    assert!(report.resolved.is_empty() && report.lock_advanced);
    let pushed = push::push(&f.device.env, &f.ws()).await.unwrap();
    assert!(pushed.nothing_to_push);
}

#[tokio::test]
async fn a_resolution_made_against_an_outdated_remote_reopens_the_conflict() {
    let f = marked_prd_conflict().await;
    f.write(PRD, "# merged PRD\n");
    f.api.external_change(PRD, Some("# remote PRD v3\n")); // moved before the next pull

    let report = f.pull().await.unwrap();
    assert!(report.resolved.is_empty() && !report.lock_advanced);
    assert_eq!(report.conflicts.len(), 1);
    // The user's resolution is preserved next to the remote they have not seen yet.
    assert_eq!(
        f.read(PRD).as_deref(),
        Some(
            "<<<<<<< local\n# merged PRD\n||||||| base\n# PRD\n\nPayments.\n=======\n# remote PRD v3\n>>>>>>> remote\n"
        )
    );
    assert_eq!(
        f.ws().load_lock().unwrap().marked[PRD],
        git_blob_sha(b"# remote PRD v3\n")
    );
    let commits = f.api.commits();
    assert!(push::push(&f.device.env, &f.ws()).await.is_err());
    assert_eq!(f.api.commits(), commits);
}

#[tokio::test]
async fn resolving_one_conflict_keeps_the_others_pending() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.write(PRD, "# local PRD\n");
    f.api.external_change(PRD, Some("# remote PRD\n"));
    f.write(README, "# local readme\n");
    f.api.external_change(README, Some("# remote readme\n"));
    f.pull().await.unwrap();

    f.write(PRD, "# merged PRD\n"); // README markers stay
    let report = f.pull().await.unwrap();
    assert_eq!(report.resolved, vec![PRD.to_string()]);
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(report.conflicts[0].path, README);
    assert!(!report.lock_advanced);
    let lock = f.ws().load_lock().unwrap();
    assert_eq!(lock.pending, vec![README.to_string()]);
    assert_eq!(lock.marked.keys().collect::<Vec<_>>(), vec![README]);
    assert_eq!(f.read(PRD).as_deref(), Some("# merged PRD\n"));
}

const FIVE: &str = "one\ntwo\nthree\nfour\nfive\n";

/// A pulled workspace whose `PRD` is a five-line document, so edits can land on different lines.
async fn five_line_prd() -> Fixture {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.api.external_change(PRD, Some(FIVE));
    f.pull().await.unwrap();
    assert_eq!(f.read(PRD).as_deref(), Some(FIVE));
    f
}

#[tokio::test]
async fn edits_to_different_lines_merge_cleanly_and_can_be_pushed() {
    let f = five_line_prd().await;
    f.write(PRD, "ONE\ntwo\nthree\nfour\nfive\n"); // local: first line
    f.api
        .external_change(PRD, Some("one\ntwo\nthree\nfour\nFIVE\n")); // remote: last line

    let report = f.pull().await.unwrap();
    assert_eq!(report.merged, vec![PRD.to_string()]);
    assert!(report.conflicts.is_empty() && report.lock_advanced);
    let merged = "ONE\ntwo\nthree\nfour\nFIVE\n";
    assert_eq!(f.read(PRD).as_deref(), Some(merged));
    // The remote is the new baseline, so the file is exactly my edit on top of it.
    assert_eq!(
        f.ws().read_base(PRD).unwrap().unwrap(),
        b"one\ntwo\nthree\nfour\nFIVE\n"
    );
    let lock = f.ws().load_lock().unwrap();
    assert!(lock.pending.is_empty() && lock.marked.is_empty());
    let status = status::status(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(
        status.files.iter().find(|x| x.path == PRD).unwrap().state,
        "local-only"
    );

    push::push(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(f.api.file(PRD).as_deref(), Some(merged));
}

#[tokio::test]
async fn only_the_lines_both_sides_changed_get_markers() {
    let f = five_line_prd().await;
    f.write(PRD, "ONE\ntwo\nthree\nLOCAL\nfive\n");
    f.api
        .external_change(PRD, Some("one\ntwo\nthree\nREMOTE\nfive\n"));

    let report = f.pull().await.unwrap();
    assert!(report.merged.is_empty() && !report.lock_advanced);
    assert_eq!(report.conflicts.len(), 1);
    assert!(report.conflicts[0].markers);
    // The untouched lines and my non-conflicting edit are outside the block.
    assert_eq!(
        f.read(PRD).as_deref(),
        Some(
            "ONE\ntwo\nthree\n<<<<<<< local\nLOCAL\n||||||| base\nfour\n=======\nREMOTE\n>>>>>>> remote\nfive\n"
        )
    );
    // The baseline is untouched until the conflict is resolved.
    assert_eq!(f.ws().read_base(PRD).unwrap().unwrap(), FIVE.as_bytes());
    let lock = f.ws().load_lock().unwrap();
    assert_eq!(
        lock.marked[PRD],
        git_blob_sha(b"one\ntwo\nthree\nREMOTE\nfive\n")
    );
}

#[tokio::test]
async fn resolving_a_partial_conflict_keeps_the_merged_lines_and_pushes_them() {
    let f = five_line_prd().await;
    f.write(PRD, "ONE\ntwo\nthree\nLOCAL\nfive\n");
    f.api
        .external_change(PRD, Some("one\ntwo\nthree\nREMOTE\nfive\n"));
    f.pull().await.unwrap();

    // Like "Accept Both" in the editor: both lines kept, every marker removed.
    f.write(PRD, "ONE\ntwo\nthree\nLOCAL\nREMOTE\nfive\n");
    let report = f.pull().await.unwrap();
    assert_eq!(report.resolved, vec![PRD.to_string()]);
    assert!(report.lock_advanced);

    push::push(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(
        f.api.file(PRD).as_deref(),
        Some("ONE\ntwo\nthree\nLOCAL\nREMOTE\nfive\n")
    );
}

#[tokio::test]
async fn merging_one_file_does_not_disturb_other_pending_conflicts() {
    let f = five_line_prd().await;
    f.write(PRD, "ONE\ntwo\nthree\nfour\nfive\n");
    f.api
        .external_change(PRD, Some("one\ntwo\nthree\nfour\nFIVE\n")); // clean merge
    f.write(README, "# local readme\n");
    f.api.external_change(README, Some("# remote readme\n")); // conflict

    let report = f.pull().await.unwrap();
    assert_eq!(report.merged, vec![PRD.to_string()]);
    assert_eq!(report.conflicts.len(), 1);
    assert!(!report.lock_advanced);
    assert_eq!(
        f.read(PRD).as_deref(),
        Some("ONE\ntwo\nthree\nfour\nFIVE\n")
    );
    let lock = f.ws().load_lock().unwrap();
    assert_eq!(lock.pending, vec![README.to_string()]);
    assert_eq!(lock.marked.keys().collect::<Vec<_>>(), vec![README]);
    // The merged file already sits on the new baseline even though the pull did not finish.
    assert_eq!(
        f.ws().load_manifest().unwrap().files[PRD].sha,
        git_blob_sha(b"one\ntwo\nthree\nfour\nFIVE\n")
    );
}

#[tokio::test]
async fn an_identical_untracked_local_file_is_adopted_not_flagged() {
    let f = Fixture::new().await;
    f.write(PRD, "# PRD\n\nPayments.\n");
    let report = f.pull().await.unwrap();
    assert!(report.conflicts.is_empty());
    assert_eq!(report.adopted, vec![PRD.to_string()]);
    assert!(report.lock_advanced);
}

#[tokio::test]
async fn a_head_that_moves_during_pull_is_retried_from_a_fresh_manifest() {
    let f = Fixture::new().await;
    f.api.with(|s| s.move_head_after_sync = true);
    let report = f.pull().await.unwrap();
    // The second attempt used the moved head, so no files from two heads were mixed.
    assert_eq!(report.head_sha, f.api.head());
    assert_eq!(f.read("llms.txt").as_deref(), Some("# llms (moved)\n"));
    assert_eq!(f.api.count("GET /v1/projects/proj_payment/sync"), 2);
}

#[tokio::test]
async fn an_interrupted_pull_leaves_no_partial_state() {
    let f = Fixture::new().await;
    f.api.with(|s| s.fail_path = Some(PRD.into()));
    let err = f.pull().await.unwrap_err();
    assert_eq!(err.exit_code(), exit::UNAVAILABLE);
    assert!(
        !f.path("llms.txt").exists(),
        "nothing is applied until every blob arrived"
    );
    assert!(f.ws().load_lock().unwrap().head_sha.is_none());
    assert!(f.ws().load_manifest().unwrap().files.is_empty());

    f.api.with(|s| s.fail_path = None);
    assert!(f.pull().await.unwrap().lock_advanced);
}

#[tokio::test]
async fn rate_limited_reads_are_retried() {
    let f = Fixture::new().await;
    f.api.with(|s| s.rate_limit_docs_once = true);
    assert!(f.pull().await.unwrap().lock_advanced);
}

#[tokio::test]
async fn hostile_remote_paths_never_touch_the_disk() {
    let f = Fixture::new().await;
    f.api
        .with(|s| s.inject_manifest_path = Some(("../evil.md".into(), "pwned".into())));
    let err = f.pull().await.unwrap_err();
    assert!(err.to_string().contains("non-canonical"));
    assert!(!f.workspace_dir.path().join("evil.md").exists());
    assert!(!f.path("llms.txt").exists());
}

#[tokio::test]
async fn paths_from_the_previous_structure_are_refused_from_the_remote() {
    for old in [
        "epics/epic-payment/product-brief.md",
        "epics/epic-payment/features/create-payment/README.md",
    ] {
        let f = Fixture::new().await;
        f.api
            .with(|s| s.inject_manifest_path = Some((old.into(), "old".into())));
        let err = f.pull().await.unwrap_err();
        assert!(err.to_string().contains("non-canonical"), "{old}");
        assert!(!f.path("llms.txt").exists(), "{old}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn symlinks_are_never_followed_or_sent() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.md"), "secret").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.md"),
        f.path("epics/epic-payment/stories/st-001-create-payment/DESIGN.md"),
    )
    .unwrap();

    assert!(push::push(&f.device.env, &f.ws()).await.is_err());
    assert!(f.pull().await.is_err());
    assert_eq!(f.api.count("POST /v1/projects/proj_payment/changes"), 0);
    // A symlinked parent directory is refused as a write target too.
    std::fs::remove_file(f.path("epics/epic-payment/stories/st-001-create-payment/DESIGN.md"))
        .unwrap();
    std::fs::remove_dir_all(f.path("epics/epic-payment")).unwrap();
    std::os::unix::fs::symlink(outside.path(), f.path("epics/epic-payment")).unwrap();
    f.api.external_change(PRD, Some("# changed\n"));
    assert!(f.pull().await.is_err());
    assert!(!outside.path().join("prd.md").exists());
}

#[tokio::test]
async fn status_reports_each_category_and_diff_works_with_and_without_network() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.write(README, "# mine\n"); // local-only
    std::fs::remove_file(f.path("llms.txt")).unwrap(); // deleted
    f.api
        .external_change("epics/epic-payment/architecture.md", Some("# arch v2\n")); // remote-only
    f.write(PRD, "# local\n");
    f.api.external_change(PRD, Some("# remote\n")); // conflict

    let report = status::status(&f.device.env, &f.ws()).await.unwrap();
    let state = |p: &str| report.files.iter().find(|x| x.path == p).unwrap().state;
    assert_eq!(state(README), "local-only");
    assert_eq!(state("llms.txt"), "deleted");
    assert_eq!(state("epics/epic-payment/architecture.md"), "remote-only");
    assert_eq!(state(PRD), "conflict");
    assert_eq!(
        state("epics/epic-payment/briefs/br-001-overview.md"),
        "unchanged"
    );
    assert!(!report.up_to_date);
    assert_eq!(report.exit_code_for_test(), exit::CONFLICT);

    let remote = status::diff_remote(&f.device.env, &f.ws(), PRD)
        .await
        .unwrap();
    assert!(
        !remote.identical && remote.diff.contains("-# remote") && remote.diff.contains("+# local")
    );

    let before = f.api.requests();
    let base = status::diff_base(&f.ws(), PRD).unwrap();
    assert_eq!(
        f.api.requests(),
        before,
        "diff --base must not use the network"
    );
    assert!(base.diff.contains("-# PRD") && base.diff.contains("+# local"));
}

#[tokio::test]
async fn update_applies_one_file_only_when_local_is_unchanged() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.api.external_change(PRD, Some("# PRD v2\n"));
    f.api.external_change(README, Some("# readme v2\n"));

    let done = speq::sync::pull::update(&f.device.env, &f.ws(), PRD)
        .await
        .unwrap();
    assert_eq!(done.result, "updated");
    assert_eq!(f.read(PRD).as_deref(), Some("# PRD v2\n"));
    assert_eq!(
        f.read(README).as_deref(),
        Some("# Create payment\n"),
        "other files are untouched"
    );
    assert_eq!(
        speq::sync::pull::update(&f.device.env, &f.ws(), PRD)
            .await
            .unwrap()
            .result,
        "up-to-date"
    );

    f.write(README, "# my edit\n");
    let err = speq::sync::pull::update(&f.device.env, &f.ws(), README)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Conflict(_)));
    assert_eq!(f.read(README).as_deref(), Some("# my edit\n"));
    assert!(
        speq::sync::pull::update(&f.device.env, &f.ws(), "../x.md")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn push_sends_one_batch_and_moves_baseline_and_lock() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.write(README, "# readme edited\n"); // update
    f.write(NEW_DOC, "# frontend\n"); // create
    std::fs::remove_file(
        f.path("epics/epic-payment/stories/st-001-create-payment/tasks/be-001-api.md"),
    )
    .unwrap(); // delete

    let before_commits = f.api.commits();
    let report = push::push(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(report.changes.len(), 3);
    assert_eq!(f.api.commits() - before_commits, 1, "one batch, one commit");
    assert_eq!(f.api.file(README).as_deref(), Some("# readme edited\n"));
    assert_eq!(f.api.file(NEW_DOC).as_deref(), Some("# frontend\n"));
    assert!(
        f.api
            .file("epics/epic-payment/stories/st-001-create-payment/tasks/be-001-api.md")
            .is_none()
    );

    assert_eq!(
        f.ws().load_lock().unwrap().head_sha.as_deref(),
        Some(f.api.head().as_str())
    );
    let status = status::status(&f.device.env, &f.ws()).await.unwrap();
    assert!(
        status.files.iter().all(|x| x.state == "unchanged"),
        "{:?}",
        status.files
    );
    assert!(status.up_to_date);
    assert!(
        push::push(&f.device.env, &f.ws())
            .await
            .unwrap()
            .nothing_to_push
    );
}

#[tokio::test]
async fn a_rejected_push_keeps_lock_and_files_and_is_not_retried() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    let lock = f.ws().load_lock().unwrap();
    f.write(README, "# mine\n");
    f.api.external_change(PRD, Some("# someone else\n")); // the head moves

    let err = push::push(&f.device.env, &f.ws()).await.unwrap_err();
    assert!(matches!(err, Error::Conflict(_)));
    assert!(err.to_string().contains("speq pull"));
    assert_eq!(
        f.api.count("POST /v1/projects/proj_payment/changes"),
        1,
        "no automatic retry"
    );
    assert_eq!(f.ws().load_lock().unwrap(), lock);
    assert_eq!(f.read(README).as_deref(), Some("# mine\n"));

    // After pulling, the same edit pushes cleanly.
    f.pull().await.unwrap();
    push::push(&f.device.env, &f.ws()).await.unwrap();
}

#[tokio::test]
async fn repeating_an_interrupted_push_finalises_it_without_a_second_commit() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.write(README, "# mine\n");
    f.api.with(|s| s.fail_after_commit_once = true);
    let before = f.api.commits();

    let err = push::push(&f.device.env, &f.ws()).await.unwrap_err();
    assert_eq!(err.exit_code(), exit::UNAVAILABLE);
    // The lock did not move, so the retry carries the same base and the same Idempotency-Key.
    let report = push::push(&f.device.env, &f.ws()).await.unwrap();
    assert!(report.replayed);
    assert_eq!(f.api.commits() - before, 1);
    assert_eq!(
        f.ws().load_lock().unwrap().head_sha.as_deref(),
        Some(f.api.head().as_str())
    );
}

#[tokio::test]
async fn push_rejects_unsendable_content_before_any_request() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    std::fs::write(f.path(README), [0xff, 0xfe, 0x00]).unwrap();
    assert_eq!(
        push::push(&f.device.env, &f.ws())
            .await
            .unwrap_err()
            .exit_code(),
        exit::INVALID
    );
    f.write(README, &"x".repeat(1024 * 1024 + 1));
    assert!(push::push(&f.device.env, &f.ws()).await.is_err());
    f.write(README, "# ok\n");
    f.write(
        "epics/epic-payment/notes.md",
        "ignored: not a canonical path\n",
    );
    let report = push::push(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(
        report.changes.len(),
        1,
        "non-canonical files are never sent"
    );
    assert_eq!(f.api.count("POST /v1/projects/proj_payment/changes"), 1);
}

#[tokio::test]
async fn a_revoked_role_is_caught_by_the_api_even_while_the_snapshot_is_valid() {
    let f = Fixture::new().await;
    // The snapshot still lists the project, but the API no longer knows this membership.
    f.api.with(|s| {
        s.members
            .get_mut("user_a")
            .unwrap()
            .retain(|p| p != PROJECT)
    });
    assert_eq!(
        speq::init::list(&f.device.env)
            .await
            .unwrap()
            .projects
            .len(),
        3
    );
    let err = f.pull().await.unwrap_err();
    assert_eq!(err.exit_code(), exit::INVALID); // generic 404 for a project the user may not read
    assert_eq!(
        f.api.count("POST"),
        f.api.count("POST /v1/auth"),
        "no mutation was attempted"
    );
}

#[tokio::test]
async fn state_is_separated_per_workspace() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    let other = tempfile::tempdir().unwrap();
    let options = speq::init::InitOptions {
        repo: "acme/cart-specs",
        local_dir: "docs/specs",
        reconfigure: false,
    };
    speq::init::init(
        &f.device.env,
        &std::fs::canonicalize(other.path()).unwrap(),
        options,
    )
    .await
    .unwrap();
    let ws = speq::workspace::Workspace::discover(other.path()).unwrap();
    assert!(ws.load_lock().unwrap().head_sha.is_none());
    assert!(ws.load_manifest().unwrap().files.is_empty());
    assert_eq!(ws.config.spec_source.local_dir, "docs/specs");
    assert_ne!(
        f.device.env.dirs.workspace_lock(&f.ws().root),
        f.device.env.dirs.workspace_lock(&ws.root)
    );
}

#[tokio::test]
async fn a_second_command_in_the_same_workspace_fails_fast() {
    let f = Fixture::new().await;
    let _held = f.ws().guard(&f.device.env, false).unwrap();
    let err = f.pull().await.unwrap_err();
    assert!(matches!(err, Error::Conflict(_)));
}

trait ExitForTest {
    fn exit_code_for_test(&self) -> u8;
}

impl ExitForTest for status::StatusReport {
    fn exit_code_for_test(&self) -> u8 {
        use speq::output::Human;
        self.exit_code()
    }
}

const LAYOUT: &str = "epics/epic-payment/stories/st-001-create-payment/layout.html";
const COMPONENT: &str = "design/components/button.html";
const PAGE: &str = "<!doctype html>\n<html>\n<body>\n<h1>Pay</h1>\n<button class=\"a\">Go</button>\n<p>footer</p>\n</body>\n</html>\n";

#[tokio::test]
async fn html_documents_are_pulled_hashed_diffed_and_pushed_like_markdown() {
    let f = Fixture::new().await;
    f.api.external_change(LAYOUT, Some(PAGE));
    f.api
        .external_change(COMPONENT, Some("<button>ok</button>\n"));
    let report = f.pull().await.unwrap();
    assert!(
        report.added.contains(&LAYOUT.to_string()) && report.added.contains(&COMPONENT.to_string())
    );
    assert_eq!(f.read(LAYOUT).as_deref(), Some(PAGE));
    assert_eq!(
        f.ws().read_base(COMPONENT).unwrap().unwrap(),
        b"<button>ok</button>\n"
    );
    assert_eq!(
        f.ws().load_manifest().unwrap().files[LAYOUT].sha,
        git_blob_sha(PAGE.as_bytes())
    );

    // Untouched: nothing to send. Edited locally: shows in status and diff, then pushes.
    assert!(
        push::push(&f.device.env, &f.ws())
            .await
            .unwrap()
            .changes
            .is_empty()
    );
    let edited = PAGE.replace("<h1>Pay</h1>", "<h1>Pay now</h1>");
    f.write(LAYOUT, &edited);
    f.write(
        "design/components/card.html",
        "<div class=\"card\"></div>\n",
    ); // create
    std::fs::remove_file(f.path(COMPONENT)).unwrap(); // delete

    let status = status::status(&f.device.env, &f.ws()).await.unwrap();
    let state = |p: &str| status.files.iter().find(|x| x.path == p).unwrap().state;
    assert_eq!(state(LAYOUT), "local-only");
    assert_eq!(state(COMPONENT), "deleted");
    let base = status::diff_base(&f.ws(), LAYOUT).unwrap();
    assert!(base.diff.contains("-<h1>Pay</h1>") && base.diff.contains("+<h1>Pay now</h1>"));

    let pushed = push::push(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(pushed.changes.len(), 3);
    assert_eq!(f.api.file(LAYOUT).as_deref(), Some(edited.as_str()));
    assert_eq!(
        f.api.file("design/components/card.html").as_deref(),
        Some("<div class=\"card\"></div>\n")
    );
    assert!(f.api.file(COMPONENT).is_none());
}

#[tokio::test]
async fn html_documents_merge_by_line_and_conflict_with_markers_that_resolve_and_push() {
    let f = Fixture::new().await;
    f.api.external_change(LAYOUT, Some(PAGE));
    f.pull().await.unwrap();

    // Different lines on each side combine cleanly.
    f.write(LAYOUT, &PAGE.replace("<h1>Pay</h1>", "<h1>Pay now</h1>"));
    f.api
        .external_change(LAYOUT, Some(&PAGE.replace("footer", "legal")));
    let report = f.pull().await.unwrap();
    assert_eq!(report.merged, vec![LAYOUT.to_string()]);
    let merged = PAGE
        .replace("<h1>Pay</h1>", "<h1>Pay now</h1>")
        .replace("footer", "legal");
    assert_eq!(f.read(LAYOUT).as_deref(), Some(merged.as_str()));
    push::push(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(f.api.file(LAYOUT).as_deref(), Some(merged.as_str()));

    // The same line on both sides conflicts; the markers sit on their own lines inside the HTML.
    f.write(LAYOUT, &merged.replace("class=\"a\"", "class=\"mine\""));
    f.api.external_change(
        LAYOUT,
        Some(&merged.replace("class=\"a\"", "class=\"theirs\"")),
    );
    let report = f.pull().await.unwrap();
    assert_eq!(report.conflicts.len(), 1);
    assert!(report.conflicts[0].markers && !report.lock_advanced);
    let marked = f.read(LAYOUT).unwrap();
    assert!(marked.contains("<<<<<<< local\n<button class=\"mine\">Go</button>\n||||||| base\n"));
    assert!(marked.contains("=======\n<button class=\"theirs\">Go</button>\n>>>>>>> remote\n"));
    assert!(push::push(&f.device.env, &f.ws()).await.is_err());

    // Editing the markers away resolves it, and the resolution can be pushed.
    let resolved = merged.replace("class=\"a\"", "class=\"mine theirs\"");
    f.write(LAYOUT, &resolved);
    let report = f.pull().await.unwrap();
    assert_eq!(report.resolved, vec![LAYOUT.to_string()]);
    assert!(report.lock_advanced);
    push::push(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(f.api.file(LAYOUT).as_deref(), Some(resolved.as_str()));
}

#[tokio::test]
async fn html_conflicts_can_be_taken_from_either_side_with_resolve() {
    use speq::sync::resolve::{Choice, Target, resolve};

    let f = Fixture::new().await;
    f.api.external_change(LAYOUT, Some(PAGE));
    f.pull().await.unwrap();
    f.write(LAYOUT, &PAGE.replace("Go", "Mine"));
    f.api
        .external_change(LAYOUT, Some(&PAGE.replace("Go", "Theirs")));
    f.pull().await.unwrap();

    let only = [LAYOUT.to_string()];
    resolve(&f.device.env, &f.ws(), Target::Paths(&only), Choice::Ours).unwrap();
    assert_eq!(
        f.read(LAYOUT).as_deref(),
        Some(PAGE.replace("Go", "Mine").as_str())
    );
}
