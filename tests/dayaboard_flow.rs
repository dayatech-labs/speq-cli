//! The Dayaboard API key issued with `speq login`: stored with the token, expiring with it, and revoked when this
//! device logs out or signs in again. The fake API also plays Dayaboard (`/api/auth/app-disconnect`).
mod support;

use serde_json::json;
use speq::credentials::StoredCredential;
use speq::dayaboard::{Connection, Revocation};
use speq::error::Error;
use speq::session::Session;
use support::{Device, FakeApi, FakeDayaboard, Machine};

fn stored(device: &Device) -> StoredCredential {
    device
        .creds
        .load(&device.env.account())
        .unwrap()
        .expect("credential in the store")
}

fn live_keys(api: &FakeApi) -> Vec<String> {
    api.with(|s| s.dayaboard_keys.iter().cloned().collect())
}

#[tokio::test]
async fn login_stores_a_dayaboard_key_that_expires_with_the_token() {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    let device = Device::new(&api);
    let report = device.login().await;

    let credential = stored(&device);
    let key = credential.dayaboard.clone().expect("Dayaboard key stored");
    assert_eq!(key.url, api.url);
    assert_eq!(live_keys(&api), vec![key.api_key.clone()]);
    let drift = (key.expires_at - credential.expires_at).abs();
    assert!(drift < time::Duration::seconds(5), "drift {drift}");
    assert_eq!(
        report.dayaboard,
        Some(Connection::Connected {
            url: api.url.clone(),
            expires_at: key.expires_at,
        })
    );
    // The key never leaves the credential store, and the debug form of the credential hides it.
    assert!(!format!("{credential:?}").contains(&key.api_key));
}

#[tokio::test]
async fn login_succeeds_without_a_key_when_dayaboard_refuses() {
    let api = FakeApi::start();
    api.with(|s| {
        s.dayaboard = FakeDayaboard::Refuse(
            "dayaboard_refused".into(),
            "User is not registered\u{1b}[31m in Dayaboard".into(),
        )
    });
    let device = Device::new(&api);
    let report = device.login().await;

    assert!(stored(&device).dayaboard.is_none());
    assert_eq!(
        report.dayaboard,
        Some(Connection::NotConnected {
            code: "dayaboard_refused".into(),
            message: "User is not registered [31m in Dayaboard".into(),
        })
    );
    // The Speq session works regardless.
    assert!(Session::new(&device.env).authenticate().await.is_ok());
}

#[tokio::test]
async fn an_api_without_the_integration_leaves_no_dayaboard_state() {
    let api = FakeApi::start();
    let device = Device::new(&api);
    let report = device.login().await;
    assert_eq!(report.dayaboard, None);
    assert!(stored(&device).dayaboard.is_none());
    let report = Session::new(&device.env).logout(false).await.unwrap();
    assert_eq!(report.dayaboard, None);
    assert_eq!(api.count("app-disconnect"), 0);
}

#[tokio::test]
async fn refuses_a_key_for_a_dayaboard_that_is_not_https() {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::ConnectAt("http://dayaboard.example".into()));
    let device = Device::new(&api);
    let report = device.login().await;
    assert!(stored(&device).dayaboard.is_none());
    assert!(matches!(
        report.dayaboard,
        Some(Connection::NotConnected { ref code, .. }) if code == "invalid_response"
    ));
}

#[tokio::test]
async fn signing_in_again_revokes_the_key_it_replaces() {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    let device = Device::new(&api);
    device.login().await;
    let first = stored(&device).dayaboard.unwrap().api_key;
    device.login().await;
    let second = stored(&device).dayaboard.unwrap().api_key;

    assert_ne!(first, second);
    assert_eq!(live_keys(&api), vec![second]);
    // Only the Dayaboard key is sent to Dayaboard, never the Speq token.
    assert_eq!(
        api.with(|s| s.dayaboard_disconnects.clone()),
        vec![format!("Bearer {first}")]
    );
}

#[tokio::test]
async fn logout_revokes_this_devices_key_only() {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    let (laptop, desktop) = (Device::new(&api), Device::new(&api));
    laptop.login().await;
    desktop.login().await;
    let desktop_key = stored(&desktop).dayaboard.unwrap();

    let report = Session::new(&laptop.env).logout(false).await.unwrap();
    assert!(report.was_logged_in);
    assert!(matches!(
        report.dayaboard,
        Some(Revocation { revoked: true, .. })
    ));
    assert_eq!(live_keys(&api), vec![desktop_key.api_key]);
    assert!(laptop.creds.load(&laptop.env.account()).unwrap().is_none());
}

#[tokio::test]
async fn logout_all_also_revokes_the_key() {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    let device = Device::new(&api);
    device.login().await;
    let report = Session::new(&device.env).logout(true).await.unwrap();
    assert!(report.all_devices);
    assert!(matches!(
        report.dayaboard,
        Some(Revocation { revoked: true, .. })
    ));
    assert!(live_keys(&api).is_empty());
}

#[tokio::test]
async fn logout_still_signs_out_when_dayaboard_cannot_revoke() {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    let device = Device::new(&api);
    device.login().await;
    let key = stored(&device).dayaboard.unwrap();
    api.with(|s| s.dayaboard_down = true);

    let report = Session::new(&device.env).logout(false).await.unwrap();
    assert_eq!(
        report.dayaboard,
        Some(Revocation {
            revoked: false,
            expires_at: key.expires_at,
        })
    );
    assert!(device.creds.load(&device.env.account()).unwrap().is_none());
}

#[tokio::test]
async fn a_key_dayaboard_no_longer_accepts_counts_as_revoked() {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    let device = Device::new(&api);
    device.login().await;
    api.with(|s| s.dayaboard_keys.clear());
    let report = Session::new(&device.env).logout(false).await.unwrap();
    assert!(matches!(
        report.dayaboard,
        Some(Revocation { revoked: true, .. })
    ));
}

#[tokio::test]
async fn an_expired_key_is_not_sent_to_dayaboard() {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    let device = Device::new(&api);
    device.login().await;
    let mut credential = stored(&device);
    if let Some(key) = credential.dayaboard.as_mut() {
        key.expires_at = time::OffsetDateTime::now_utc() - time::Duration::minutes(1);
    }
    device
        .creds
        .save(&device.env.account(), &credential)
        .unwrap();

    let report = Session::new(&device.env).logout(false).await.unwrap();
    assert_eq!(report.dayaboard, None);
    assert_eq!(api.count("app-disconnect"), 0);
}

#[tokio::test]
async fn uninstall_revokes_the_key_before_the_credential_is_removed() {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    let device = Device::new(&api);
    device.login().await;
    let revocation = Session::new(&device.env).disconnect_dayaboard().await;
    assert!(matches!(revocation, Some(Revocation { revoked: true, .. })));
    assert!(live_keys(&api).is_empty());
}

#[test]
fn credentials_saved_before_the_integration_still_load_and_round_trip() {
    let old = r#"{"user_id":"u1","access_token":"sat_x","expires_at":"2026-11-19T14:58:51Z"}"#;
    let credential: StoredCredential = serde_json::from_str(old).unwrap();
    assert!(credential.dayaboard.is_none());
    // Without a key the stored form is unchanged, so an older speq can still read it.
    assert!(
        !serde_json::to_string(&credential)
            .unwrap()
            .contains("dayaboard")
    );
}

// ------------------------------------------------------------ speq dayaboard

const UUID: &str = "6f1c2a9e-3b4d-4e5f-8a7b-9c0d1e2f3a4b";

async fn connected() -> (FakeApi, Device) {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    let device = Device::new(&api);
    device.login().await;
    (api, device)
}

#[tokio::test]
async fn calls_send_the_stored_key_body_and_idempotency_key_to_the_issuing_origin() {
    let (api, device) = connected().await;
    let key = stored(&device).dayaboard.unwrap();
    let issue = json!({ "projectKey": "DEV", "summary": "st-003 Pay by card", "issueType": "Story", "storyPoints": 3 });
    api.with(|s| s.dayaboard_reply = Some((201, json!({ "issueKey": "DEV-42" }), vec![])));

    let response = Session::new(&device.env)
        .dayaboard("post", "/api/v1/workflow/issues", Some(&issue), Some(UUID))
        .await
        .unwrap();
    assert_eq!(response.status, 201);
    assert_eq!(response.body, json!({ "issueKey": "DEV-42" }));
    assert!(response.error().is_none());

    let call = api.with(|s| s.dayaboard_calls.clone()).pop().unwrap();
    assert_eq!(call.method, "POST");
    assert_eq!(call.url, "/api/v1/workflow/issues");
    assert_eq!(call.authorization, Some(format!("Bearer {}", key.api_key)));
    assert_eq!(call.idempotency_key.as_deref(), Some(UUID));
    assert_eq!(call.body, issue);
}

#[tokio::test]
async fn queries_reach_dayaboard_unchanged() {
    let (api, device) = connected().await;
    let path = "/api/v1/workflow/issues?projectKey=DEV&q=st-003&limit=10";
    Session::new(&device.env)
        .dayaboard("GET", path, None, None)
        .await
        .unwrap();
    let call = api.with(|s| s.dayaboard_calls.clone()).pop().unwrap();
    assert_eq!((call.method.as_str(), call.url.as_str()), ("GET", path));
    assert_eq!(call.idempotency_key, None);
}

#[tokio::test]
async fn paths_outside_the_workflow_and_docs_api_never_leave_the_machine() {
    let (api, device) = connected().await;
    let session = Session::new(&device.env);
    for path in [
        "/api/auth/app-connect",
        "/api/v1/workflow/",
        "/api/v1/workflow/../auth/app-disconnect",
        "/api/v1/workflow/%2e%2e/auth",
        "/api/v1/workflow//issues",
        "//evil.example/api/v1/workflow/issues",
        "https://evil.example/api/v1/workflow/issues",
        "api/v1/workflow/issues",
        "/api/v1/workflow/issues#x",
        "/api/v1/workflow/a b",
        "/api/v1/workflow/a\\b",
    ] {
        let error = session
            .dayaboard("GET", path, None, None)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Invalid(_)), "{path}: {error:?}");
    }
    assert!(api.with(|s| s.dayaboard_calls.is_empty()));
}

#[tokio::test]
async fn only_uuid_idempotency_keys_and_known_methods_are_sent() {
    let (api, device) = connected().await;
    let session = Session::new(&device.env);
    for key in [
        "abc",
        "6f1c2a9e3b4d4e5f8a7b9c0d1e2f3a4b",
        "6f1c2a9e-3b4d-4e5f-8a7b-9c0d1e2f3a4z",
    ] {
        let error = session
            .dayaboard(
                "POST",
                "/api/v1/workflow/issues",
                Some(&json!({})),
                Some(key),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Invalid(_)), "{key}");
    }
    let error = session
        .dayaboard("DELETE", "/api/v1/workflow/issues/DEV-1", None, None)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
    assert!(api.with(|s| s.dayaboard_calls.is_empty()));
}

#[tokio::test]
async fn without_a_usable_key_the_answer_is_to_sign_in_again() {
    // Logged out: the Speq login is missing.
    let api = FakeApi::start();
    let device = Device::new(&api);
    let error = Session::new(&device.env)
        .dayaboard("GET", "/api/v1/workflow/projects", None, None)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::NotLoggedIn));

    // Logged in while Dayaboard could not be connected.
    device.login().await;
    let error = Session::new(&device.env)
        .dayaboard("GET", "/api/v1/workflow/projects", None, None)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::DayaboardNotConnected));
    assert_eq!(error.exit_code(), speq::error::exit::AUTH);

    // An expired key is not sent.
    let (api, device) = connected().await;
    let mut credential = stored(&device);
    if let Some(key) = credential.dayaboard.as_mut() {
        key.expires_at = time::OffsetDateTime::now_utc() - time::Duration::seconds(1);
    }
    device
        .creds
        .save(&device.env.account(), &credential)
        .unwrap();
    let error = Session::new(&device.env)
        .dayaboard("GET", "/api/v1/workflow/projects", None, None)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::DayaboardNotConnected));
    assert!(api.with(|s| s.dayaboard_calls.is_empty()));
}

#[tokio::test]
async fn a_key_dayaboard_rejects_is_forgotten_but_the_speq_session_stays() {
    let (api, device) = connected().await;
    api.with(|s| s.dayaboard_keys.clear());
    let session = Session::new(&device.env);
    let error = session
        .dayaboard("GET", "/api/v1/workflow/projects", None, None)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::DayaboardNotConnected));
    assert!(stored(&device).dayaboard.is_none());
    assert!(session.authenticate().await.is_ok());

    // The next call does not ask Dayaboard again.
    let calls = api.with(|s| s.dayaboard_calls.len());
    let error = session
        .dayaboard("GET", "/api/v1/workflow/projects", None, None)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::DayaboardNotConnected));
    assert_eq!(api.with(|s| s.dayaboard_calls.len()), calls);
}

#[tokio::test]
async fn dayaboard_errors_keep_their_code_details_and_retry_hint() {
    let (api, device) = connected().await;
    let session = Session::new(&device.env);

    api.with(|s| {
        s.dayaboard_reply = Some((
            404,
            json!({ "error": "project_not_found", "message": "Project not found", "details": { "availableProjects": ["DEV"] } }),
            vec![],
        ))
    });
    let response = session
        .dayaboard("POST", "/api/v1/workflow/issues", Some(&json!({})), None)
        .await
        .unwrap();
    let error = response.error().expect("an error");
    assert_eq!(error.code(), "project_not_found");
    assert_eq!(error.exit_code(), speq::error::exit::INVALID);
    let Error::Api(api_error) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(
        api_error.details,
        Some(json!({ "availableProjects": ["DEV"] }))
    );

    api.with(|s| {
        s.dayaboard_reply = Some((
            429,
            json!({ "error": "Too Many Requests" }),
            vec![("Retry-After".into(), "30".into())],
        ))
    });
    let error = session
        .dayaboard("GET", "/api/v1/workflow/projects", None, None)
        .await
        .unwrap()
        .error()
        .expect("an error");
    assert_eq!(error.exit_code(), speq::error::exit::UNAVAILABLE);
    assert_eq!(error.hint().as_deref(), Some("Try again in 30s"));
}

#[tokio::test]
async fn answers_are_made_safe_for_a_terminal() {
    let (api, device) = connected().await;
    api.with(|s| {
        s.dayaboard_reply = Some((
            200,
            json!({ "summary": "a\u{202E}b\u{1b}[31m", "items": [{ "x": "\u{2066}y" }] }),
            vec![],
        ))
    });
    let response = Session::new(&device.env)
        .dayaboard("GET", "/api/v1/workflow/issues/DEV-1", None, None)
        .await
        .unwrap();
    assert_eq!(response.body["items"][0]["x"], " y");
    let printed = serde_json::to_string_pretty(&response.body).unwrap();
    assert!(printed.contains("a b\\u001b[31m"), "{printed}");
    assert!(!printed.contains('\u{1b}'));
}

#[tokio::test]
async fn the_command_prints_dayaboards_json_and_exits_with_its_status() {
    let machine = Machine::new().await;
    machine.api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    machine.device.login().await;
    let cwd = machine.dir("work");
    std::fs::write(
        cwd.join("issue.json"),
        r#"{"projectKey":"DEV","summary":"x"}"#,
    )
    .unwrap();

    machine
        .api
        .with(|s| s.dayaboard_reply = Some((201, json!({ "issueKey": "DEV-7" }), vec![])));
    let out = machine.run(
        &cwd,
        &[
            "dayaboard",
            "POST",
            "/api/v1/workflow/issues",
            "--data",
            "@issue.json",
            "--idempotency-key",
            UUID,
        ],
    );
    assert_eq!(out.code, 0, "{out:?}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&out.stdout).unwrap(),
        json!({ "issueKey": "DEV-7" })
    );
    let call = machine
        .api
        .with(|s| s.dayaboard_calls.clone())
        .pop()
        .unwrap();
    assert_eq!(call.body, json!({ "projectKey": "DEV", "summary": "x" }));
    let key = machine
        .device
        .creds
        .load(&machine.device.env.account())
        .unwrap()
        .and_then(|c| c.dayaboard)
        .unwrap();
    assert!(!out.stdout.contains(&key.api_key) && !out.stderr.contains(&key.api_key));

    machine.api.with(|s| {
        s.dayaboard_reply = Some((
            400,
            json!({ "error": "validation_error", "message": "summary is required" }),
            vec![],
        ))
    });
    let out = machine.run(
        &cwd,
        &[
            "dayaboard",
            "POST",
            "/api/v1/workflow/issues",
            "--data",
            "{}",
        ],
    );
    assert_eq!(out.code, 7, "{out:?}");
    assert!(out.stdout.contains("validation_error"));
    assert!(out.stderr.contains("summary is required"), "{}", out.stderr);

    let out = machine.run(
        &cwd,
        &[
            "dayaboard",
            "POST",
            "/api/v1/workflow/issues",
            "--data",
            "{oops",
        ],
    );
    assert_eq!(out.code, 7);
    assert!(out.stderr.contains("not valid JSON"), "{}", out.stderr);
}

// ------------------------------------------------------------ Web app links

#[tokio::test]
async fn speq_list_names_the_web_app_so_documents_can_be_linked() {
    let api = FakeApi::start();
    let device = Device::new(&api);
    device.login().await;
    let report = speq::clone::list(&device.env).await.unwrap();
    assert_eq!(report.web_url.as_deref(), Some("https://speq.example"));
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["web_url"], "https://speq.example");
}

#[tokio::test]
async fn an_unsafe_or_missing_web_app_origin_is_left_out() {
    for web_url in [
        Some("http://speq.example"),
        Some("https://speq.example/path"),
        Some("javascript:alert(1)"),
        None,
    ] {
        let api = FakeApi::start();
        api.with(|s| s.web_url = web_url.map(Into::into));
        let device = Device::new(&api);
        device.login().await;
        let report = speq::clone::list(&device.env).await.unwrap();
        assert_eq!(report.web_url, None, "{web_url:?}");
        assert!(
            serde_json::to_value(&report)
                .unwrap()
                .get("web_url")
                .is_none()
        );
    }
}

#[tokio::test]
async fn speq_list_names_each_projects_dayaboard_project() {
    let api = FakeApi::start();
    api.with(|s| {
        s.projects
            .get_mut(support::PROJECT)
            .unwrap()
            .dayaboard_project_key = Some("DEV\u{1b}[31m".into())
    });
    let device = Device::new(&api);
    device.login().await;
    let report = speq::clone::list(&device.env).await.unwrap();
    let key = |id: &str| {
        report
            .projects
            .iter()
            .find(|p| p.project_id == id)
            .and_then(|p| p.dayaboard_project_key.clone())
    };
    // Server text is made printable, like every other name the API sends.
    assert_eq!(key(support::PROJECT).as_deref(), Some("DEV [31m"));
    assert_eq!(key("proj_cart"), None);
    let json = serde_json::to_value(&report).unwrap();
    assert!(
        json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["dayaboard_project_key"] == "DEV [31m")
    );
}
