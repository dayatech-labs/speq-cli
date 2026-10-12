mod support;

use speq::context::context;
use speq::error::exit;
use support::{Fixture, PROJECT};

const STORY: &str = "epic-payment/st-001-create-payment";
const STORY_DIR: &str = "epics/epic-payment/stories/st-001-create-payment";

/// Documents around the default story (`st-001-create-payment`) that must never leak into its context.
fn seed_unrelated(f: &Fixture) {
    for (path, body) in [
        ("README.md", "# Root\n"),
        ("architecture.md", "# Product architecture\n"),
        ("design/DESIGN.md", "# Design system\n"),
        ("design/components/button.html", "<button>ok</button>\n"),
        (
            "epics/epic-payment/decisions/dr-01-use-postgres.md",
            "# DR\n",
        ),
        (
            "epics/epic-payment/stories/st-002-refund/README.md",
            "# Refund\n",
        ),
        (
            "epics/epic-payment/stories/st-002-refund/tasks/be-001-refund.md",
            "# Refund task\n",
        ),
        ("epics/epic-cart/prd.md", "# Cart\n"),
        ("epics/epic-cart/briefs/br-001-cart.md", "# Cart brief\n"),
        (
            "epics/epic-cart/stories/st-001-create-payment/README.md",
            "# Same story name, other epic\n",
        ),
    ] {
        f.api.external_change(path, Some(body));
    }
}

#[tokio::test]
async fn lists_existing_documents_in_the_fixed_order_without_calling_the_api() {
    let f = Fixture::new().await;
    seed_unrelated(&f);
    for (path, body) in [
        ("epics/epic-payment/briefs/br-010-late.md", "# b10\n"),
        ("epics/epic-payment/briefs/br-002-second.md", "# b2\n"),
        ("epics/epic-payment/briefs/br-001-overview.md", "# b1\n"), // replaces the seeded brief
        (&format!("{STORY_DIR}/layout.html"), "<main>layout</main>\n"),
        (&format!("{STORY_DIR}/DESIGN.md"), "# Design\n"),
        (&format!("{STORY_DIR}/acceptance-criteria.md"), "# AC\n"),
        (
            &format!("{STORY_DIR}/tasks/fe-002-validation.md"),
            "# fe2\n",
        ),
        (&format!("{STORY_DIR}/tasks/fe-001-form.md"), "# fe1\n"),
        (&format!("{STORY_DIR}/tasks/be-010-late.md"), "# be10\n"),
    ] {
        f.api.external_change(path, Some(body));
    }
    f.pull().await.unwrap();

    let before = f.api.requests();
    let report = context(&f.ws(), STORY).unwrap();
    assert_eq!(f.api.requests(), before, "context never calls the API");
    assert_eq!(
        report.paths,
        [
            "specs/llms.txt",
            "specs/README.md",
            "specs/architecture.md",
            "specs/epics/epic-payment/briefs/br-001-overview.md",
            "specs/epics/epic-payment/briefs/br-002-second.md",
            "specs/epics/epic-payment/briefs/br-010-late.md",
            "specs/epics/epic-payment/prd.md",
            "specs/epics/epic-payment/architecture.md",
            "specs/epics/epic-payment/stories/st-001-create-payment/README.md",
            "specs/epics/epic-payment/stories/st-001-create-payment/acceptance-criteria.md",
            "specs/epics/epic-payment/stories/st-001-create-payment/DESIGN.md",
            "specs/epics/epic-payment/stories/st-001-create-payment/layout.html",
            "specs/epics/epic-payment/stories/st-001-create-payment/tasks/be-001-api.md",
            "specs/epics/epic-payment/stories/st-001-create-payment/tasks/be-010-late.md",
            "specs/epics/epic-payment/stories/st-001-create-payment/tasks/fe-001-form.md",
            "specs/epics/epic-payment/stories/st-001-create-payment/tasks/fe-002-validation.md",
        ]
    );
    assert_eq!(report.head_sha, f.api.head());
    assert_eq!(report.project_id, PROJECT);
    assert_eq!(context(&f.ws(), STORY).unwrap().paths, report.paths);
}

#[tokio::test]
async fn lists_only_documents_that_exist() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    // The default project has no root README, root architecture, DESIGN, layout or acceptance criteria.
    let report = context(&f.ws(), STORY).unwrap();
    assert_eq!(
        report.paths,
        [
            "specs/llms.txt",
            "specs/epics/epic-payment/briefs/br-001-overview.md",
            "specs/epics/epic-payment/prd.md",
            "specs/epics/epic-payment/architecture.md",
            "specs/epics/epic-payment/stories/st-001-create-payment/README.md",
            "specs/epics/epic-payment/stories/st-001-create-payment/tasks/be-001-api.md",
        ]
    );
}

#[tokio::test]
async fn an_html_layout_alone_is_enough_to_have_a_story() {
    let f = Fixture::new().await;
    f.api.external_change(
        "epics/epic-payment/stories/st-003-prototype/layout.html",
        Some("<!doctype html>\n<title>x</title>\n"),
    );
    f.pull().await.unwrap();
    let report = context(&f.ws(), "epic-payment/st-003-prototype").unwrap();
    assert_eq!(
        report.paths.last().map(String::as_str),
        Some("specs/epics/epic-payment/stories/st-003-prototype/layout.html")
    );
    assert!(
        report
            .paths
            .iter()
            .all(|p| !p.contains("st-001-create-payment")),
        "{:?}",
        report.paths
    );
}

#[tokio::test]
async fn a_story_without_documents_is_an_error_even_when_the_epic_has_some() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    for missing in [
        "epic-payment/st-009-missing",
        "epic-missing/st-001-create-payment",
    ] {
        let err = context(&f.ws(), missing).unwrap_err();
        assert_eq!(err.exit_code(), exit::INVALID, "{missing}");
        assert!(err.detail().contains("no documents found"), "{err:?}");
    }
}

#[tokio::test]
async fn rejects_targets_that_are_not_epic_slash_story() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    for bad in [
        "",
        "epic-payment",
        "epic-payment/",
        "/st-001-create-payment",
        "epic-payment/create-payment",         // the old feature name
        "epic-payment/st-1-create-payment",    // fewer than 3 digits
        "epic-payment/st-001",                 // no name
        "epic-payment/ST-001-create-payment",  // uppercase
        "epic-payment/st-001-Create",          // uppercase name
        "epic-payment/st-001-create_payment",  // underscore not allowed in stories
        "epic-payment/st-001-create-payment/", // trailing slash
        "epic-payment/st-001-create-payment/tasks",
        "epic-payment/st-001-create-payment/README.md",
        "Epic-Payment/st-001-create-payment",
        "../x/st-001-create-payment",
        "epic-payment/../st-001-create-payment",
        "epic-payment/.st-001-create-payment",
        "epic-payment/st-001-create%2dpayment",
        "epic-payment\\st-001-create-payment",
        "epics/epic-payment/st-001-create-payment",
    ] {
        let err = context(&f.ws(), bad).unwrap_err();
        assert_eq!(err.exit_code(), exit::INVALID, "{bad:?}");
    }
    let message = context(&f.ws(), "epic-payment/create-payment")
        .unwrap_err()
        .detail();
    assert!(
        message.contains("epic-payment/st-001-create-payment"),
        "{message}"
    );
}

#[tokio::test]
async fn needs_a_pulled_clean_workspace() {
    let f = Fixture::new().await;
    // Never pulled: no head yet.
    let err = context(&f.ws(), STORY).unwrap_err();
    assert_eq!(err.exit_code(), exit::CONFLICT);

    f.pull().await.unwrap();
    assert!(context(&f.ws(), STORY).is_ok());

    // An unresolved conflict makes the workspace unclean.
    let readme = format!("{STORY_DIR}/README.md");
    f.write(&readme, "# mine\n");
    f.api.external_change(&readme, Some("# theirs\n"));
    let report = f.pull().await.unwrap();
    assert!(!report.conflicts.is_empty());
    let err = context(&f.ws(), STORY).unwrap_err();
    assert_eq!(err.exit_code(), exit::CONFLICT);
    assert!(err.detail().contains("not clean"), "{err:?}");
}

#[tokio::test]
async fn ignores_files_that_are_not_canonical_documents() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    for path in [
        "tasks/notes.md",
        "tasks/fe-1-short-number.md",
        "tasks/FE-001-upper.md",
        "tasks/fe-001-form.html",
        "tasks/nested/fe-001-form.md",
        "frontend.md",
        "layout.md",
        "README.html",
    ] {
        f.write(&format!("{STORY_DIR}/{path}"), "x\n");
    }
    f.write("epics/epic-payment/briefs/brief.md", "x\n");
    f.write("epics/epic-payment/product-brief.md", "x\n");
    let report = context(&f.ws(), STORY).unwrap();
    assert_eq!(report.paths.len(), 6, "{:?}", report.paths);
    assert!(
        report
            .paths
            .iter()
            .all(|p| !p.contains("notes") && !p.contains("product-brief"))
    );
}

#[tokio::test]
async fn includes_new_local_documents_before_they_are_pushed() {
    let f = Fixture::new().await;
    f.pull().await.unwrap();
    f.write(&format!("{STORY_DIR}/layout.html"), "<main></main>\n");
    f.write(&format!("{STORY_DIR}/tasks/qa-001-smoke.md"), "# qa\n");
    let report = context(&f.ws(), STORY).unwrap();
    assert!(report.paths.iter().any(|p| p.ends_with("/layout.html")));
    assert!(
        report
            .paths
            .iter()
            .any(|p| p.ends_with("/tasks/qa-001-smoke.md"))
    );
}

#[cfg(unix)]
#[tokio::test]
async fn never_lists_symbolic_links() {
    use std::os::unix::fs::symlink;

    let f = Fixture::new().await;
    f.api
        .external_change(&format!("{STORY_DIR}/tasks/fe-001-form.md"), Some("# fe\n"));
    f.pull().await.unwrap();
    let outside = tempfile::tempdir().expect("tempdir");
    std::fs::write(outside.path().join("secret.md"), "secret\n").unwrap();

    // A linked file, and a linked folder holding a brief.
    let linked = f.path(&format!("{STORY_DIR}/tasks/fe-002-linked.md"));
    symlink(outside.path().join("secret.md"), &linked).unwrap();
    std::fs::create_dir_all(outside.path().join("briefs")).unwrap();
    std::fs::write(outside.path().join("briefs/br-001-x.md"), "secret\n").unwrap();
    std::fs::remove_dir_all(f.path("epics/epic-payment/briefs")).unwrap();
    symlink(
        outside.path().join("briefs"),
        f.path("epics/epic-payment/briefs"),
    )
    .unwrap();

    let report = context(&f.ws(), STORY).unwrap();
    assert!(
        report
            .paths
            .iter()
            .all(|p| !p.contains("fe-002-linked") && !p.contains("/briefs/")),
        "{:?}",
        report.paths
    );
    assert!(
        report
            .paths
            .iter()
            .any(|p| p.ends_with("tasks/fe-001-form.md"))
    );
}
