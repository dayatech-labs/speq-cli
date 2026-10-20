# Releasing the Speq CLI

Releases are automatic: one push to `main`, one commit, one workflow (`.github/workflows/release.yml`).

## How a release happens

Every push to `main` runs `.github/workflows/release.yml`. There is no manual tagging: the version and the tag come
from the commit messages since the last `vX.Y.Z` tag (`scripts/next_version.py`), using the conventional-commit
subjects this repository already requires (`<type>: <summary>`):

| Commits since the last tag | Next version |
| --- | --- |
| a breaking commit (`feat!: …`, or a `BREAKING CHANGE:` footer) | major (minor while the version is `0.x`) |
| at least one `feat` | minor |
| only `fix`, `perf`, `refactor` | patch |
| only `docs`, `chore`, `test`, `ci`, `build`, `style`, `revert` (or subjects without a type) | **no release** |

The first releasable commit publishes `v0.1.0`. Merging with a squash commit? Make the squash subject carry the type.

The checked-in `version` in `Cargo.toml` is only a placeholder: CI stamps the computed version into `Cargo.toml` and the
`speq` entry of `Cargo.lock` before building (`scripts/stamp_version.py`, not committed), so `speq --version` and the
manifest always show the released version. Local builds report the placeholder.

Before merging to `main`: the PR is green (fmt, clippy, tests on macOS/Linux-musl/Windows, MSRV, `cargo deny`, gitleaks,
installers), dependency changes were reviewed (`../dependency-review.md`), and the repository is **public** (installers
download anonymously). To suppress a release for a code change, use a non-releasing type (`chore:`/`docs:`).

## What the workflow does

1. **plan** — runs the release-script tests, computes `release`/`version`/`tag`; stops here when nothing is releasable or the
   tag already exists. Releases are serialised (`concurrency: release`) so two quick pushes never collide.
2. **verify** — fmt, clippy, deny, installer lint.
3. **build** (macOS ARM64, macOS Intel, Linux x86_64 musl, Windows x86_64 MSVC) — stamps the version, `cargo test`, release build
   with `SPEQ_COMMIT=<sha>`, smoke test `speq --version` (checks version, commit, target), package.
4. **publish** — `SHA256SUMS`, `manifest.json`, rendered Homebrew/Scoop/WinGet files, `install.sh`, `install.ps1`; creates a
   **draft** release, uploads everything, then publishes (`--latest`). The tag is created at that last step on the built commit,
   so a failed build leaves no tag, and users never see a partial release.
5. **verify-install** — installs the published release with the real installers on Linux, macOS (both archs), and Windows, then
   runs `--version`, `help`, `upgrade --check`.

A `workflow_dispatch` run re-evaluates the same rules on the current `main`; it cannot force a version. Dispatched on
any other branch, `plan` is skipped and so is every job after it: only `main` publishes.

Workflow supply chain (checked by `scripts/test_release_scripts.py`): every action is pinned to a full commit SHA, with
its tag in a comment; update both together. No `actions/checkout` keeps the token in `.git/config`
(`persist-credentials: false`); `publish` hands its write token only to `gh`.

Release assets (per version `V`, target `T`):

| Asset | Purpose |
| --- | --- |
| `speq-V-T[.exe]` | bare binary: what the installers and `speq upgrade` download |
| `speq-V-T.tar.gz` / `.zip` | archive for manual install and package managers |
| `SHA256SUMS` | checksums of every file; the installers find the right line from it |
| `manifest.json` | versioned manifest read by `speq upgrade` (version, commit, per-target URL/SHA-256/size) |
| `install.sh`, `install.ps1` | installers |
| `speq.rb`, `speq.scoop.json`, `Dayatech.Speq.yaml` | package-manager metadata with fixed-version URLs |

### Signing

`publish` signs `manifest.json` and `SHA256SUMS` with Ed25519 (`manifest.json.sig`, `SHA256SUMS.sig`: 64 raw bytes).
The private key is the `SPEQ_RELEASE_SIGNING_KEY` secret of the `release` environment (deployment branch `main`), and
the public key is in `RELEASE_KEYS` (`src/upgrade.rs`). `speq upgrade` believes a manifest, and the binary SHA-256s
in it, only with a valid signature by one of those keys, so whoever can replace release assets cannot get an upgrade
installed. `publish` refuses to sign with a key that is not in `RELEASE_KEYS`. The installers are downloaded from the
same release and do not check signatures; turn on immutable releases so assets cannot be replaced after publishing.

Create the key (OpenSSL 3; macOS's LibreSSL has no Ed25519), keep a copy in the team's password manager, and delete
the file:

```bash
openssl genpkey -algorithm ed25519 -out speq-release.pem
gh secret set SPEQ_RELEASE_SIGNING_KEY --repo dayatech-labs/speq-cli --env release < speq-release.pem
openssl pkey -in speq-release.pem -pubout -outform DER | tail -c 32 | xxd -p -c 32   # goes into RELEASE_KEYS
```

Rotate: add the new public key to `RELEASE_KEYS` and release (still signed with the old key), then switch the secret
to the new key; remove the old key from `RELEASE_KEYS` a release later. A lost or leaked key works the same way,
except the leaked key leaves `RELEASE_KEYS` at once.

Check a release by hand (OpenSSL 3), with the public key as PEM:

```bash
openssl pkeyutl -verify -pubin -inkey speq-release.pub.pem -rawin -in manifest.json -sigfile manifest.json.sig
```

## After release

* Homebrew: copy `speq.rb` into the tap repository (`brew install <tap>/speq`).
* Scoop / WinGet: submit `speq.scoop.json` / `Dayatech.Speq.yaml` to the bucket or `microsoft/winget-pkgs`.
* Pin/downgrade check: `speq upgrade --version <old> --yes`.

## Rollback

Releases are immutable by convention. To withdraw a bad release: mark it as pre-release or delete it on GitHub
(`latest` then points at the previous stable release; delete the tag too if the version number must be reused, otherwise the next
`fix:` commit publishes a fixed patch version). Installed users move with
`speq upgrade`; pinned users stay until they choose to move.
