# Speq CLI

`speq` brings your product's specs repository into the `specs/` folder of your implementation
repository, shows what changed on either side, and sends your edits back through the Speq API.
It never talks to GitHub directly and never needs GitHub credentials.

## Install

No Rust toolchain is needed. The installers put `speq` in a per-user folder (no `sudo`/admin), verify the
download against `SHA256SUMS`, and are safe to re-run.

```bash
# macOS (Apple Silicon or Intel) and Linux (x86_64)
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/dayatech-labs/speq-cli/releases/latest/download/install.sh | sh

# pin a version (e.g. for company deployments)
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/dayatech-labs/speq-cli/releases/download/v0.1.0/install.sh | sh -s -- --version 0.1.0
```

```powershell
# Windows PowerShell (x86_64)
irm https://github.com/dayatech-labs/speq-cli/releases/latest/download/install.ps1 | iex

# pin a version
& ([scriptblock]::Create((irm https://github.com/dayatech-labs/speq-cli/releases/download/v0.1.0/install.ps1))) -Version 0.1.0
```

| OS | Installed to | Notes |
| --- | --- | --- |
| macOS, Linux | `~/.local/bin/speq` | the folder is added to your shell profile once if it is missing from `PATH` |
| Windows | `%LOCALAPPDATA%\Speq\bin\speq.exe` | the folder is added to your user `PATH` once |

Reload your shell (or open a new terminal) if `speq` is not found right after installing.

**Package managers.** Each release also ships a Homebrew formula (`speq.rb`), a Scoop manifest
(`speq.scoop.json`), and a WinGet manifest (`Dayatech.Speq.yaml`) pointing at the same fixed-version
URLs and checksums. **Manual install:** download `speq-<version>-<target>.tar.gz` (or `.zip`) from the
[releases page](https://github.com/dayatech-labs/speq-cli/releases), verify it with `SHA256SUMS`, and put
the `speq` binary on your `PATH`.

Targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-musl`, `x86_64-pc-windows-msvc`.
Other systems are refused with a link to the archives; the installer never guesses.

### Upgrade, downgrade, pin

```bash
speq upgrade --check            # installed vs. latest; exit 0 either way, non-zero if the check itself fails
speq upgrade                    # asks for confirmation on a terminal
speq upgrade --yes              # for automation
speq upgrade --version 0.1.0 --yes   # pin or downgrade
```

`upgrade` verifies size and SHA-256, runs `--version` on the new binary, then swaps it in atomically; any failure
keeps the old binary. It only replaces binaries the official installer put in the folders above. If `speq`
is managed by Homebrew, Scoop, WinGet, or Nix it stops and prints the package manager's own command
(`brew upgrade speq`, `scoop update speq`, ...). It never runs on its own during `login`, `pull`, or any other command.

### Uninstall

```bash
speq logout --all    # optional: also end this account's sessions on the server and in the Web app
speq uninstall       # asks for confirmation on a terminal
speq uninstall --yes # for automation
```

`uninstall` removes the stored credential and the cache, then the binary itself if the
official installer put it there. If `speq` is managed by Homebrew, Scoop, WinGet, or Nix, the binary is left alone
and the package manager's own command is printed (`brew uninstall speq`, `scoop uninstall speq`, ...). It never
touches your workspaces (`specs/` and `.speq/`; see `deinit` below), does not end the server-side session (use `logout --all` first), and
only removes the credential of the API origin in use (`--api-url` / `SPEQ_API_URL`). The installer's PATH line in your
shell profile (Unix) or the user PATH entry (Windows) is left for you to remove; the command points it out.

## Quick start

```bash
speq login                              # approve this device in your browser
speq list                               # repositories you may use
cd my-frontend-repo
speq init payment-specs # repo or project name works when it is unique; otherwise owner/repo
speq pull                               # downloads every specs document into ./specs
# ... edit files under specs/ ...
speq status                             # unchanged / local-only / remote-only / conflict / deleted
speq diff epics/epic-payment/prd.md           # against the latest remote (add --base to compare offline)
speq push                               # one batch through the API
speq context epic-payment/st-001-create-payment   # files a coding agent should read for one story, one per line
```

`speq help` lists every command; `speq <command> --help` shows usage, arguments, flags, examples, and exit codes.
Add `--json` to any command for machine-readable output (errors are JSON on stderr).

Text output follows one template (`src/template.rs`): a title with an icon (`✔` done, `!` needs attention, `ℹ` info, `✖` error), aligned facts, grouped lists, and a `→` next step. Colors appear only on a terminal and never when `NO_COLOR` is set. `speq context` is the exception: it prints bare paths, one per line, for scripts and coding agents.

| Command | What it does |
| --- | --- |
| `login` / `logout [--all]` | PKCE device login; logout deletes the local token and cache. A token cannot be revoked on its own: `--all` ends every session of your account (all devices and the Web app) |
| `list` | repositories you can use, from the local capability cache while it is valid |
| `init <repo>` | writes `.speq/config.toml`, creates `specs/`, adds `specs/` and `.speq/` to `.gitignore`; re-init needs `--reconfigure` |
| `deinit [--yes]` | delete this workspace's `.speq/` and specs folder, including unpushed edits; asks first on a terminal. Leaves `.gitignore` and your login alone |
| `pull` | full-project sync; never overwrites local work |
| `status` | local vs. last sync vs. latest remote, per path |
| `diff <path> [--base]` | unified diff; `--base` needs no network |
| `update <path>` | apply the latest remote version of one file, only when it has no local changes |
| `resolve <path>... \| --all [--ours \| --theirs]` | finish conflicts after a pull, offline: keep the file you edited, restore your version (`--ours`) or take the remote's (`--theirs`, discards your edits; asks first). See [Resolving conflicts](#resolving-conflicts) |
| `push` | send local changes as one batch; refused while the workspace has conflicts |
| `context <epic>/<story>` | deterministic list of context files for one story (`st-NNN-name` folder), from the local copy: `llms.txt`, root `README.md` and `architecture.md`, the epic's briefs, `prd.md`, `architecture.md`, then the story's `README.md`, `acceptance-criteria.md`, `DESIGN.md`, `layout.html`, and tasks. Only files that exist |
| `upgrade` | check for / install a newer `speq` binary |
| `uninstall` | remove local data and, for an official install, the binary |

### Specs layout

Only these paths are documents `speq` syncs (the API enforces the same rules; anything else under `specs/` is ignored and listed by `status`):

```text
llms.txt, README.md, architecture.md
design/**.md and design/**.html
epics/<epic>/prd.md, architecture.md
epics/<epic>/briefs/br-NNN-<name>.md
epics/<epic>/decisions/dr-NN-<name>.md
epics/<epic>/stories/st-NNN-<name>/README.md, acceptance-criteria.md, DESIGN.md, layout.html
epics/<epic>/stories/st-NNN-<name>/tasks/<type>-NNN-<name>.md      (type: 2-8 lowercase letters, e.g. fe, be)
```

HTML documents sync, diff, merge, and resolve line by line exactly like Markdown. The older layout
(`product-brief.md`, `features/<feature>/...`) is no longer recognised.

### How sync stays safe

* `pull` compares three versions of each file (remote, last synced baseline, your copy). Remote-only changes are
  applied; local-only edits are kept; a file changed on both sides, a remote delete over a local edit, or a remote
  add over an untracked file is a **conflict**. The lock (`.speq/lock.json`) only advances when the whole manifest
  reconciled without conflicts, and `push` is refused until then. Nothing of yours is dropped: a file changed on both
  sides is merged line by line like Git (see below); a delete conflict is left exactly as it is.
* Every file is fetched at the same commit as the manifest and checked against its Git blob SHA. If the
  repository moves mid-pull, the attempt is discarded and repeated (3 tries), with nothing changed locally.
* Files are written through a temporary file and an atomic rename. Symbolic links and paths outside the specs
  folder are never followed.
* The only way to discard local edits is `resolve --theirs` (asks first) or `deinit`. `push` never merges and never retries a write automatically; after a
  rejection use `status`, `diff`, `pull`. Whether you may write a path is always decided by the API.

### Workspace files

```
.speq/config.toml      project mapping                                     ┐
.speq/lock.json        last fully synced commit, unresolved conflicts      │
.speq/manifest.json    per-path baseline hashes                            │ the whole .speq/ is
.speq/base/            exact copies of the last applied remote content     │ ignored by Git
.speq/conflict/        both versions of each unresolved conflict           │
specs/                   the working copy                                  ┘
```

## Security

* The Speq access token lives only in the OS credential store (Keychain, Credential Manager, Secret
  Service). It is signed, valid for 30 days, and there is no refresh: when it expires (or the API answers `401`) the
  CLI removes its local state, commands exit with code 3, and you run `speq login` again. Google and GitHub tokens
  never reach the CLI.
* The capability cache (projects, roles, expiry) is stored in the per-user application-data folder with owner-only
  permissions. It is never used after it expires and never authorizes anything: the API checks every request.
* The CLI runs no background process. When the cache has expired, the next command refreshes it in the foreground.
* Logs never contain tokens or document content. Set `SPEQ_LOG=debug` to see request method, path, status, and
  request ID.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | success |
| 1 | unexpected failure (I/O, internal) |
| 2 | invalid usage |
| 3 | not logged in, or session expired/revoked |
| 4 | network or server unavailable, including rate limits |
| 5 | conflict: unresolved conflicts, rejected push, workspace not clean |
| 6 | permission denied by the API |
| 7 | invalid input or workspace state (unknown path, not initialized, ambiguous repository) |

## Resolving conflicts

When a file changed on both sides, `speq pull` merges it line by line, the way `git merge` does:

* **Different lines** (yours and the remote's do not touch): both changes are combined. The file is reported as
  *merged* and is now your edit on top of the remote, so `speq status` shows `local-only`. Review it with
  `speq diff <path>` and `speq push`. There is nothing to resolve.
* **The same lines** (or lines that touch): only that region is wrapped in markers; the rest of the file is merged.

```text
unchanged and merged lines...
<<<<<<< local
your version of this region
||||||| base
the version you last synced
=======
the remote version of this region
>>>>>>> remote
more merged lines...
```

VS Code (and most editors) open this layout in a merge editor: for each block pick **Accept Current** (yours),
**Accept Incoming** (remote) or **Both**, or edit it by hand, then save. Remove every marker line; a file that still
has one stays a conflict, and pulling again never touches it.

1. `speq pull` — the merge is written; push is blocked while any conflict is open.
2. Resolve each block in the editor and save.
3. `speq resolve <path>` (or `speq resolve --all`) — your saved file is kept as a local change on top of the remote
   version it was merged against. It works offline, skips any file that still has markers, and when the last
   conflict is done it moves the lock so `speq push` is open. Without a side, `--all` only takes files that had
   markers; a delete on one side is listed as *needs a decision* and must be named (see below). `speq pull` recognises the same saved file, so running
   it again also works.
4. `speq push` when no conflicts remain.

Prefer not to merge by hand? Take a whole side, per file or for everything:

```bash
speq resolve prd.md --ours       # your version from before the merge
speq resolve prd.md --theirs     # the remote version; your edits to the file are discarded (asks first)
speq resolve --all --theirs --yes
```

`pull` keeps both versions of every conflict in `.speq/conflict/` until it is resolved, so `--ours` and `--theirs`
work offline and survive the merge rewriting your file. After `--theirs` there is no copy of your edits left.

If the remote moved on after the markers were written, `speq pull` does not accept the stale resolution: the file is
merged again against the new remote. `speq pull` also notes that the saved remote copy is out of date, so
`speq resolve` still finishes the file but does not move the lock; run `speq pull` once more to bring the file and
the lock up to date (`speq status` shows the file as `remote-only` until then). A remote change you have not seen is
never overwritten, and the API rejects a stale `push` (`document_changed`, nothing is committed). Apart from
`--theirs`, local content is never discarded.

Details worth knowing:

* Lines are compared exactly, including line endings. A missing final newline is kept as it is outside a marker
  block, and added inside one so each marker stays on its own line.
* A file that is not valid UTF-8 text cannot be split into lines; it gets one block around the whole file.
* A remote add over an untracked local file has no baseline, so the whole file is one block.
* A delete on one side against an edit on the other has nothing to mark: the file is left as it is. Decide with
  `speq diff <path>`, then `speq resolve <path>` to keep your side (your file, or your delete) or `--theirs` to take
  the remote's. `speq resolve --all` never decides these for you unless you give it `--ours` or `--theirs`.
* A `=======` line left behind in a file `pull` marked counts as an unfinished marker, unless the original document
  already had that many of them (a Markdown heading underline). If your resolution really adds a new `=======`
  underline, write the heading another way (`# Title` or `-------`).
* Deleting a file that `pull` marked does not lose your version: the copy from before the merge stays in
  `.speq/conflict/`, and `speq resolve <path> --ours` brings the file back.
* A marker is only recognised by its specific label lines (`<<<<<<< local`, `||||||| base`, `>>>>>>> remote`); a
  document that legitimately contains one of those lines cannot be resolved this way.

## Troubleshooting

| Symptom | Cause and fix |
| --- | --- |
| `speq: command not found` after install | open a new terminal, or `export PATH="$HOME/.local/bin:$PATH"` (Windows: new terminal) |
| `credential store: ... Secret Service` on Linux | start a Secret Service provider (GNOME Keyring/KWallet) in your session; headless servers have none |
| `error: not logged in` (exit 3) | run `speq login`. A revoked or expired session also lands here after local state is cleared |
| `pull` reports conflicts | files marked "conflict markers in the file": resolve each block in your editor, save, then `speq resolve <path>` or `speq resolve --all` (see [Resolving conflicts](#resolving-conflicts)). Other conflicts (a delete on one side): `speq diff <path>`, then `speq resolve <path>` (keep yours) or `--theirs` |
| `push` is refused: "workspace is not clean" | a previous pull left conflicts or never completed: `speq status`, `speq resolve`, or `speq pull` when it never completed |
| `push` rejected with `head_changed` / `document_changed` (exit 5) | someone changed the specs first; nothing was committed: `speq pull`, re-check, push again |
| `push` rejected with `policy_denied` (exit 6) | your role may not write that path; ask a platform admin. `list` refreshes after the denial |
| "the remote kept changing while pulling" | the repository moved during all 3 attempts; nothing changed locally, retry |
| `429`/rate-limited (exit 4) | reads are retried automatically a few times; wait a minute and retry |
| `another speq command is already running in this workspace` | wait for it to finish; it is a per-workspace lock |
| `upgrade` says it was "not installed by the official installer" | reinstall with the installer, or use your package manager |
| `uninstall` reports the credential "could not be fully removed" | the OS credential store was unreachable; delete the `speq` entry there by hand (the token expires on its own) |

## Development

Untuk menjalankan CLI melawan API dan Web lokal, lihat [local-development.md](../local-development.md) (`export SPEQ_API_URL=http://localhost:8787`).

```bash
cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings
cargo test --locked          # unit tests + integration tests against an in-process fake API
cargo deny check             # advisories, licenses, bans, sources
```

Rust 1.89+ (edition 2024). Tests never touch the real credential store, scheduler, or network: they run the
library against a stateful fake of the API contract in `speq-api/contract/` (`tests/support/`). See
[docs/releasing.md](docs/releasing.md) for the release process.
