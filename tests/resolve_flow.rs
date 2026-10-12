mod support;

use speq::error::{Error, exit};
use speq::hashing::git_blob_sha;
use speq::sync::pull::PullReport;
use speq::sync::resolve::{Choice, ResolveReport, Target, resolve};
use speq::sync::{push, status};
use speq::workspace::Side;
use support::Fixture;

const PRD: &str = "epics/epic-payment/prd.md";
const README: &str = "epics/epic-payment/stories/st-001-create-payment/README.md";
const BRIEF: &str = "epics/epic-payment/briefs/br-001-overview.md";
const NEW_DOC: &str = "epics/epic-payment/stories/st-001-create-payment/tasks/fe-001-form.md";

const FIVE: &str = "one\ntwo\nthree\nfour\nfive\n";
const PRD_LOCAL: &str = "ONE\ntwo\nthree\nLOCAL\nfive\n";
const PRD_REMOTE: &str = "one\ntwo\nthree\nREMOTE\nfive\n";
const PRD_MARKED: &str = "ONE\ntwo\nthree\n<<<<<<< local\nLOCAL\n||||||| base\nfour\n=======\nREMOTE\n>>>>>>> remote\nfive\n";

fn run(f: &Fixture, target: Target<'_>, choice: Choice) -> Result<ResolveReport, Error> {
    resolve(&f.device.env, &f.ws(), target, choice)
}

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|p| p.to_string()).collect()
}

/// `PRD` changed on both sides at the same line, with a non-conflicting edit of mine nearby.
async fn marked_prd() -> (Fixture, PullReport) {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.api.external_change(PRD, Some(FIVE));
    f.pull().await.unwrap();
    f.write(PRD, PRD_LOCAL);
    f.api.external_change(PRD, Some(PRD_REMOTE));
    let report = f.pull().await.unwrap();
    assert_eq!(f.read(PRD).as_deref(), Some(PRD_MARKED));
    (f, report)
}

#[tokio::test]
async fn a_conflicting_pull_saves_both_versions_for_resolve() {
    let (f, report) = marked_prd().await;
    assert_eq!(report.conflicts.len(), 1);
    let ws = f.ws();
    assert_eq!(
        ws.read_conflict(Side::Ours, PRD).unwrap().unwrap(),
        PRD_LOCAL.as_bytes()
    );
    assert_eq!(
        ws.read_conflict(Side::Theirs, PRD).unwrap().unwrap(),
        PRD_REMOTE.as_bytes()
    );
    let lock = ws.load_lock().unwrap();
    assert_eq!(lock.pending_head.as_deref(), Some(f.api.head().as_str()));
}

#[tokio::test]
async fn resolving_an_edited_file_keeps_it_as_my_change_and_opens_push() {
    let (f, _) = marked_prd().await;
    f.write(PRD, "ONE\ntwo\nthree\nLOCAL\nREMOTE\nfive\n");

    let report = run(&f, Target::Paths(&paths(&[PRD])), Choice::Edited).unwrap();
    assert_eq!(report.resolved, vec![PRD.to_string()]);
    assert!(report.pending.is_empty() && report.lock_advanced);

    let lock = f.ws().load_lock().unwrap();
    assert_eq!(lock.head_sha.as_deref(), Some(f.api.head().as_str()));
    assert!(lock.pending.is_empty() && lock.marked.is_empty() && lock.pending_head.is_none());
    assert_eq!(
        f.ws().read_base(PRD).unwrap().unwrap(),
        PRD_REMOTE.as_bytes()
    );
    assert!(f.ws().read_conflict(Side::Ours, PRD).unwrap().is_none());
    assert!(f.ws().read_conflict(Side::Theirs, PRD).unwrap().is_none());

    let state = status::status(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(
        state.files.iter().find(|x| x.path == PRD).unwrap().state,
        "local-only"
    );
    push::push(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(
        f.api.file(PRD).as_deref(),
        Some("ONE\ntwo\nthree\nLOCAL\nREMOTE\nfive\n")
    );
}

#[tokio::test]
async fn a_file_that_still_has_markers_is_left_alone() {
    let (f, _) = marked_prd().await;
    let lock = f.ws().load_lock().unwrap();

    let report = run(&f, Target::Paths(&paths(&[PRD])), Choice::Edited).unwrap();
    assert!(report.resolved.is_empty());
    assert_eq!(report.still_marked, vec![PRD.to_string()]);
    assert_eq!(report.pending, vec![PRD.to_string()]);
    assert!(!report.lock_advanced);
    assert_eq!(f.read(PRD).as_deref(), Some(PRD_MARKED));
    assert_eq!(f.ws().load_lock().unwrap(), lock);
    assert_eq!(
        f.ws().read_base(PRD).unwrap().unwrap(),
        FIVE.as_bytes(),
        "the baseline does not move until the file is resolved"
    );
}

#[tokio::test]
async fn ours_restores_my_version_from_before_the_merge() {
    let (f, _) = marked_prd().await;

    let report = run(&f, Target::Paths(&paths(&[PRD])), Choice::Ours).unwrap();
    assert_eq!(report.resolved, vec![PRD.to_string()]);
    assert_eq!(f.read(PRD).as_deref(), Some(PRD_LOCAL));
    push::push(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(f.api.file(PRD).as_deref(), Some(PRD_LOCAL));
}

#[tokio::test]
async fn theirs_replaces_the_file_with_the_remote_and_leaves_nothing_to_push() {
    let (f, _) = marked_prd().await;

    let report = run(&f, Target::Paths(&paths(&[PRD])), Choice::Theirs).unwrap();
    assert_eq!(report.resolved, vec![PRD.to_string()]);
    assert_eq!(f.read(PRD).as_deref(), Some(PRD_REMOTE));
    let pushed = push::push(&f.device.env, &f.ws()).await.unwrap();
    assert!(pushed.nothing_to_push);
}

/// Three conflicts of different kinds: both-sided (markers), remote delete over my edit, and
/// my delete over a remote edit.
async fn mixed_conflicts() -> Fixture {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.write(PRD, "# local PRD\n");
    f.api.external_change(PRD, Some("# remote PRD\n"));
    f.write(BRIEF, "# my brief\n");
    f.api.external_change(BRIEF, None);
    std::fs::remove_file(f.path(README)).unwrap();
    f.api.external_change(README, Some("# remote readme\n"));
    let report = f.pull().await.unwrap();
    assert_eq!(report.conflicts.len(), 3);
    f
}

#[tokio::test]
async fn deleting_conflicts_keep_both_versions_too() {
    let f = mixed_conflicts().await;
    let ws = f.ws();
    // Remote deleted my edited file: only mine exists. I deleted a file the remote edited: only theirs.
    assert_eq!(
        ws.read_conflict(Side::Ours, BRIEF).unwrap().unwrap(),
        b"# my brief\n"
    );
    assert!(ws.read_conflict(Side::Theirs, BRIEF).unwrap().is_none());
    assert!(ws.read_conflict(Side::Ours, README).unwrap().is_none());
    assert_eq!(
        ws.read_conflict(Side::Theirs, README).unwrap().unwrap(),
        b"# remote readme\n"
    );
}

#[tokio::test]
async fn keeping_my_file_after_a_remote_delete_pushes_it_as_a_new_document() {
    let f = mixed_conflicts().await;
    run(&f, Target::Paths(&paths(&[BRIEF])), Choice::Edited).unwrap();
    assert_eq!(f.read(BRIEF).as_deref(), Some("# my brief\n"));
    assert!(!f.ws().load_manifest().unwrap().files.contains_key(BRIEF));
    assert!(f.ws().read_base(BRIEF).unwrap().is_none());
}

#[tokio::test]
async fn theirs_accepts_a_remote_delete_by_removing_my_file() {
    let f = mixed_conflicts().await;
    run(&f, Target::Paths(&paths(&[BRIEF])), Choice::Theirs).unwrap();
    assert!(f.read(BRIEF).is_none());
    assert!(!f.ws().load_manifest().unwrap().files.contains_key(BRIEF));
}

#[tokio::test]
async fn keeping_my_delete_over_a_remote_edit_pushes_the_delete() {
    let f = mixed_conflicts().await;
    run(&f, Target::Paths(&paths(&[README])), Choice::Edited).unwrap();
    assert!(f.read(README).is_none());
    assert_eq!(
        f.ws().load_manifest().unwrap().files[README].sha,
        git_blob_sha(b"# remote readme\n"),
        "the remote version is the baseline, so the missing file is my delete"
    );
}

#[tokio::test]
async fn theirs_brings_back_a_file_i_deleted() {
    let f = mixed_conflicts().await;
    run(&f, Target::Paths(&paths(&[README])), Choice::Theirs).unwrap();
    assert_eq!(f.read(README).as_deref(), Some("# remote readme\n"));
}

#[tokio::test]
async fn all_takes_the_edited_files_and_leaves_delete_conflicts_to_me() {
    let f = mixed_conflicts().await;
    f.write(PRD, "# merged PRD\n"); // markers removed

    let report = run(&f, Target::All, Choice::Edited).unwrap();
    assert_eq!(report.resolved, vec![PRD.to_string()]);
    assert_eq!(report.undecided.len(), 2);
    assert!(report.undecided.contains(&BRIEF.to_string()));
    assert!(report.undecided.contains(&README.to_string()));
    assert_eq!(report.pending.len(), 2);
    assert!(!report.lock_advanced);
    // Neither delete conflict was decided: both files are exactly as they were.
    assert_eq!(f.read(BRIEF).as_deref(), Some("# my brief\n"));
    assert!(f.read(README).is_none());
    let lock = f.ws().load_lock().unwrap();
    assert_eq!(lock.pending.len(), 2);
    assert!(f.ws().read_conflict(Side::Ours, BRIEF).unwrap().is_some());
    assert!(f.ws().load_manifest().unwrap().files.contains_key(BRIEF));

    // Naming them is the explicit decision; the last one opens push.
    let done = run(&f, Target::Paths(&paths(&[BRIEF, README])), Choice::Edited).unwrap();
    assert_eq!(done.resolved.len(), 2);
    assert!(done.undecided.is_empty() && done.pending.is_empty() && done.lock_advanced);
}

#[tokio::test]
async fn all_skips_files_with_markers_and_keeps_the_lock_where_it_is() {
    let f = mixed_conflicts().await; // PRD still has markers
    let before = f.ws().load_lock().unwrap();

    let report = run(&f, Target::All, Choice::Edited).unwrap();
    assert!(report.resolved.is_empty());
    assert_eq!(report.still_marked, vec![PRD.to_string()]);
    assert_eq!(report.undecided.len(), 2);
    assert_eq!(report.pending.len(), 3);
    assert!(!report.lock_advanced);
    assert_eq!(f.ws().load_lock().unwrap(), before);
    // Push is still blocked, and the file with markers is untouched.
    let err = push::push(&f.device.env, &f.ws()).await.unwrap_err();
    assert_eq!(err.exit_code(), exit::CONFLICT);
    assert!(f.read(PRD).unwrap().contains("<<<<<<< local"));
}

#[tokio::test]
async fn all_with_a_side_decides_every_conflict_including_deletes() {
    let f = mixed_conflicts().await;

    let report = run(&f, Target::All, Choice::Theirs).unwrap();
    assert_eq!(report.resolved.len(), 3);
    assert!(report.undecided.is_empty() && report.pending.is_empty() && report.lock_advanced);
    assert_eq!(f.read(PRD).as_deref(), Some("# remote PRD\n"));
    assert!(f.read(BRIEF).is_none(), "the remote delete is accepted");
    assert_eq!(f.read(README).as_deref(), Some("# remote readme\n"));
    let lock = f.ws().load_lock().unwrap();
    assert_eq!(lock.head_sha.as_deref(), Some(f.api.head().as_str()));
}

#[tokio::test]
async fn a_remote_delete_after_markers_turns_it_into_a_decision_all_does_not_make() {
    let (f, _) = marked_prd().await;
    f.api.external_change(PRD, None); // the remote deletes the file I am still resolving

    let report = f.pull().await.unwrap();
    assert_eq!(report.conflicts.len(), 1);
    assert!(!report.conflicts[0].markers, "a delete has nothing to mark");
    let lock = f.ws().load_lock().unwrap();
    assert!(lock.marked.is_empty(), "no longer a marker conflict");
    assert_eq!(
        f.read(PRD).as_deref(),
        Some(PRD_MARKED),
        "the file is untouched"
    );
    // My version from before the merge is still the one kept, not the marker text.
    assert_eq!(
        f.ws().read_conflict(Side::Ours, PRD).unwrap().unwrap(),
        PRD_LOCAL.as_bytes()
    );
    assert!(f.ws().read_conflict(Side::Theirs, PRD).unwrap().is_none());

    // `--all` neither resolves it nor fails: it is for me to decide.
    f.write(PRD, "# merged\n");
    let all = run(&f, Target::All, Choice::Edited).unwrap();
    assert!(all.resolved.is_empty());
    assert_eq!(all.undecided, vec![PRD.to_string()]);
    assert_eq!(f.read(PRD).as_deref(), Some("# merged\n"));

    // Naming it keeps my file as a new document on top of the remote delete.
    let done = run(&f, Target::Paths(&paths(&[PRD])), Choice::Edited).unwrap();
    assert_eq!(done.resolved, vec![PRD.to_string()]);
    assert!(done.lock_advanced);
    assert!(!f.ws().load_manifest().unwrap().files.contains_key(PRD));
}

#[tokio::test]
async fn after_a_remote_delete_ours_still_restores_the_version_from_before_the_merge() {
    let (f, _) = marked_prd().await;
    f.api.external_change(PRD, None);
    f.pull().await.unwrap();

    // Plain resolve refuses while the markers are in the file.
    let report = run(&f, Target::Paths(&paths(&[PRD])), Choice::Edited).unwrap();
    assert_eq!(report.still_marked, vec![PRD.to_string()]);

    run(&f, Target::Paths(&paths(&[PRD])), Choice::Ours).unwrap();
    assert_eq!(f.read(PRD).as_deref(), Some(PRD_LOCAL));
}

#[tokio::test]
async fn a_separator_left_behind_keeps_the_file_unresolved() {
    let (f, _) = marked_prd().await;
    // The three labelled lines were removed, the `=======` between the two versions was not.
    let stray = "ONE\ntwo\nthree\nLOCAL\n=======\nREMOTE\nfive\n";
    f.write(PRD, stray);
    let lock = f.ws().load_lock().unwrap();

    let report = run(&f, Target::All, Choice::Edited).unwrap();
    assert!(report.resolved.is_empty());
    assert_eq!(report.still_marked, vec![PRD.to_string()]);
    // `pull` agrees: it neither accepts the file nor merges over it.
    let pulled = f.pull().await.unwrap();
    assert_eq!(pulled.conflicts.len(), 1);
    assert!(pulled.resolved.is_empty());
    assert_eq!(f.read(PRD).as_deref(), Some(stray));
    assert_eq!(f.ws().load_lock().unwrap(), lock);
    let err = push::push(&f.device.env, &f.ws()).await.unwrap_err();
    assert_eq!(err.exit_code(), exit::CONFLICT);

    f.write(PRD, "ONE\ntwo\nthree\nLOCAL\nREMOTE\nfive\n");
    let done = run(&f, Target::All, Choice::Edited).unwrap();
    assert_eq!(done.resolved, vec![PRD.to_string()]);
}

#[tokio::test]
async fn a_markdown_underline_the_document_always_had_is_not_a_leftover() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.api
        .external_change(PRD, Some("Title\n=======\none\ntwo\nthree\nfour\nfive\n"));
    f.pull().await.unwrap();
    f.write(PRD, "Title\n=======\nONE\ntwo\nthree\nLOCAL\nfive\n");
    f.api
        .external_change(PRD, Some("Title\n=======\none\ntwo\nthree\nREMOTE\nfive\n"));
    f.pull().await.unwrap();
    assert!(f.read(PRD).unwrap().starts_with("Title\n=======\nONE\n"));

    f.write(
        PRD,
        "Title\n=======\nONE\ntwo\nthree\nLOCAL\nREMOTE\nfive\n",
    );
    let report = run(&f, Target::All, Choice::Edited).unwrap();
    assert_eq!(report.resolved, vec![PRD.to_string()]);
    push::push(&f.device.env, &f.ws()).await.unwrap();
    assert!(f.api.file(PRD).unwrap().starts_with("Title\n=======\n"));
}

#[tokio::test]
async fn deleting_a_marked_file_keeps_my_version_from_before_the_merge() {
    let (f, _) = marked_prd().await;
    std::fs::remove_file(f.path(PRD)).unwrap();

    // The version survives this pull and the next one.
    for _ in 0..2 {
        let report = f.pull().await.unwrap();
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(
            f.ws().read_conflict(Side::Ours, PRD).unwrap().unwrap(),
            PRD_LOCAL.as_bytes()
        );
    }
    // Keeping the delete is still the plain choice...
    let kept = run(&f, Target::Paths(&paths(&[PRD])), Choice::Edited).unwrap();
    assert_eq!(kept.resolved, vec![PRD.to_string()]);
    assert!(f.read(PRD).is_none());
}

#[tokio::test]
async fn ours_brings_back_a_marked_file_that_was_deleted() {
    let (f, _) = marked_prd().await;
    std::fs::remove_file(f.path(PRD)).unwrap();
    f.pull().await.unwrap();

    run(&f, Target::Paths(&paths(&[PRD])), Choice::Ours).unwrap();
    assert_eq!(f.read(PRD).as_deref(), Some(PRD_LOCAL));
}

#[tokio::test]
async fn a_remote_that_moved_after_the_markers_makes_resolve_ask_for_a_pull() {
    let (f, _) = marked_prd().await;
    let before = f.ws().load_lock().unwrap();
    f.api
        .external_change(PRD, Some("one\ntwo\nthree\nREMOTE3\nfive\n"));
    f.pull().await.unwrap(); // the markers stay, written against the older remote
    assert!(f.ws().load_lock().unwrap().pending_head.is_none());

    let report = run(&f, Target::All, Choice::Theirs).unwrap();
    assert_eq!(report.resolved, vec![PRD.to_string()]);
    assert!(
        !report.lock_advanced,
        "the saved remote copy is out of date"
    );
    let lock = f.ws().load_lock().unwrap();
    assert_eq!(lock.head_sha, before.head_sha);
    let state = status::status(&f.device.env, &f.ws()).await.unwrap();
    assert!(!state.up_to_date);
    assert_eq!(
        state.files.iter().find(|x| x.path == PRD).unwrap().state,
        "remote-only"
    );

    // One pull brings the file and the lock up to the real remote.
    let pulled = f.pull().await.unwrap();
    assert_eq!(pulled.updated, vec![PRD.to_string()]);
    assert!(pulled.lock_advanced);
    assert_eq!(
        f.read(PRD).as_deref(),
        Some("one\ntwo\nthree\nREMOTE3\nfive\n")
    );
    assert_eq!(
        f.ws().load_lock().unwrap().head_sha.as_deref(),
        Some(f.api.head().as_str())
    );
}

#[tokio::test]
async fn all_with_nothing_pending_changes_nothing() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    let lock = f.ws().load_lock().unwrap();
    let report = run(&f, Target::All, Choice::Edited).unwrap();
    assert!(report.resolved.is_empty() && report.pending.is_empty());
    assert_eq!(f.ws().load_lock().unwrap(), lock);
}

#[tokio::test]
async fn a_path_without_a_pending_conflict_is_refused_and_nothing_is_changed() {
    let f = mixed_conflicts().await;
    f.write(PRD, "# merged PRD\n");
    let lock = f.ws().load_lock().unwrap();

    let err = run(
        &f,
        Target::Paths(&paths(&[PRD, "epics/epic-payment/architecture.md"])),
        Choice::Edited,
    )
    .unwrap_err();
    assert_eq!(err.exit_code(), exit::INVALID);
    assert_eq!(f.ws().load_lock().unwrap(), lock);
    assert!(f.ws().read_conflict(Side::Theirs, PRD).unwrap().is_some());
    assert!(
        run(&f, Target::Paths(&paths(&["../x.md"])), Choice::Edited).is_err(),
        "paths outside the specs folder are refused"
    );
}

#[tokio::test]
async fn missing_saved_versions_are_reported_instead_of_guessing() {
    let (f, _) = marked_prd().await;
    f.ws().forget_conflict(PRD).unwrap();
    f.write(PRD, "# merged\n");

    let err = run(&f, Target::Paths(&paths(&[PRD])), Choice::Edited).unwrap_err();
    assert_eq!(err.exit_code(), exit::INVALID);
    assert_eq!(f.read(PRD).as_deref(), Some("# merged\n"));
    assert_eq!(f.ws().load_lock().unwrap().pending, vec![PRD.to_string()]);
}

#[tokio::test]
async fn a_pull_that_recognises_a_manual_resolution_drops_the_saved_versions() {
    let (f, _) = marked_prd().await;
    f.write(PRD, "ONE\ntwo\nthree\nLOCAL\nREMOTE\nfive\n");

    let report = f.pull().await.unwrap();
    assert_eq!(report.resolved, vec![PRD.to_string()]);
    assert!(f.ws().read_conflict(Side::Ours, PRD).unwrap().is_none());
    assert!(f.ws().read_conflict(Side::Theirs, PRD).unwrap().is_none());
    assert!(f.ws().load_lock().unwrap().pending_head.is_none());
}

#[tokio::test]
async fn a_clean_merge_leaves_no_saved_versions_behind() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.api.external_change(PRD, Some(FIVE));
    f.pull().await.unwrap();
    f.write(PRD, "ONE\ntwo\nthree\nfour\nfive\n");
    f.api
        .external_change(PRD, Some("one\ntwo\nthree\nfour\nFIVE\n"));

    let report = f.pull().await.unwrap();
    assert_eq!(report.merged, vec![PRD.to_string()]);
    assert!(f.ws().read_conflict(Side::Ours, PRD).unwrap().is_none());
    assert!(f.ws().read_conflict(Side::Theirs, PRD).unwrap().is_none());
}

#[tokio::test]
async fn add_over_an_untracked_file_resolves_like_any_other_conflict() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.write(NEW_DOC, "# my frontend doc\n");
    f.api
        .external_change(NEW_DOC, Some("# remote frontend doc\n"));
    f.pull().await.unwrap();

    run(&f, Target::Paths(&paths(&[NEW_DOC])), Choice::Ours).unwrap();
    assert_eq!(f.read(NEW_DOC).as_deref(), Some("# my frontend doc\n"));
    push::push(&f.device.env, &f.ws()).await.unwrap();
    assert_eq!(f.api.file(NEW_DOC).as_deref(), Some("# my frontend doc\n"));
}
