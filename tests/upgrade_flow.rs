use semver::Version;
use speq::hashing::sha256_hex;
#[cfg(unix)]
use speq::upgrade::Asset;
use speq::upgrade::{self, Install};
use std::collections::HashMap;

#[path = "support/http.rs"]
mod http;

/// Serves fixed bodies by path, like a release page.
struct Releases {
    base: String,
    _server: http::HttpServer,
}

impl Releases {
    fn start(routes: HashMap<String, Vec<u8>>) -> Releases {
        let server = http::serve(move |request| match routes.get(&request.url) {
            Some(body) => http::Reply {
                status: 200,
                headers: vec![],
                body: body.clone(),
            },
            None => http::Reply {
                status: 404,
                headers: vec![],
                body: b"missing".to_vec(),
            },
        });
        Releases {
            base: format!("http://{}", server.addr),
            _server: server,
        }
    }
}

fn manifest(base: &str, version: &str, binary: &[u8]) -> Vec<u8> {
    serde_json::json!({
        "version": version,
        "commit": "abc",
        "artifacts": { upgrade::CURRENT_TARGET: { "binary": {
            "url": format!("{base}/bin/speq-{version}"), "sha256": sha256_hex(binary), "size": binary.len() } } }
    })
    .to_string()
    .into_bytes()
}

/// The release key of these tests (a fixed seed), standing in for the one CI signs with.
fn release_key() -> ring::signature::Ed25519KeyPair {
    ring::signature::Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap()
}

fn trusted() -> Vec<[u8; 32]> {
    use ring::signature::KeyPair;
    vec![release_key().public_key().as_ref().try_into().unwrap()]
}

/// `routes` plus, for each `manifest.json`, its `manifest.json.sig` by `key`.
fn signed_by(
    key: &ring::signature::Ed25519KeyPair,
    routes: Vec<(&str, Vec<u8>)>,
) -> HashMap<String, Vec<u8>> {
    let mut all = HashMap::new();
    for (path, body) in routes {
        if path.ends_with("manifest.json") {
            all.insert(format!("{path}.sig"), key.sign(&body).as_ref().to_vec());
        }
        all.insert(path.to_string(), body);
    }
    all
}

fn signed(routes: Vec<(&str, Vec<u8>)>) -> HashMap<String, Vec<u8>> {
    signed_by(&release_key(), routes)
}

#[tokio::test]
async fn check_reports_update_available_and_up_to_date() {
    let releases = Releases::start(signed(vec![(
        "/latest/download/manifest.json",
        manifest("http://127.0.0.1:1", "99.0.0", b"x"),
    )]));
    let http = upgrade::http_client().unwrap();
    let url = upgrade::manifest_url(&releases.base, None).unwrap();
    let found = upgrade::fetch_manifest(&http, &url, &trusted())
        .await
        .unwrap();
    let report = upgrade::check(&upgrade::current_version(), &found);
    assert_eq!(report.status, "update-available");
    assert_eq!(report.latest, "99.0.0");

    let same = upgrade::check(&Version::parse("99.0.0").unwrap(), &found);
    assert_eq!(same.status, "up-to-date");
}

#[tokio::test]
async fn network_failure_or_invalid_manifest_is_an_error_never_up_to_date() {
    let http = upgrade::http_client().unwrap();
    let down = upgrade::manifest_url("http://127.0.0.1:1", None).unwrap();
    assert_eq!(
        upgrade::fetch_manifest(&http, &down, &trusted())
            .await
            .unwrap_err()
            .exit_code(),
        speq::error::exit::UNAVAILABLE
    );

    // Signed, so what is refused is the content.
    let releases = Releases::start(signed(vec![
        ("/latest/download/manifest.json", b"{ not json".to_vec()),
        ("/download/v1.0.0/manifest.json", serde_json::json!({ "version": "1.0.0", "artifacts": { "t": { "binary": { "url": "http://evil.example/x", "sha256": "00", "size": 1 } } } }).to_string().into_bytes()),
    ]));
    let invalid = upgrade::manifest_url(&releases.base, None).unwrap();
    let error = upgrade::fetch_manifest(&http, &invalid, &trusted())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("manifest is invalid"), "{error}");
    let pinned =
        upgrade::manifest_url(&releases.base, Some(&Version::parse("1.0.0").unwrap())).unwrap();
    let error = upgrade::fetch_manifest(&http, &pinned, &trusted())
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("entry for t is invalid"),
        "non-https asset URLs are rejected: {error}"
    );
}

/// The release workflow signs with `openssl pkeyutl -sign -rawin` (OpenSSL 3): its signature must verify here, or
/// the first signed release would stop every upgrade. Skipped where `openssl` is not OpenSSL 3 (it is on CI).
#[test]
fn a_signature_made_like_the_release_workflow_verifies() {
    let openssl = |args: &[&str]| std::process::Command::new("openssl").args(args).output();
    let Ok(version) = openssl(&["version"]) else {
        return eprintln!("skipped: no openssl");
    };
    if !String::from_utf8_lossy(&version.stdout).starts_with("OpenSSL 3") {
        return eprintln!("skipped: openssl is not OpenSSL 3");
    }
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name).to_str().unwrap().to_string();
    let manifest = manifest("https://example.test", "9.9.9", b"binary");
    std::fs::write(path("manifest.json"), &manifest).unwrap();
    for args in [
        vec!["genpkey", "-algorithm", "ed25519", "-out", &path("key.pem")],
        vec![
            "pkeyutl",
            "-sign",
            "-rawin",
            "-inkey",
            &path("key.pem"),
            "-in",
            &path("manifest.json"),
            "-out",
            &path("manifest.json.sig"),
        ],
        vec![
            "pkey",
            "-in",
            &path("key.pem"),
            "-pubout",
            "-outform",
            "DER",
            "-out",
            &path("public.der"),
        ],
    ] {
        assert!(openssl(&args).unwrap().status.success(), "openssl {args:?}");
    }
    let der = std::fs::read(path("public.der")).unwrap();
    let key: [u8; 32] = der[der.len() - 32..].try_into().unwrap();
    let signature = std::fs::read(path("manifest.json.sig")).unwrap();

    assert!(upgrade::signed_by(&manifest, &signature, &[key]));
    assert!(!upgrade::signed_by(b"other bytes", &signature, &[key]));
}

#[tokio::test]
async fn a_manifest_without_the_release_key_signature_is_never_believed() {
    let http = upgrade::http_client().unwrap();
    let good = manifest("http://127.0.0.1:1", "99.0.0", b"x");
    let forger = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[9; 32]).unwrap();
    let mut tampered = signed(vec![("/latest/download/manifest.json", good.clone())]);
    // Same signature, other bytes: a manifest pointing at another binary.
    tampered.insert(
        "/latest/download/manifest.json".into(),
        manifest("http://127.0.0.1:1", "99.0.0", b"evil"),
    );
    let mut unsigned = signed(vec![("/latest/download/manifest.json", good.clone())]);
    unsigned.remove("/latest/download/manifest.json.sig");

    for (case, routes, expected) in [
        ("unsigned", unsigned, "no signature"),
        ("tampered", tampered, "not signed with speq's release key"),
        (
            "another key",
            signed_by(
                &forger,
                vec![("/latest/download/manifest.json", good.clone())],
            ),
            "not signed with speq's release key",
        ),
    ] {
        let releases = Releases::start(routes);
        let url = upgrade::manifest_url(&releases.base, None).unwrap();
        let error = upgrade::fetch_manifest(&http, &url, &trusted())
            .await
            .unwrap_err();
        assert!(error.to_string().contains(expected), "{case}: {error}");
    }

    // A build that trusts no key believes no manifest.
    let releases = Releases::start(signed(vec![("/latest/download/manifest.json", good)]));
    let url = upgrade::manifest_url(&releases.base, None).unwrap();
    assert!(upgrade::fetch_manifest(&http, &url, &[]).await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn install_verifies_then_replaces_atomically_and_keeps_the_old_binary_on_failure() {
    use std::os::unix::fs::PermissionsExt;
    let new_binary = b"#!/bin/sh\necho \"speq 9.9.9\"\n".to_vec();
    let releases = Releases::start(HashMap::from([
        ("/bin/good".to_string(), new_binary.clone()),
        (
            "/bin/tampered".to_string(),
            b"#!/bin/sh\necho evil\n".to_vec(),
        ),
        (
            "/bin/liar".to_string(),
            b"#!/bin/sh\necho \"speq 1.0.0\"\n".to_vec(),
        ),
    ]));
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("speq");
    std::fs::write(&exe, "old").unwrap();
    let http = upgrade::http_client().unwrap();
    let version = Version::parse("9.9.9").unwrap();
    let asset = |path: &str, body: &[u8]| Asset {
        url: format!("{}{path}", releases.base),
        sha256: sha256_hex(body),
        size: body.len() as u64,
    };

    // Checksum mismatch: the bytes served differ from what the manifest promised.
    let mut bad = asset("/bin/tampered", &new_binary);
    bad.size = b"#!/bin/sh\necho evil\n".len() as u64;
    assert!(
        upgrade::install_binary(&http, &bad, &exe, &version)
            .await
            .is_err()
    );
    assert_eq!(std::fs::read_to_string(&exe).unwrap(), "old");

    // Right checksum but the binary does not report the expected version.
    let lying = asset("/bin/liar", b"#!/bin/sh\necho \"speq 1.0.0\"\n");
    assert!(
        upgrade::install_binary(&http, &lying, &exe, &version)
            .await
            .is_err()
    );
    assert_eq!(std::fs::read_to_string(&exe).unwrap(), "old");

    // Unreachable download.
    let gone = Asset {
        url: "http://127.0.0.1:1/x".into(),
        sha256: "0".repeat(64),
        size: 5,
    };
    assert!(
        upgrade::install_binary(&http, &gone, &exe, &version)
            .await
            .is_err()
    );
    assert_eq!(std::fs::read_to_string(&exe).unwrap(), "old");

    upgrade::install_binary(&http, &asset("/bin/good", &new_binary), &exe, &version)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&exe).unwrap(), new_binary);
    assert_eq!(
        std::fs::metadata(&exe).unwrap().permissions().mode() & 0o111,
        0o111
    );
    // No temporary files are left behind.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn a_new_binary_that_cannot_install_skills_only_produces_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let warning = upgrade::refresh_skills(&dir.path().join("missing-speq"));
    assert!(warning.unwrap().contains("speq skills install"));
}

#[cfg(unix)]
#[test]
fn the_new_binary_is_asked_to_install_its_own_skills() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let args = dir.path().join("args");
    let exe = dir.path().join("speq");
    std::fs::write(
        &exe,
        format!("#!/bin/sh\necho \"$@\" > '{}'\n", args.display()),
    )
    .unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(upgrade::refresh_skills(&exe), None);
    assert_eq!(
        std::fs::read_to_string(&args).unwrap().trim(),
        "skills install"
    );
}

#[test]
fn package_manager_binaries_are_never_replaced() {
    let official = std::path::Path::new("/home/a/.local/bin");
    for path in [
        "/opt/homebrew/Cellar/speq/1.0.0/bin/speq",
        "/home/linuxbrew/.linuxbrew/bin/speq",
        "C:\\Users\\a\\scoop\\shims\\speq.exe",
    ] {
        assert!(
            matches!(
                upgrade::detect_install(std::path::Path::new(path), Some(official)),
                Install::Managed { .. }
            ),
            "{path}"
        );
    }
    assert_eq!(
        upgrade::detect_install(std::path::Path::new("/usr/local/bin/speq"), Some(official)),
        Install::Unmanaged
    );
}
