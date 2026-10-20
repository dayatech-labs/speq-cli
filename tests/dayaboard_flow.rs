//! The Dayaboard API key issued with `speq login`: stored with the token, expiring with it, and revoked when this
//! device logs out or signs in again. The fake API also plays Dayaboard (`/api/auth/app-disconnect`).
mod support;

use speq::credentials::StoredCredential;
use speq::dayaboard::{Connection, Revocation};
use speq::session::Session;
use support::{Device, FakeApi, FakeDayaboard};

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
