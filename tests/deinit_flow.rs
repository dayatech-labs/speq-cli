mod support;

use speq::credentials::CredentialStore;
use speq::deinit::deinit;
use speq::error::Error;
use support::Fixture;

#[tokio::test]
async fn removes_the_metadata_and_the_specs_folder_including_local_edits() {
    let fx = Fixture::new().await;
    fx.pull().await.expect("pull");
    fx.write("epics/a/prd.md", "unpushed edit");
    let ws = fx.ws();
    assert!(ws.speq_dir().exists() && ws.specs_dir().exists());

    let report = deinit(&fx.device.env, &ws).unwrap();

    assert_eq!(report.specs_dir, "specs");
    assert!(!ws.speq_dir().exists(), ".speq/ is gone");
    assert!(!ws.specs_dir().exists(), "specs/ is gone");
    assert!(
        matches!(
            speq::workspace::Workspace::discover(fx.workspace_dir.path()),
            Err(Error::NotInitialized)
        ),
        "the directory is no longer a workspace"
    );
}

#[tokio::test]
async fn leaves_other_files_the_gitignore_and_the_login_alone() {
    let fx = Fixture::new().await;
    std::fs::write(fx.workspace_dir.path().join("README.md"), "mine").unwrap();
    std::fs::create_dir_all(fx.workspace_dir.path().join("src")).unwrap();
    let gitignore = std::fs::read_to_string(fx.workspace_dir.path().join(".gitignore")).ok();

    deinit(&fx.device.env, &fx.ws()).unwrap();

    assert_eq!(
        std::fs::read_to_string(fx.workspace_dir.path().join("README.md")).unwrap(),
        "mine"
    );
    assert!(fx.workspace_dir.path().join("src").is_dir());
    assert_eq!(
        std::fs::read_to_string(fx.workspace_dir.path().join(".gitignore")).ok(),
        gitignore
    );
    assert!(
        fx.device
            .creds
            .load(&fx.device.env.account())
            .unwrap()
            .is_some(),
        "deinit does not log out"
    );
}

#[tokio::test]
async fn honours_a_nested_local_dir_and_keeps_its_parent() {
    let fx = Fixture::new().await;
    let root = std::fs::canonicalize(fx.workspace_dir.path()).unwrap();
    let options = speq::init::InitOptions {
        repo: support::REPO,
        local_dir: "docs/specs",
        reconfigure: true,
    };
    speq::init::init(&fx.device.env, &root, options)
        .await
        .expect("reinit");
    std::fs::write(root.join("docs/guide.md"), "keep").unwrap();

    deinit(&fx.device.env, &fx.ws()).unwrap();

    assert!(!root.join("docs/specs").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("docs/guide.md")).unwrap(),
        "keep"
    );
}

#[tokio::test]
async fn can_finish_when_the_specs_folder_is_already_gone() {
    let fx = Fixture::new().await;
    let ws = fx.ws();
    std::fs::remove_dir_all(ws.specs_dir()).unwrap();

    deinit(&fx.device.env, &ws).unwrap();

    assert!(!ws.speq_dir().exists());
}

#[cfg(unix)]
#[tokio::test]
async fn refuses_to_delete_through_a_symlink_and_changes_nothing() {
    let fx = Fixture::new().await;
    let ws = fx.ws();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("precious.md"), "outside").unwrap();
    std::fs::remove_dir_all(ws.specs_dir()).unwrap();
    std::os::unix::fs::symlink(outside.path(), ws.specs_dir()).unwrap();

    let err = deinit(&fx.device.env, &ws).unwrap_err();

    assert!(matches!(err, Error::Invalid(_)), "{err:?}");
    assert!(outside.path().join("precious.md").exists());
    assert!(ws.speq_dir().exists(), "the config survives a refusal");
}

#[tokio::test]
async fn is_refused_while_another_command_holds_the_workspace() {
    let fx = Fixture::new().await;
    let ws = fx.ws();
    let _busy = ws.guard(&fx.device.env, false).unwrap();

    let err = deinit(&fx.device.env, &ws).unwrap_err();

    assert!(matches!(err, Error::Conflict(_)), "{err:?}");
    assert!(ws.speq_dir().exists() && ws.specs_dir().exists());
}
