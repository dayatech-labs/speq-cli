//! `speq info`: the signed-in account, the session, the Dayaboard connection, and the roles held per repository.
mod support;

use speq::error::Error;
use speq::info::{self, DayaboardState, PlatformRole};
use speq::snapshot;
use support::{Device, FakeApi, FakeDayaboard};
use time::Duration;

#[tokio::test]
async fn reports_the_account_the_session_and_the_roles_held_in_each_repository() {
    let api = FakeApi::start();
    api.with(|s| {
        s.project_roles
            .insert("proj_cart".into(), vec!["analyst".into(), "qa".into()]);
    });
    let device = Device::new(&api);
    device.login().await;

    let report = info::info(&device.env, None).await.unwrap();

    assert_eq!(report.account.user_id, "user_a");
    assert_eq!(report.account.email.as_deref(), Some("user_a@example.com"));
    assert_eq!(report.account.name.as_deref(), Some("user_a"));
    assert_eq!(report.account.platform_role, Some(PlatformRole::User));
    assert_eq!(report.api_url, device.env.account());
    // The token lasts 30 days, and there is no refresh: that is when this session ends.
    let left = report.session.expires_at - snapshot::now();
    assert!(left > Duration::days(29) && left <= Duration::days(30));

    assert_eq!(report.projects.len(), 3);
    let payments = report
        .projects
        .iter()
        .find(|p| p.project_id == "proj_cart")
        .expect("the project the user is a member of");
    assert_eq!(payments.roles, vec!["analyst", "qa"]);
    // A project without a named role keeps the fake's default, whatever the roles of another project are.
    assert!(
        report
            .projects
            .iter()
            .filter(|p| p.project_id != "proj_cart")
            .all(|p| p.roles == vec!["engineer"])
    );
    assert_eq!(report.current_project_id, None);
}

#[tokio::test]
async fn says_whether_the_account_is_a_platform_admin_or_a_superadmin() {
    let api = FakeApi::start();
    let device = Device::new(&api);
    device.login().await;

    api.with(|s| {
        s.platform_admins.insert("user_a".into());
    });
    let admin = info::info(&device.env, None).await.unwrap();
    assert_eq!(
        admin.account.platform_role,
        Some(PlatformRole::PlatformAdmin)
    );

    api.with(|s| {
        s.superadmins.insert("user_a".into());
    });
    let root = info::info(&device.env, None).await.unwrap();
    assert_eq!(root.account.platform_role, Some(PlatformRole::Superadmin));
}

#[tokio::test]
async fn reports_the_dayaboard_connection_from_the_key_login_stored() {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    let device = Device::new(&api);
    device.login().await;
    let key = device
        .creds
        .load(&device.env.account())
        .unwrap()
        .and_then(|c| c.dayaboard)
        .expect("a Dayaboard key was stored at login");

    let report = info::info(&device.env, None).await.unwrap();
    assert_eq!(
        report.dayaboard,
        DayaboardState::Connected {
            expires_at: key.expires_at
        }
    );
    // The key itself is never part of the report.
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains(&key.api_key));
    assert!(!json.contains("api_key"));
}

#[tokio::test]
async fn reports_an_expired_dayaboard_key_as_expired_and_none_as_not_connected() {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    let device = Device::new(&api);
    device.login().await;

    let mut credential = device.creds.load(&device.env.account()).unwrap().unwrap();
    let ended = snapshot::now() - Duration::hours(1);
    credential.dayaboard.as_mut().unwrap().expires_at = ended;
    device
        .creds
        .save(&device.env.account(), &credential)
        .unwrap();
    let expired = info::info(&device.env, None).await.unwrap();
    assert_eq!(
        expired.dayaboard,
        DayaboardState::Expired { expires_at: ended }
    );

    credential.dayaboard = None;
    device
        .creds
        .save(&device.env.account(), &credential)
        .unwrap();
    let none = info::info(&device.env, None).await.unwrap();
    assert_eq!(none.dayaboard, DayaboardState::NotConnected);
}

#[tokio::test]
async fn an_api_without_the_integration_shows_dayaboard_as_not_connected() {
    let api = FakeApi::start();
    let device = Device::new(&api);
    device.login().await;
    let report = info::info(&device.env, None).await.unwrap();
    assert_eq!(report.dayaboard, DayaboardState::NotConnected);
}

#[tokio::test]
async fn still_reports_roles_and_session_when_the_account_name_cannot_be_fetched() {
    let api = FakeApi::start();
    let device = Device::new(&api);
    device.login().await;
    api.with(|s| s.me_down = true);

    let report = info::info(&device.env, None).await.unwrap();
    assert_eq!(report.account.user_id, "user_a");
    assert_eq!(report.account.email, None);
    assert_eq!(report.account.name, None);
    // Without the API's word for it, the platform role is not guessed.
    assert_eq!(report.account.platform_role, None);
    assert_eq!(report.projects.len(), 3);
}

#[tokio::test]
async fn marks_the_repository_the_command_ran_in() {
    let api = FakeApi::start();
    let device = Device::new(&api);
    device.login().await;
    let report = info::info(&device.env, Some("proj_cart".into()))
        .await
        .unwrap();
    assert_eq!(report.current_project_id.as_deref(), Some("proj_cart"));
}

#[tokio::test]
async fn needs_a_login() {
    let api = FakeApi::start();
    let device = Device::new(&api);
    assert!(matches!(
        info::info(&device.env, None).await,
        Err(Error::NotLoggedIn)
    ));
    assert_eq!(api.requests(), 0, "nothing to ask the API without a token");
}

#[tokio::test]
async fn an_expired_token_is_reported_as_expired_without_calling_the_api() {
    let api = FakeApi::start();
    let device = Device::new(&api);
    device.login().await;
    let before = api.requests();
    let mut credential = device.creds.load(&device.env.account()).unwrap().unwrap();
    credential.expires_at = snapshot::now() - Duration::seconds(1);
    device
        .creds
        .save(&device.env.account(), &credential)
        .unwrap();

    assert!(matches!(
        info::info(&device.env, None).await,
        Err(Error::SessionExpired)
    ));
    assert_eq!(api.requests(), before);
}

#[tokio::test]
async fn the_json_report_has_a_stable_shape() {
    let api = FakeApi::start();
    api.with(|s| s.dayaboard = FakeDayaboard::Connect);
    let device = Device::new(&api);
    device.login().await;
    let report = info::info(&device.env, Some("proj_cart".into()))
        .await
        .unwrap();

    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["account"]["user_id"], "user_a");
    assert_eq!(json["account"]["platform_role"], "user");
    assert_eq!(json["dayaboard"]["status"], "connected");
    assert!(json["dayaboard"]["expires_at"].is_string());
    assert!(json["session"]["expires_at"].is_string());
    assert_eq!(json["current_project_id"], "proj_cart");
    assert!(json["projects"][0]["roles"].is_array());
    assert!(json["projects"][0].get("capabilities").is_none());
}
