//! `speq upgrade`: verify and atomically replace the installed binary. It never runs on its own.
use crate::api::install_crypto_provider;
use crate::error::{Error, IoContext, Result};
use crate::hashing::sha256_hex;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::NamedTempFile;
use url::Url;

pub const DEFAULT_RELEASE_BASE: &str = "https://github.com/dayatech-labs/speq-cli/releases";
pub const CURRENT_TARGET: &str = env!("SPEQ_TARGET");
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 200 * 1024 * 1024;

/// Ed25519 public keys (hex) whose signature on `manifest.json` `speq upgrade` accepts. The release workflow signs
/// with the private half, the `SPEQ_RELEASE_SIGNING_KEY` secret of the `release` environment, and refuses to
/// publish when it does not match a key here. The manifest holds every binary's SHA-256, so whoever can replace
/// release assets still cannot get an upgrade installed. Rotation: add the new key, release, switch the secret,
/// and drop the old key a release later (docs/releasing.md).
pub const RELEASE_KEYS: &[&str] = &[
    // 2026-10: first release key.
    "09dc64d6b1ba01905999813dc4ffc6997574a153c8c90f5e4fd1facaf45a3d2d",
];

/// Debug builds only: one more trusted key (hex), so tests can sign releases of their own.
#[cfg(debug_assertions)]
pub const TEST_KEY_VARIABLE: &str = "SPEQ_RELEASE_TEST_KEY";

fn key_from_hex(hex: &str) -> Option<[u8; 32]> {
    let digits = hex.trim().as_bytes();
    if digits.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for (byte, pair) in key.iter_mut().zip(digits.chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(key)
}

/// The keys this build trusts for release manifests.
pub fn release_keys() -> Vec<[u8; 32]> {
    #[allow(unused_mut)]
    let mut keys: Vec<[u8; 32]> = RELEASE_KEYS
        .iter()
        .filter_map(|k| key_from_hex(k))
        .collect();
    #[cfg(debug_assertions)]
    keys.extend(
        std::env::var(TEST_KEY_VARIABLE)
            .ok()
            .and_then(|k| key_from_hex(&k)),
    );
    keys
}

/// Whether `signature` (64 bytes, Ed25519) signs exactly `message` with one of `keys`.
pub fn signed_by(message: &[u8], signature: &[u8], keys: &[[u8; 32]]) -> bool {
    use ring::signature::{ED25519, UnparsedPublicKey};
    signature.len() == 64
        && keys.iter().any(|key| {
            UnparsedPublicKey::new(&ED25519, key)
                .verify(message, signature)
                .is_ok()
        })
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    /// The bare executable, which is what installers and `upgrade` place on disk.
    pub binary: Asset,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseManifest {
    pub version: Version,
    pub commit: Option<String>,
    pub artifacts: BTreeMap<String, Artifact>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct CheckReport {
    pub current: String,
    pub latest: String,
    pub status: &'static str,
    pub target: &'static str,
}

#[derive(Debug, Serialize)]
pub struct UpgradeReport {
    pub from: String,
    pub to: String,
    pub installed: bool,
    pub path: Option<String>,
    /// Set when the new binary could not refresh the agent skills.
    pub skills_warning: Option<String>,
}

fn allowed_url(url: &Url) -> bool {
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    url.scheme() == "https" || (url.scheme() == "http" && loopback)
}

pub fn http_client() -> Result<reqwest::Client> {
    install_crypto_provider();
    reqwest::Client::builder()
        .user_agent(format!("speq-cli/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(10))
        // Release assets redirect to a CDN; follow only to https (or loopback in tests).
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if allowed_url(attempt.url()) && attempt.previous().len() < 5 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|e| Error::Other(format!("cannot build the HTTP client: {e}")))
}

pub fn manifest_url(base: &str, version: Option<&Version>) -> Result<Url> {
    let base = base.trim_end_matches('/');
    let raw = match version {
        None => format!("{base}/latest/download/manifest.json"),
        Some(v) => format!("{base}/download/v{v}/manifest.json"),
    };
    let url = Url::parse(&raw).map_err(|e| Error::invalid(format!("invalid release URL: {e}")))?;
    if !allowed_url(&url) {
        return Err(Error::invalid("the release URL must use https"));
    }
    Ok(url)
}

async fn get_limited(http: &reqwest::Client, url: &Url, limit: u64) -> Result<Vec<u8>> {
    let response = http.get(url.clone()).send().await.map_err(|e| {
        Error::Network(format!(
            "cannot download {}: {e}",
            url.host_str().unwrap_or("release")
        ))
    })?;
    if !response.status().is_success() {
        return Err(Error::Network(format!(
            "{} answered HTTP {}",
            url.host_str().unwrap_or("the release server"),
            response.status().as_u16()
        )));
    }
    if response.content_length().is_some_and(|n| n > limit) {
        return Err(Error::Other("the download is larger than expected".into()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| Error::Network(format!("download interrupted: {e}")))?;
    if bytes.len() as u64 > limit {
        return Err(Error::Other("the download is larger than expected".into()));
    }
    Ok(bytes.to_vec())
}

/// A network failure or an invalid manifest is an error, never "already up to date". The manifest must carry a
/// signature (`manifest.json.sig`, next to it) by one of `keys` before anything in it is believed.
pub async fn fetch_manifest(
    http: &reqwest::Client,
    url: &Url,
    keys: &[[u8; 32]],
) -> Result<ReleaseManifest> {
    let bytes = get_limited(http, url, MAX_MANIFEST_BYTES as u64).await?;
    let signature_url = Url::parse(&format!("{url}.sig"))
        .map_err(|e| Error::invalid(format!("invalid release URL: {e}")))?;
    let signature = get_limited(http, &signature_url, 64).await.map_err(|e| {
        Error::Other(format!(
            "the release has no signature for its manifest ({e}); nothing was changed"
        ))
    })?;
    if !signed_by(&bytes, &signature, keys) {
        return Err(Error::Other(
            "the release manifest is not signed with speq's release key; nothing was changed"
                .into(),
        ));
    }
    let manifest: ReleaseManifest = serde_json::from_slice(&bytes)
        .map_err(|e| Error::Other(format!("the release manifest is invalid: {e}")))?;
    for (target, artifact) in &manifest.artifacts {
        let url = Url::parse(&artifact.binary.url).map_err(|_| {
            Error::Other(format!(
                "the release manifest has an invalid URL for {target}"
            ))
        })?;
        let hash_ok = artifact.binary.sha256.len() == 64
            && artifact
                .binary
                .sha256
                .chars()
                .all(|c| c.is_ascii_hexdigit());
        if !allowed_url(&url)
            || !hash_ok
            || artifact.binary.size == 0
            || artifact.binary.size > MAX_BINARY_BYTES
        {
            return Err(Error::Other(format!(
                "the release manifest entry for {target} is invalid"
            )));
        }
    }
    Ok(manifest)
}

pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("package version is semver")
}

pub fn check(current: &Version, manifest: &ReleaseManifest) -> CheckReport {
    CheckReport {
        current: current.to_string(),
        latest: manifest.version.to_string(),
        status: if manifest.version > *current {
            "update-available"
        } else {
            "up-to-date"
        },
        target: CURRENT_TARGET,
    }
}

// ------------------------------------------------------------ installation

#[derive(Debug, PartialEq, Eq)]
pub enum Install {
    /// Placed by the official installer in the per-user directory: safe to replace.
    Official,
    /// Owned by a package manager: leave it alone and print its command.
    Managed {
        manager: &'static str,
        /// Updates speq.
        command: &'static str,
        /// Removes speq.
        uninstall: &'static str,
    },
    /// Anywhere else (a build tree, `/usr/local/bin`, ...): not ours to overwrite.
    Unmanaged,
}

pub fn official_dir(home: Option<&Path>, local_app_data: Option<&Path>) -> Option<PathBuf> {
    if cfg!(windows) {
        local_app_data.map(|p| p.join("Speq").join("bin"))
    } else {
        home.map(|h| h.join(".local").join("bin"))
    }
}

pub fn detect_install(exe: &Path, official: Option<&Path>) -> Install {
    let path = exe.to_string_lossy().replace('\\', "/").to_lowercase();
    const BREW: (&str, &str, &str) = ("Homebrew", "brew upgrade speq", "brew uninstall speq");
    const WINGET: (&str, &str, &str) = (
        "WinGet",
        "winget upgrade Dayatech.Speq",
        "winget uninstall Dayatech.Speq",
    );
    let managed = [
        ("/cellar/", BREW),
        ("/homebrew/", BREW),
        ("/linuxbrew/", BREW),
        (
            "/scoop/",
            ("Scoop", "scoop update speq", "scoop uninstall speq"),
        ),
        ("/winget/", WINGET),
        ("/microsoft/windowsapps/", WINGET),
        (
            "/nix/store/",
            ("Nix", "nix profile upgrade speq", "nix profile remove speq"),
        ),
    ];
    if let Some((_, (manager, command, uninstall))) =
        managed.iter().find(|(marker, _)| path.contains(marker))
    {
        return Install::Managed {
            manager,
            command,
            uninstall,
        };
    }
    // Compare canonical forms: the executable usually arrives canonicalized (`\\?\C:\...` on Windows), the official
    // folder as the OS reports it. A path that does not exist is compared as written.
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    match (exe.parent(), official) {
        (Some(parent), Some(dir)) if canonical(parent) == canonical(dir) => Install::Official,
        _ => Install::Unmanaged,
    }
}

/// The running executable (symlinks resolved) and who owns it.
pub fn current_install() -> Result<(PathBuf, Install)> {
    let exe = std::env::current_exe().ctx("locate the speq executable")?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let dirs = directories::BaseDirs::new();
    let official = official_dir(
        dirs.as_ref().map(|d| d.home_dir()),
        dirs.as_ref().map(|d| d.data_local_dir()),
    );
    let install = detect_install(&exe, official.as_deref());
    Ok((exe, install))
}

/// Download, verify (size and SHA-256), smoke-test, then atomically replace `exe`.
/// The previous binary stays usable if any step fails.
pub async fn install_binary(
    http: &reqwest::Client,
    asset: &Asset,
    exe: &Path,
    expected: &Version,
) -> Result<()> {
    let url = Url::parse(&asset.url).map_err(|_| Error::Other("invalid download URL".into()))?;
    let bytes = get_limited(http, &url, asset.size).await?;
    if bytes.len() as u64 != asset.size || !sha256_hex(&bytes).eq_ignore_ascii_case(&asset.sha256) {
        return Err(Error::Other(
            "the downloaded binary failed checksum verification; nothing was changed".into(),
        ));
    }
    let dir = exe
        .parent()
        .ok_or_else(|| Error::Other("the executable has no parent directory".into()))?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".speq-upgrade-")
        .tempfile_in(dir)
        .ctx(format!("create a temporary file in {}", dir.display()))?;
    tmp.write_all(&bytes).ctx("write the new binary")?;
    tmp.as_file().sync_all().ctx("sync the new binary")?;
    make_executable(&tmp)?;
    // Close the write handle before running the file: Linux refuses to execute a file that is still open
    // for writing ("Text file busy"). The path stays reserved, and is removed again unless we persist it.
    let staged = tmp.into_temp_path();
    smoke_test(&staged, expected)?;
    replace(staged, exe)
}

/// The new binary embeds the matching agent skills; let it install them. Returns a warning on failure,
/// because the upgrade itself already succeeded.
pub fn refresh_skills(exe: &Path) -> Option<String> {
    match std::process::Command::new(exe)
        .args(["skills", "install"])
        .output()
    {
        Ok(out) if out.status.success() => None,
        _ => Some(
            "the agent skills were not refreshed; run `speq skills install` to update them".into(),
        ),
    }
}

#[cfg(unix)]
fn make_executable(tmp: &NamedTempFile) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    tmp.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o755))
        .ctx("make the new binary executable")
}

#[cfg(not(unix))]
fn make_executable(_: &NamedTempFile) -> Result<()> {
    Ok(())
}

fn smoke_test(path: &Path, expected: &Version) -> Result<()> {
    // The file is closed, but another thread that forked while it was open can still make the first
    // exec report `ExecutableFileBusy` for a moment: retry briefly instead of failing a valid upgrade.
    let mut attempts = 0;
    let output = loop {
        match std::process::Command::new(path).arg("--version").output() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && attempts < 10 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(Error::Other(format!("the new binary does not run: {e}"))),
            Ok(output) => break output,
        }
    };
    let text = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !text.contains(&expected.to_string()) {
        return Err(Error::Other(
            "the new binary did not report the expected version; nothing was changed".into(),
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace(staged: tempfile::TempPath, exe: &Path) -> Result<()> {
    staged.persist(exe).map_err(|e| Error::Io {
        context: format!("replace {}", exe.display()),
        source: e.error,
    })
}

/// A running `.exe` cannot be overwritten on Windows, but it can be renamed away.
#[cfg(windows)]
fn replace(staged: tempfile::TempPath, exe: &Path) -> Result<()> {
    let old = exe.with_extension("exe.old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).ctx(format!("move the current binary aside ({})", exe.display()))?;
    match staged.persist(exe) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::rename(&old, exe);
            Err(Error::Io {
                context: format!("install {}", exe.display()),
                source: e.error,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `current_install` hands over the running executable canonicalized (`\\?\C:\...` on Windows, `/private/var/...`
    /// on macOS) and the official folder as the OS reports it: both forms must still match.
    #[test]
    fn an_official_install_is_recognised_when_only_the_executable_is_canonical() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("speq"), "").unwrap();
        let exe = std::fs::canonicalize(bin.join("speq")).unwrap();

        assert_eq!(detect_install(&exe, Some(&bin)), Install::Official);
    }

    #[test]
    fn detects_package_managers_and_official_installs() {
        let official = Path::new("/home/a/.local/bin");
        assert_eq!(
            detect_install(Path::new("/home/a/.local/bin/speq"), Some(official)),
            Install::Official
        );
        assert!(matches!(
            detect_install(
                Path::new("/opt/homebrew/Cellar/speq/1.0.0/bin/speq"),
                Some(official)
            ),
            Install::Managed {
                manager: "Homebrew",
                ..
            }
        ));
        assert!(matches!(
            detect_install(
                Path::new("C:\\Users\\a\\scoop\\apps\\speq\\current\\speq.exe"),
                None
            ),
            Install::Managed {
                manager: "Scoop",
                ..
            }
        ));
        assert_eq!(
            detect_install(
                Path::new("/work/speq-cli/target/debug/speq"),
                Some(official)
            ),
            Install::Unmanaged
        );
    }

    #[test]
    fn the_release_key_is_set() {
        // Without one, this build could never upgrade again: every signed manifest would be refused.
        assert!(!RELEASE_KEYS.is_empty(), "add the release public key");
        for key in RELEASE_KEYS {
            assert!(
                key_from_hex(key).is_some(),
                "{key:?} is not 32 bytes of hex"
            );
        }
    }

    #[test]
    fn only_a_signature_of_these_exact_bytes_by_a_trusted_key_counts() {
        use ring::signature::{Ed25519KeyPair, KeyPair};
        let signer = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let other = Ed25519KeyPair::from_seed_unchecked(&[8; 32]).unwrap();
        let key: [u8; 32] = signer.public_key().as_ref().try_into().unwrap();
        let manifest = br#"{"version":"9.9.9"}"#;
        let signature = signer.sign(manifest);

        assert!(signed_by(manifest, signature.as_ref(), &[key]));
        assert!(!signed_by(
            br#"{"version":"9.9.8"}"#,
            signature.as_ref(),
            &[key]
        ));
        assert!(!signed_by(manifest, other.sign(manifest).as_ref(), &[key]));
        assert!(!signed_by(manifest, signature.as_ref(), &[]));
        assert!(!signed_by(manifest, &signature.as_ref()[..63], &[key]));
    }

    #[test]
    fn keys_are_32_bytes_of_hex() {
        let hex = "0123456789abcdef".repeat(4);
        assert_eq!(key_from_hex(&hex).unwrap()[..2], [0x01, 0x23]);
        assert_eq!(
            key_from_hex(&format!(" {} \n", hex.to_uppercase())),
            key_from_hex(&hex)
        );
        for bad in ["", &hex[..62], &format!("{hex}00"), &"zz".repeat(32)] {
            assert_eq!(key_from_hex(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn release_urls() {
        let v = Version::parse("1.4.0").unwrap();
        assert_eq!(
            manifest_url("https://h/releases/", None).unwrap().as_str(),
            "https://h/releases/latest/download/manifest.json"
        );
        assert_eq!(
            manifest_url("https://h/releases", Some(&v))
                .unwrap()
                .as_str(),
            "https://h/releases/download/v1.4.0/manifest.json"
        );
        assert!(manifest_url("http://example.com/releases", None).is_err());
    }
}
