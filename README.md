# Speq CLI

`speq` lets you work on your product's specs with the Git commands you already know. `speq clone` makes an ordinary
Git repository of the specs (in `specs/` of your implementation repository); after that `speq status`, `speq add`,
`speq commit`, `speq pull`, `speq push`, `speq log`, ... are Git itself, run in that repository. Fetches and pushes go
straight to the project's GitHub repository over HTTPS, with your own GitHub credentials (`gh auth login` or Git
Credential Manager): your GitHub account needs access to the repository (a project admin invites you), and GitHub decides
who may push. Speq's role and path checks are not applied to these pushes for now; the Web app still applies them to its
edits. Your commits keep their SHAs: history is Git's own. You sign
in to Speq (`speq login`) to list and clone repositories, and for `context`, `impact`, `ui`, and `dayaboard`.

## Install

No Rust toolchain is needed. The installers put `speq` in a per-user folder (no `sudo`/admin), verify the
download against `SHA256SUMS`, and are safe to re-run. Next to `speq` they place `git-remote-speq` and
`git-credential-speq` (links on macOS/Linux, copies on Windows): the same binary, which Git runs under those names. Only
repositories cloned with an earlier speq need them (see [Repositories cloned with an earlier speq](#repositories-cloned-with-an-earlier-speq)).

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

`upgrade` first checks the release manifest's Ed25519 signature against the release keys built into speq, then
verifies the binary's size and SHA-256 from that manifest, runs `--version` on the new binary, and swaps it in
atomically; any failure keeps the old binary. It only replaces binaries the official installer put in the folders above. If `speq`
is managed by Homebrew, Scoop, WinGet, or Nix it stops and prints the package manager's own command
(`brew upgrade speq`, `scoop update speq`, ...). It never runs on its own during `login`, `pull`, or any other command.

### Uninstall

```bash
speq logout --all    # optional: also end this account's sessions on the server and in the Web app
speq uninstall       # asks for confirmation on a terminal
speq uninstall --yes # for automation
```

`uninstall` removes the stored credential, the cache, and the agent skill, then the binary, its Git helpers, and (Windows) the private
Git if the official installer put them there. If `speq` is managed by Homebrew, Scoop, WinGet, or Nix, the binary is left alone
and the package manager's own command is printed (`brew uninstall speq`, `scoop uninstall speq`, ...). It never touches your specs repositories (`speq deinit` removes one), does not end the server-side session (use `logout --all` first), and
only removes the credential of the API origin in use (`--api-url` / `SPEQ_API_URL`). The installer's PATH line in your
shell profile (Unix) or the user PATH entry (Windows) is left for you to remove; the command points it out.

### Skills for AI coding agents

The binary ships five [Agent Skills](https://agentskills.io) skills.

`speq-cli` knows how to run speq on a specs repository: always `speq`, never `git`, from the implementation
repository; one short-lived local branch per piece of work, created from `origin/main`; the push rules; resolving a
`speq pull` conflict; fixing each rejected-push reason; and `speq context` before implementing an epic, story, or task. It asks before
anything that reaches other people or loses work (commit, push, `deinit`, discarding changes) and never force-pushes.

`speq-write` knows the specs layout, the IDs, the `version` / `briefs` front matter the Web app reads, and each role's
write permissions (read live from `speq list --json`). Ask your agent to use it to:

* draft the next document from the ones before it: brief, PRD, stories, acceptance criteria, tasks, epic architecture
  (tech discovery), decision records;
* review an existing document, or check whether a story is ready for engineering;
* set up the first documents of a new product, or move existing `docs/` from an implementation repository into Speq.

It also keeps `development-status.md` in the specs root current: every story of every epic, old and new, with its
status worked out from the `status` of its tasks (a story is `DONE` once all its tasks are), a table of epics linking to
each epic's section and its `prd.md`, and a `last-update` time.
The agent rebuilds it whenever a story or task is added, removed, or changes status.

`speq-dayaboard` turns an epic, its stories, and their tasks into Dayaboard tickets (Epic, Story, Task) through
`speq dayaboard`, with the Dayaboard API key `speq login` stored. It finds tickets that already exist, asks for the
estimates the specs leave open, and creates nothing before you confirm the plan. Each document then carries its ticket
key in the front matter (`ticket-id: DEV-42`), and each ticket names its document and the specs commit it reflects, so
a later update changes only what changed in the spec. Its API reference is
https://dayaboard.odyssey.co.id/llms-full.txt.

`speq-impact` runs before an acceptance criterion or business rule changes. It reads `speq impact` and reports
the documents that state and cite the ID, the tasks that implement it (`DONE` ones to reopen) with their tickets, and
the tests that claim it. It edits nothing.

`speq-interview` takes a raw request from a product owner or client (chat, `.docx`, `.xlsx`), skips what the specs
already answer, and interviews you with multiple-choice questions on the problem, roles and access, screen states,
and business rules. It then hands the answers to `speq-write` for the brief or PRD.

`speq-ui-design`, `speq-ui-flow`, and `speq-ui-cli` are for Speq UI prototypes (a `<product>-ui` repository of HTML +
Tailwind screens). `speq-ui-design` holds the rules for the files: one screen per file, `data-component`,
`data-persona` and `data-state` variants in the same screen, `personas.json`, and the design language in `DESIGN.md`. `speq-ui-flow` runs the whole flow in
three gated phases: draft on a design canvas, review and change it with the user, and only when asked export it as
`pages/`, `personas.json` and (when wanted) `canvas/` and `DESIGN.md` and push. `speq-ui-cli` reads a prototype with `speq ui get`, `speq ui personas` and `speq ui design`, and
publishes to the UI repository with plain `git` after you confirm the push.

No skill commits or pushes unless you ask. The installers run `speq skills install`, which writes all eight to
`~/.agents/skills/` (Codex, Cursor, Copilot, Gemini CLI, OpenCode) and `~/.claude/skills/` (Claude Code; or
`$CLAUDE_CONFIG_DIR/skills/`), and `speq upgrade` refreshes them, so the skills always match the CLI version. After
installing with Homebrew, Scoop, WinGet, or by hand, run `speq skills install` yourself. Other skills in those folders
are never touched; the `speq-review` skill of earlier versions is removed. `speq skills install` also keeps a short
block, between `<!-- speq:begin` and `<!-- speq:end -->`, in the global instruction file of each agent installed on
the machine, so the agent refreshes `development-status.md` in every session, not only when a skill is loaded:

| Agent | File |
| --- | --- |
| Claude Code | `~/.claude/CLAUDE.md` (or `$CLAUDE_CONFIG_DIR/CLAUDE.md`) |
| Codex | `~/.codex/AGENTS.md` (or `$CODEX_HOME/AGENTS.md`) |
| Gemini CLI | `~/.gemini/GEMINI.md` |
| OpenCode | `~/.config/opencode/AGENTS.md` (or `$XDG_CONFIG_HOME/opencode/AGENTS.md`) |
| Copilot CLI | `~/.copilot/copilot-instructions.md` (or `$COPILOT_HOME/copilot-instructions.md`) |

An agent whose folder does not exist gets no file. Cursor keeps global rules in Settings > Rules, not in a file: paste
the block there by hand. The rest of those files is never changed, a newer `speq skills install` replaces the block,
and `speq uninstall` removes only the block.

## Quick start

```bash
speq login                              # approve this device in your browser
speq list                               # repositories you may use
speq info                               # who you are signed in as, Dayaboard, and your roles
cd my-frontend-repo
speq clone payment-specs                # ./specs, ignored by this repository's .gitignore
# ... edit files under specs/ ...
speq status                             # Git's own commands, run in ./specs
speq diff
speq commit -am "docs: add the refund rule"
speq push                               # checked by the API, lands on main with the same SHA
speq pull                               # rebases your unpushed commits on top of the remote
speq context epic-payment/st-001-create-payment   # files a coding agent should read for one story
speq context epic-payment/st-001-create-payment/be-001-api   # ... for one task (or `speq context epic-payment` for an epic)
speq ui get https://speq.example/ui/ui_1/checkout/payment  # HTML of a UI prototype page, from its Web app link
speq ui personas https://speq.example/ui/ui_1               # the mock personas a UI product declares
speq ui design https://speq.example/ui/ui_1                 # DESIGN.md of a UI product: its design language
```

A step-by-step guide with every supported command, the everyday flow, and what to do on a conflict is in
[docs/usage.md](docs/usage.md).

`speq help` lists speq's own commands; `speq <command> --help` shows usage, arguments, flags, examples, and exit codes.
`speq help <git command>` opens Git's manual. Add `--json` to speq's own commands for machine-readable output.

| Command | What it does |
| --- | --- |
| `login` / `logout [--all]` | PKCE device login; logout deletes the local token and cache. A token cannot be revoked on its own: `--all` ends every session of your account (all devices and the Web app) |
| `list` | repositories you can use, from the local capability cache while it is valid |
| `clone <repo> [dir]` | a Git repository of the specs, cloned from the project's GitHub repository (needs your GitHub access). `dir` defaults to `specs` inside a Git repository (and is added to its `.gitignore`), else to the repository name |
| `deinit [--yes]` | delete the specs repository (or a workspace of the older speq) to start over. Asks first on a terminal and names what would be lost: uncommitted changes, unpushed commits, stashes. The login and the `.gitignore` entry stay |
| `context <epic>[/<story>[/<task>]]` | deterministic list of context files, from the working tree: `llms.txt`, root `README.md` and `architecture.md`, the epic's briefs, `prd.md`, `architecture.md`, then for `<epic>` every story (by folder name) with its tasks; for `<epic>/<story>` (`st-NNN-name` folder) the story's `README.md`, `acceptance-criteria.md`, `DESIGN.md`, `layout.html`, and tasks; for `<epic>/<story>/<task>` (task file name, `.md` optional) the same story files and only that task. Only files that exist; paths relative to where you are |
| `impact <epic>/<story> AC-NN` / `impact <epic> BR-NN [--code DIR]...` | read-only, from the working tree: where the ID is written, the epic's documents that cite it, the criteria it covers, the tasks naming them with `status` and `ticket-id` (`DONE` ones flagged to reopen), and the code lines that name the story and criterion together, such as `(st-001 AC-04)`, in each `--code` repository or the one around `./specs`, plus the criteria no code line names |
| `ui get <url>` | read-only: the HTML of one UI prototype page of a UI product's `<product>-ui` repository, from its link in the Speq Web app (`<speq-web>/ui/<ui-product>/<page>`, optionally `#component=<id>`, or just the path), read through the API with your session; only links of the Web app you are signed in to are accepted. Prints the page alone (control characters other than line breaks and tabs shown as spaces, invisible characters as `<U+XXXX>` markers; `--json` keeps the exact text, written with `\uXXXX` escapes, and adds the commit it was read at). A selected component is named on stderr; the whole page is printed. A query string in the link (`?persona=admin&state=empty`) only changes the Web preview and is ignored. The page holds every persona variant (`data-persona="a b"`) and state (`data-state="empty"`). A UI product you cannot open, or a page that is not there: exit code 7 |
| `ui personas <url>` | read-only: the mock personas a UI product declares in `personas.json` (id, name, start page, the page patterns they see, note), from any link of the product: `<speq-web>/ui/<ui-product>[/<page>]`, a query is ignored. Same origin check, session and exit codes as `ui get`; text from the repository is neutralised the same way (`--json` keeps it exact with `\uXXXX` escapes). A `personas.json` that is invalid or too large is noted on stderr and lists nothing (`problem` in `--json`) |
| `ui design <url>` | read-only: the text of `DESIGN.md` at the root of the UI repository (the design language of the prototype), as it is on stdout so it can be redirected to a file, from any link of the product (`<speq-web>/ui/<ui-product>[/<page>]`, query and fragment ignored). Same origin check, session and exit codes as `ui get`; the text is neutralised the same way (`--json` keeps it exact: `ui_product_id`, `head_sha`, `problem` when set, `content`). No `DESIGN.md`, one that is too large (over 256 KiB) or not UTF-8: nothing on stdout, a `note:` on stderr, exit 0 (`content` is null in `--json`). Nothing printed and no note means an empty file |
| `push` | Git's push to GitHub. Before it, speq warns, without stopping, when the commits change an acceptance criterion or a PRD business rule a `DONE` task still names |
| `push --squash [args]` | fold the commits not yet on the remote into one, then `git push [args]` (opt-in; see below) |
| `skills install` | write the `speq-cli`, `speq-dayaboard`, `speq-impact`, `speq-interview`, `speq-ui-cli`, `speq-ui-design`, `speq-ui-flow`, and `speq-write` skills for AI coding agents, and the speq block in their `CLAUDE.md` / `AGENTS.md` |
| `upgrade` / `uninstall` | check for / install a newer `speq` (and refresh the skills); remove local data, the skills, and an official install |
| **anything else** | Git: `speq <command> <args>` runs `git <command> <args>` in the specs repository you are in, or in `./specs` (from the implementation repository), with Git's exit code |

Text output of speq's own commands follows one template (`src/template.rs`): a title with an icon (`✔` done, `!` needs
attention, `ℹ` info, `✖` error), aligned facts, and a `→` next step. Colors appear only on a terminal and never when
`NO_COLOR` is set. Git's output is Git's own, untouched: pager, colors, and interactive commands work as in Git,
and every Git command exits with Git's code.

### What `speq clone` sets up

```text
speq.address             <api>/v1/projects/<id>                  marks the folder as this project's specs repository
speq.repository          <owner>/<repo>                          the project's GitHub repository
remote.origin.url        https://github.com/<owner>/<repo>.git   fetch and push, with your own GitHub credentials
user.name / user.email   your Speq account                       commits are authored as you
core.autocrlf=false                                              documents keep their bytes (and blob SHAs)
core.symlinks=false                                              a link on the remote is checked out as a plain file
pull.rebase=true, rebase.autoStash=true                          pulls rebase, so pushes rarely carry merges
merge.conflictStyle=diff3                                        conflicts show the base version too
```

speq sets no credential helper: Git signs in to GitHub the way it does for any repository, with what you already
use (`gh auth login`, Git Credential Manager, the macOS keychain, or an SSH-to-HTTPS token). speq never reads or
stores GitHub credentials. When the first fetch fails, `speq clone` says so, leaves nothing behind, and names what is
missing: access to the repository for your GitHub account (a project admin adds you, and you accept the GitHub invitation)
and a Git sign-in to GitHub.

Only `main` is fetched. If `origin` is changed or removed, speq still recognises the folder by `speq.address` and,
after every Git command, prints the commands that point `origin` back (see [docs/usage.md](docs/usage.md)). Because it
is plain Git, editors and other Git tools work too, such as VS Code's Source Control and Sync.

### Specs layout

These paths are the documents of a specs repository: what the Speq Web app and API accept and show. A direct push to
GitHub is not checked against this list or against roles, so keep to it; anything else in the repository is read-only in
the Web app:

```text
llms.txt, development-status.md, README.md, architecture.md
design/**.md and design/**.html
epics/<epic>/prd.md, architecture.md
epics/<epic>/briefs/br-NNN-<name>.md
epics/<epic>/decisions/dr-NN-<name>.md
epics/<epic>/stories/st-NNN-<name>/README.md, acceptance-criteria.md, DESIGN.md, layout.html
epics/<epic>/stories/st-NNN-<name>/tasks/<type>-NNN-<name>.md      (type: 2-8 lowercase letters, e.g. fe, be)
```

Documents must be UTF-8 text in regular files (no symlinks or executable bits).

## How pushing works

`speq push` is `git push` to the project's GitHub repository. Nothing passes through the Speq API:

* **GitHub decides who may push.** Your GitHub account needs write access to the repository; without it GitHub answers
  with its own `Permission denied` or `Authentication failed`.
* **Fast-forward of `main`.** If someone pushed first you get Git's own `fetch first` / `non-fast-forward` rejection:
  `speq pull`, then `speq push` again. Push a local branch with `speq push origin <branch>:main`.
* **Your commits are pushed as they are**, with the SHAs you made.
* **Not checked by speq for now:** your role per document path (the Web app enforces it: everyone except the Product
  Owner may change every document, and the Product Owner writes the briefs), author and committer equal to your Speq email, document
  paths only, the 20-commit limit, and the ban on force pushes, deletes, and tags. Turn on branch protection for `main`
  on GitHub (no force pushes, no deletion) so GitHub refuses the destructive ones. Before a push speq only warns,
  without stopping, when the commits change an acceptance criterion or a PRD business rule that a `DONE` task still
  names. The Web app and the API still check roles for edits made there, and the API's webhook keeps its view of `main`
  current after your push.

`speq push --squash` is opt-in. It refuses while anything is staged, and when your branch does not contain the latest
`origin/main` (so it can never undo someone else's work). Otherwise it replaces the commits not yet on the remote
with one commit (`Squash N commits` plus every subject) and pushes. It rewrites your local history like GitHub's
"Squash and merge"; the branch is left unchanged if the commit cannot be made.

### Repositories cloned with an earlier speq

A repository made by an earlier speq has no `speq.repository`. Its `origin` fetches from the Speq API's read-only Git
endpoint (`<api>/v1/projects/<id>/git`) and pushes through `git-remote-speq` (push URL `speq::<api>/v1/projects/<id>`,
with `git-credential-speq` as credential helper), and it keeps working as it did. **This way is being retired:** new clones
never use it, and a later speq will stop supporting it, so move such a repository to GitHub directly (see the end of this
section). Those pushes work like this:

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

A push whose response was lost can simply be run again: the API recognises commits it already has.

To move such a repository to GitHub directly, push what you want to keep, `speq deinit`, then `speq clone` again.

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
* When Dayaboard is available, `speq login` also stores a personal Dayaboard API key (for creating tickets) next to the
  token, in the same credential entry. The Speq API obtains it with the browser's fresh Google sign-in and passes it
  on once without keeping it. It expires with the token, is sent only to the Dayaboard origin that issued it, and is
  revoked when this device logs out, signs in again, or uninstalls. If Dayaboard cannot be connected, login still
  succeeds with a warning.
* **GitHub credentials stay with Git.** A repository cloned by this version has no credential helper of speq's: Git signs
  in to GitHub with the helper or key you already use, and speq neither reads nor stores those credentials. The Speq
  access token is never given to Git or sent to GitHub.
* In a repository cloned with an earlier speq, the token never enters Git's configuration, and Git never gets it. `git-credential-speq` trades it for a Git fetch
  token (`POST git/token`) and gives Git only that: valid for 10 minutes, only for fetching the one project Git
  names, and ended by `logout --all` too. Any program you run could ask the helper, so it never hands out the
  token that pushes and signs you out everywhere, and it answers only for the API origin that issued the token (the
  store is keyed by origin). It tells Git not to prompt when you are logged out. `speq clone` resets the helper list
  for that origin, so a system helper such as osxkeychain never stores a token. A repository cloned before fetch
  tokens gets `useHttpPath` from its next `speq` command.
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
| `git fetch from github.com/<owner>/<repo> failed` when cloning | your GitHub account has no access to the repository, or Git is not signed in to GitHub: ask a project admin to add you with write access, and run `gh auth login` (or sign in through Git Credential Manager); then `speq clone` again |
| `Permission denied` or `Authentication failed` from GitHub when pushing | same cause: write access and a Git sign-in to GitHub are both needed |
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
| `429`/rate-limited (exit 4), or `github_write_budget` | wait a minute and retry (the Speq API only; direct pushes use GitHub's own limits) |
| `upgrade` says it was "not installed by the official installer" | reinstall with the installer, or use your package manager |
| `uninstall` reports the credential "could not be fully removed" | the OS credential store was unreachable; delete the `speq` entry there by hand (the token expires on its own) |

The rows about roles, the author email, too many commits, `remote-speq`, and `not logged in ... from Git` only concern
repositories cloned with an earlier speq, whose pushes go through the Speq API.

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
