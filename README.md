# Speq CLI

`speq` lets you work on your product's specs with the Git commands you already know. `speq clone` makes an ordinary
Git repository of the specs (in `specs/` of your implementation repository); after that `speq status`, `speq add`,
`speq commit`, `speq pull`, `speq push`, `speq log`, ... are Git itself, run in that repository. Fetches and pushes go
through the Speq API with your Speq login: you need no GitHub account and no GitHub credentials, and every push is
checked against your role. A pushed commit keeps its SHA, so your history and GitHub's are the same commits.

## Install

No Rust toolchain is needed. The installers put `speq` in a per-user folder (no `sudo`/admin), verify the
download against `SHA256SUMS`, and are safe to re-run. Next to `speq` they place `git-remote-speq` and
`git-credential-speq` (links on macOS/Linux, copies on Windows): the same binary, which Git runs under those names.

speq runs Git for you, so it needs **Git 2.28 or newer**. When Git is missing the installer offers to install it, and
always asks first. On macOS it uses `brew install git`, or `xcode-select --install`, which opens Apple's dialog. On Linux
it uses the distribution's package manager with `sudo`. On Windows it puts a private MinGit (Git for Windows'
portable build) in `%LOCALAPPDATA%\Speq\git` without admin rights, verified against the SHA-256 GitHub publishes for it.
Pass `--yes` (`-Yes`) to install Git without asking, or `--no-git` (`-NoGit`) to skip the check.

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

**Package managers.** Each release also ships a Homebrew formula (`speq.rb`, uses Git from macOS or Homebrew), a
Scoop manifest (`speq.scoop.json`), and a WinGet manifest (`Dayatech.Speq.yaml`, depends on `Git.Git`) pointing at
the same fixed-version URLs and checksums. All of them install the two Git helpers too. **Manual install:** download `speq-<version>-<target>.tar.gz` (or `.zip`) from the
[releases page](https://github.com/dayatech-labs/speq-cli/releases), verify it with `SHA256SUMS`, and put
`speq` and the two helpers from the archive on your `PATH` (speq recreates missing helpers next to itself when it can).

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

`uninstall` removes the stored credential, the cache, and the Claude Code skills, then the binary, its Git helpers, and (Windows) the private
Git if the official installer put them there. If `speq` is managed by Homebrew, Scoop, WinGet, or Nix, the binary is left alone
and the package manager's own command is printed (`brew uninstall speq`, `scoop uninstall speq`, ...). It never touches your specs repositories (`speq deinit` removes one), does not end the server-side session (use `logout --all` first), and
only removes the credential of the API origin in use (`--api-url` / `SPEQ_API_URL`). The installer's PATH line in your
shell profile (Unix) or the user PATH entry (Windows) is left for you to remove; the command points it out.

### Claude Code skills

The binary ships two [Claude Code](https://claude.com/claude-code) skills that know the specs layout, the IDs, and the
team's writing rules:

| Skill | Use it to |
| --- | --- |
| `/speq-write` | draft the next document from the ones before it: brief, PRD, stories, acceptance criteria, tasks, epic architecture (tech discovery), decision records |
| `/speq-review` | check a document before handoff or push: completeness, traceability (FR → AC → task), brief versions recorded in the PRD, and whether it reads like a professional team document rather than generic AI text |

The installers run `speq skills install`, which writes them to `~/.claude/skills/` (or `$CLAUDE_CONFIG_DIR/skills/`),
and `speq upgrade` refreshes them, so the skills always match the CLI version. After installing with Homebrew, Scoop,
WinGet, or by hand, run `speq skills install` yourself. Other skills in that folder are never touched.

## Quick start

```bash
speq login                              # approve this device in your browser
speq list                               # repositories you may use
cd my-frontend-repo
speq clone payment-specs                # ./specs, ignored by this repository's .gitignore
# ... edit files under specs/ ...
speq status                             # Git's own commands, run in ./specs
speq diff
speq commit -am "docs: add the refund rule"
speq push                               # checked by the API, lands on main with the same SHA
speq pull                               # rebases your unpushed commits on top of the remote
speq context epic-payment/st-001-create-payment   # files a coding agent should read for one story
```

A step-by-step guide with every supported command, the everyday flow, and what to do on a conflict is in
[docs/usage.md](docs/usage.md).

`speq help` lists speq's own commands; `speq <command> --help` shows usage, arguments, flags, examples, and exit codes.
`speq help <git command>` opens Git's manual. Add `--json` to speq's own commands for machine-readable output.

| Command | What it does |
| --- | --- |
| `login` / `logout [--all]` | PKCE device login; logout deletes the local token and cache. A token cannot be revoked on its own: `--all` ends every session of your account (all devices and the Web app) |
| `list` | repositories you can use, from the local capability cache while it is valid |
| `clone <repo> [dir]` | a Git repository of the specs, wired to the Speq API. `dir` defaults to `specs` inside a Git repository (and is added to its `.gitignore`), else to the repository name |
| `deinit [--yes]` | delete the specs repository (or a workspace of the older speq) to start over. Asks first on a terminal and names what would be lost: uncommitted changes, unpushed commits, stashes. The login and the `.gitignore` entry stay |
| `context <epic>/<story>` | deterministic list of context files for one story (`st-NNN-name` folder), from the working tree: `llms.txt`, root `README.md` and `architecture.md`, the epic's briefs, `prd.md`, `architecture.md`, then the story's `README.md`, `acceptance-criteria.md`, `DESIGN.md`, `layout.html`, and tasks. Only files that exist; paths relative to where you are |
| `push --squash [args]` | fold the commits not yet on the remote into one, then `git push [args]` (opt-in; see below) |
| `skills install` | write the `speq-write` and `speq-review` skills for Claude Code |
| `upgrade` / `uninstall` | check for / install a newer `speq` (and refresh the skills); remove local data, the skills, and an official install |
| **anything else** | Git: `speq <command> <args>` runs `git <command> <args>` in the specs repository you are in, or in `./specs` (from the implementation repository), with Git's exit code |

Text output of speq's own commands follows one template (`src/template.rs`): a title with an icon (`✔` done, `!` needs
attention, `ℹ` info, `✖` error), aligned facts, and a `→` next step. Colors appear only on a terminal and never when
`NO_COLOR` is set. Git's output is Git's own, untouched: pager, colors, and interactive commands work as in Git,
and every Git command exits with Git's code.

### What `speq clone` sets up

```text
speq.address             <api>/v1/projects/<id>              marks the folder as this project's specs repository
remote.origin.url        <api>/v1/projects/<id>/git          fetch: the API's read-only Git endpoint
remote.origin.pushurl    speq::<api>/v1/projects/<id>        push: git-remote-speq → POST /git/push
credential.<api>.helper  speq                                git-credential-speq hands Git your Speq token
user.name / user.email   your Speq account                   pushes must be authored and committed as you
core.autocrlf=false                                          documents keep their bytes (and blob SHAs)
pull.rebase=true, rebase.autoStash=true                      pulls rebase, so pushes rarely carry merges
merge.conflictStyle=diff3                                    conflicts show the base version too
```

Only `main` is fetched. If `origin` is changed or removed, speq still recognises the folder by `speq.address` and,
after every Git command, prints the commands that point `origin` back (see [docs/usage.md](docs/usage.md)). Because it
is plain Git, editors and other Git tools work too, such as VS Code's Source Control
and Sync. They run `git` directly, so they need `git-remote-speq` and `git-credential-speq` on your `PATH`, which the
installers take care of.

### Specs layout

Only these paths are documents you can push (the API enforces the same rules; anything else in the repository is
read-only through Speq and is refused before anything is sent):

```text
llms.txt, README.md, architecture.md
design/**.md and design/**.html
epics/<epic>/prd.md, architecture.md
epics/<epic>/briefs/br-NNN-<name>.md
epics/<epic>/decisions/dr-NN-<name>.md
epics/<epic>/stories/st-NNN-<name>/README.md, acceptance-criteria.md, DESIGN.md, layout.html
epics/<epic>/stories/st-NNN-<name>/tasks/<type>-NNN-<name>.md      (type: 2-8 lowercase letters, e.g. fe, be)
```

Documents must be UTF-8 text in regular files (no symlinks or executable bits).

## How pushing works

`speq push` is `git push`. Git asks `git-remote-speq` for the remote head and hands it the commits to send; the helper
sends each one as its raw object plus its changes. The API checks the whole push before writing anything, then
re-creates every commit on GitHub field for field, which gives it the same SHA, and moves `main` once:

* every commit must hash to its SHA, and be **authored and committed with your Speq account email** (`speq clone`
  sets it; fix the last commit with `speq commit --amend --no-edit --reset-author`);
* every change must be **allowed by your role**. A merge is judged by what it brings onto the parent already on the
  remote, never by other people's work it merges in. One denied path rejects the whole push (`403`), and nothing is
  written. The helper warns from the cached capabilities first; the API decides;
* the push must be a **fast-forward** of `main`. If someone pushed first you get Git's own `fetch first` /
  `non-fast-forward` rejection: `speq pull`, then `speq push` again;
* only `main` exists on the remote. Push a local branch with `speq push origin <branch>:main`. Force pushes, deletes,
  and tags are refused;
* at most **20 commits** per push. More than that is refused with a hint to run `speq push --squash`.

`speq push --squash` is opt-in. It refuses while anything is staged, and when your branch does not contain the latest
`origin/main` (so it can never undo someone else's work). Otherwise it replaces the commits not yet on the remote
with one commit (`Squash N commits` plus every subject) and pushes. It rewrites your local history like GitHub's
"Squash and merge"; the branch is left unchanged if the commit cannot be made.

A push whose response was lost can simply be run again: the API recognises commits it already has.

## Conflicts

They are Git's. `speq pull` rebases your unpushed commits onto the remote. Where both sides changed the same lines Git
stops with `CONFLICT` and writes markers (with the base version, `diff3`):

```text
<<<<<<< HEAD
the remote version
||||||| parent of 1a2b3c4 (docs: your commit)
the version both started from
=======
your version
>>>>>>> 1a2b3c4 (docs: your commit)
```

During a rebase `HEAD` is the remote side and the bottom block is your commit.

Resolve each block (VS Code's merge editor works), then `speq add <file>` and `speq rebase --continue`, or
`speq rebase --abort` to go back. `speq checkout --ours/--theirs <file>`, `speq restore`, `speq stash`, and the rest
work as in any Git repository. Prefer merges? `speq config pull.rebase false`; merge commits are pushed like any other.

## Security

* The Speq access token lives only in the OS credential store (Keychain, Credential Manager, Secret Service). It is
  signed, valid for 30 days, and there is no refresh: when it expires (or the API answers `401`) the CLI removes its
  local state, commands exit with code 3, and you run `speq login` again. Google and GitHub tokens never reach the CLI.
* The token never enters Git's configuration. `git-credential-speq` gives it to Git only for the API origin that
  issued it (the store is keyed by origin), and tells Git not to prompt when you are logged out. `speq clone`
  resets the helper list for that origin, so a system helper such as osxkeychain never stores the token.
* speq runs Git with the variables that redirect it elsewhere removed (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`,
  ...), so a command always acts on the specs repository speq found.
* The capability cache (projects, roles, expiry) is stored in the per-user application-data folder with owner-only
  permissions. It is never used after it expires and never authorizes anything: the API checks every request.
* Logs never contain tokens or document content. Set `SPEQ_LOG=debug` to see request method, path, status, and
  request ID.

## Exit codes

speq's own commands:

| Code | Meaning |
| --- | --- |
| 0 | success |
| 1 | unexpected failure (I/O, internal), or Git is missing |
| 2 | invalid usage |
| 3 | not logged in, or session expired/revoked |
| 4 | network or server unavailable, including rate limits |
| 5 | conflict (for example `push --squash` with staged changes) |
| 6 | permission denied by the API |
| 7 | invalid input or state (unknown repository, not a specs repository) |

Any other command is Git's and exits with Git's code.

## Troubleshooting

| Symptom | Cause and fix |
| --- | --- |
| `speq: command not found` after install | open a new terminal, or `export PATH="$HOME/.local/bin:$PATH"` (Windows: new terminal) |
| `Git is required` (exit 1) | install Git 2.28+ (the hint names the command), or re-run the installer |
| `Not a Speq specs repository` (exit 7) | run the command inside the folder `speq clone` made, or from the repository that contains it as `specs/`; else `speq clone <repo>` |
| `... is a workspace of an older speq` | this folder was set up by the pre-Git speq: push what you still need with that version, run `speq deinit` (removes `.speq/` and the specs folder), then `speq clone <repo>` |
| `credential store: ... Secret Service` on Linux | start a Secret Service provider (GNOME Keyring/KWallet) in your session; headless servers have none |
| `not logged in to <api>; run speq login` from Git | the session expired or you logged out: `speq login`, then retry |
| push rejected `(fetch first)` / `(non-fast-forward)` | someone pushed first: `speq pull`, resolve any conflict, `speq push` |
| push rejected `your role may not change every document in this push` | the lines starting `speq: not allowed:` name the paths; undo those changes (`speq restore --source origin/main <path>`, then amend) or ask a platform admin |
| push rejected `commits must be authored as ...` | `speq config user.email <your Speq email>` and `speq commit --amend --no-edit --reset-author`; for every unpushed commit `speq rebase origin/main --exec "git commit --amend --no-edit --reset-author"` |
| push rejected `too many commits` | `speq push --squash`, or push in steps with `speq push origin <commit>:main` |
| `git: 'remote-speq' is not a git command` when pushing from an editor | `git-remote-speq` is not on that program's `PATH`: re-run the installer, or push with `speq push` |
| `429`/rate-limited (exit 4), or `github_write_budget` | wait a minute and retry |
| `upgrade` says it was "not installed by the official installer" | reinstall with the installer, or use your package manager |
| `uninstall` reports the credential "could not be fully removed" | the OS credential store was unreachable; delete the `speq` entry there by hand (the token expires on its own) |

## Development

Untuk menjalankan CLI melawan API dan Web lokal, lihat [local-development.md](../local-development.md) (`export SPEQ_API_URL=http://localhost:8787`).

```bash
cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings
cargo test --locked          # unit tests + integration tests: real Git against an in-process fake API
cargo deny check             # advisories, licenses, bans, sources
```

Rust 1.89+ (edition 2024), Git 2.28+. Tests never touch the real credential store, scheduler, or network: they run the
library and the real binary (as `speq`, `git-remote-speq`, and `git-credential-speq`) against a stateful fake of the
API contract in `speq-api/contract/` whose projects are bare Git repositories (`tests/support/`). Debug builds read
the token from `SPEQ_DEV_CREDENTIAL_FILE` when it is set, so the binary can run as Git's helpers in tests; release
builds do not contain that code. See [docs/releasing.md](docs/releasing.md) for the release process.
