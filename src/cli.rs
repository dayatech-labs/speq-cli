//! The Clap command tree. `speq help` and `speq <command> --help` are generated from here.
use clap::builder::styling::{AnsiColor, Effects, Style, Styles};
use clap::{Args, Parser, Subcommand};

/// Help colors. Clap only emits them on a terminal; `NO_COLOR` and pipes get plain text.
pub const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Yellow.on_default());

pub const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\ncommit: ",
    env!("SPEQ_COMMIT"),
    "\ntarget: ",
    env!("SPEQ_TARGET")
);

const EXIT_CODES: &str = "\
Exit codes:
  0  success
  1  unexpected failure (I/O, internal, Git missing)
  2  invalid usage
  3  not logged in or session expired
  4  network or server unavailable (including rate limits)
  5  conflict
  6  permission denied by the API
  7  invalid input or state (unknown repository, not a specs repository)
Any other command is Git's, and exits with Git's code.";

/// The Git commands speq users need most, the push rules, and the first steps: below the command list of `speq help`.
const TOP_HELP: &str = "\
Git commands:
  speq status | diff | log | show          see what changed, and who changed it
  speq add | restore | rm | mv             stage changes, or undo them
  speq commit                              record staged changes, authored as your Speq account
  speq pull | fetch                        get the latest specs (pull rebases your commits on top)
  speq push                                send your commits to main, checked against your role
  speq push --squash                       fold the commits not yet pushed into one, then push
  speq switch | branch | stash | rebase    local work, exactly as in Git
  speq merge | reset | checkout | blame    ...and any other Git command, with Git's flags
  speq help <git command>                  Git's manual for that command

Git commands run in the specs repository you are in, or in ./specs (or --specs-dir / $SPEQ_SPECS_DIR) from the
repository around it.
Pushes go to main only: no force push, no tags or other branches, at most 20 commits, documents only.

Environment (set once instead of passing the flag every time):
  SPEQ_SPECS_DIR    folder of the specs, default `specs`; `.` for a plain clone of <product>-specs from GitHub
  SPEQ_API_URL      Speq API origin, for example http://localhost:8787

Get started:
  speq login
  speq list
  speq clone <repo>
  speq status
  speq commit -am \"docs: ...\" && speq push";

#[derive(Parser, Debug)]
#[command(
    name = "speq",
    styles = STYLES,
    version = env!("CARGO_PKG_VERSION"),
    long_version = LONG_VERSION,
    about = "Work on your product specs with the Git commands you already know",
    long_about = "speq clones a product's specs repository as an ordinary Git repository and runs Git for you: \
                  `speq status`, `speq add`, `speq commit`, `speq pull`, `speq push`, `speq log`, ... are Git's own \
                  commands. Fetches and pushes go through the Speq API with your Speq login, so no GitHub account or \
                  credentials are needed, and every push is checked against your role.",
    after_help = TOP_HELP,
    arg_required_else_help = true
)]
pub struct Cli {
    /// Print machine-readable JSON instead of text
    #[arg(long, global = true)]
    pub json: bool,

    /// Speq API origin (https only; http is allowed for localhost)
    #[arg(long, global = true, env = "SPEQ_API_URL", value_name = "URL")]
    pub api_url: Option<String>,

    /// Folder of the specs, inside the repository you are in or above it (default: `specs`; `.` is this folder)
    #[arg(long, global = true, env = "SPEQ_SPECS_DIR", value_name = "DIR")]
    pub specs_dir: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Log in to Speq in your browser
    #[command(
        after_help = "Examples:\n  speq login\n  speq login --no-browser\n\nThe access token is kept in the OS credential store and expires after 30 days. There is no refresh: when it\nexpires, commands fail with exit code 3 and you run `speq login` again.\n\nWhen Dayaboard is available, login also stores a personal Dayaboard API key for creating tickets. It expires with\nthe token; if Dayaboard cannot be connected, login still succeeds with a warning.\n\n"
    )]
    Login(LoginArgs),

    /// Delete the local credential and cache (--all also ends every other device)
    #[command(
        after_help = "Examples:\n  speq logout\n  speq logout --all\n\nA token cannot be revoked on its own. Plain `logout` forgets it on this machine only; --all ends every token\nyour account holds, on every device and in the Web app. This device's Dayaboard API key is revoked either way; keys\nof other devices expire with their tokens. Specs repositories are left untouched.\n\n"
    )]
    Logout(LogoutArgs),

    /// List the specs repositories you are allowed to use
    #[command(
        after_help = "Examples:\n  speq list\n  speq list --json\n\nShown from the local capability cache while it is valid; refreshed automatically after it expires.\n\n"
    )]
    List,

    /// Clone a specs repository from `speq list` as a Git repository wired to Speq
    #[command(
        after_help = "Arguments:\n  <REPO>  `owner/repo` from `speq list`, or the repo name or project name when it is unique\n  [DIR]   folder to create (default: `specs`, or --specs-dir, inside a Git repository, which is then ignored there;
          else the repo name)\n\nExamples:\n  speq clone payment-specs\n  speq clone acme/payment-specs docs/specs\n\nAfterwards use Git's commands through speq: speq status, speq add, speq commit, speq pull, speq push, speq log.\nPulls rebase, line endings are kept byte for byte, and commits are authored with your Speq account email.\n\n"
    )]
    Clone(CloneArgs),

    /// Delete the specs repository (or an older speq workspace) to start over
    #[command(
        after_help = "Examples:\n  speq deinit\n  speq deinit --yes\n\nThe inverse of `speq clone`: deletes the specs repository you are in, or ./specs from the repository around it,\nwith its Git history. Changes that were not committed or not pushed, and stashes, are lost: the confirmation names\nthem, and `speq push` first keeps them. A workspace of the older speq (`.speq/` and its specs folder) is removed the\nsame way. Your login and the `.gitignore` entry stay, so `speq clone <repo>` starts again right away.\n\n"
    )]
    Deinit(DeinitArgs),

    /// Print the files a coding agent should read for one epic, story, or task
    #[command(
        after_help = "Arguments:\n  <TARGET>  `<epic>`, `<epic>/<story>` or `<epic>/<story>/<task>`: the epic folder, the story folder, and the\n            task file name (`.md` optional), for example epic-payment/st-001-create-payment/be-001-api\n\nExamples:\n  speq context epic-payment\n  speq context epic-payment/st-001-create-payment\n  speq context epic-payment/st-001-create-payment/be-001-api\n  speq context epic-payment/st-001-create-payment --json\n\nPrints one path per line (relative to this directory) in a fixed order, from the working tree (no network): llms.txt,\nREADME.md, architecture.md, the epic's briefs, prd.md and architecture.md, then\n  <epic>                  every story of the epic, by folder name, each with its tasks\n  <epic>/<story>          the story's README.md, acceptance-criteria.md, DESIGN.md, layout.html and tasks\n  <epic>/<story>/<task>   the story's README.md, acceptance-criteria.md, DESIGN.md, layout.html and that task only\nOnly files that exist are listed.\n\nWorks in a plain clone of the specs repository from GitHub too, with `speq --specs-dir . context <TARGET>` run in its root.\n\n"
    )]
    Context(ContextArgs),

    /// Install the Speq skills for AI coding agents: using the CLI, and drafting and reviewing spec documents
    #[command(
        after_help = "Examples:\n  speq skills install\n  speq skills install --json\n\nWrites the speq-cli skill (using speq: pull, branches, push, conflicts, rejected pushes, story context) and the\nspeq-write skill (drafting and reviewing spec documents), in Agent Skills format, to ~/.agents/skills, read by\nCodex, Cursor, Copilot, Gemini CLI and OpenCode, and to ~/.claude/skills (or $CLAUDE_CONFIG_DIR/skills) for\nClaude Code. Older copies are replaced; other skills there are never touched. The installers and `speq upgrade`\nrun this for you; run it yourself after installing speq with a package manager. `speq uninstall` removes the\nskills. Needs no login.\n\n"
    )]
    Skills(SkillsArgs),

    /// Check for or install a newer speq binary
    #[command(
        after_help = "Examples:\n  speq upgrade --check\n  speq upgrade\n  speq upgrade --yes\n  speq upgrade --version 1.4.0 --yes\n\n--check needs no login and changes nothing. Binaries managed by Homebrew, Scoop, WinGet, or another\npackage manager are never replaced; the matching package-manager command is shown instead.\n\n"
    )]
    Upgrade(UpgradeArgs),

    /// Remove speq's local data and, when the installer put it there, the binary
    #[command(
        after_help = "Examples:\n  speq uninstall\n  speq uninstall --yes\n\nRemoves the stored credential and the cache, then the binary if it was placed by the\nofficial installer. Binaries managed by Homebrew, Scoop, WinGet, or another package manager are left in place and\nthe matching package-manager command is shown. Specs repositories are never touched (see `speq deinit`), and the\nserver-side session is not ended: run `speq logout --all` first for that. The Dayaboard API key is revoked. Only the credential of the current API\norigin is removed.\n\n"
    )]
    Uninstall(UninstallArgs),
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Command::Login(_) => "login",
            Command::Logout(_) => "logout",
            Command::List => "list",
            Command::Clone(_) => "clone",
            Command::Deinit(_) => "deinit",
            Command::Context(_) => "context",
            Command::Skills(_) => "skills",
            Command::Upgrade(_) => "upgrade",
            Command::Uninstall(_) => "uninstall",
        }
    }
}

#[derive(Args, Debug)]
pub struct LoginArgs {
    /// Print the approval URL without opening a browser
    #[arg(long)]
    pub no_browser: bool,
}

#[derive(Args, Debug)]
pub struct LogoutArgs {
    /// Also end every other session of this account (all devices and the Web app)
    #[arg(long)]
    pub all: bool,
}

#[derive(Args, Debug)]
pub struct CloneArgs {
    /// Repository: `owner/repo`, or a repo or project name that is unique in `speq list`
    pub repo: String,
    /// Folder to create
    pub dir: Option<String>,
}

#[derive(Args, Debug)]
pub struct DeinitArgs {
    /// Do not ask for confirmation
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct ContextArgs {
    /// `<epic>`, `<epic>/<story>` or `<epic>/<story>/<task>`, for example `epic-payment/st-001-create-payment`
    pub target: String,
}

#[derive(Args, Debug)]
pub struct SkillsArgs {
    #[command(subcommand)]
    pub action: SkillsAction,
}

#[derive(Subcommand, Debug)]
pub enum SkillsAction {
    /// Write the skills into the agents' skills folders, replacing older copies
    Install,
}

#[derive(Args, Debug)]
pub struct UpgradeArgs {
    /// Only report the installed and latest versions
    #[arg(long)]
    pub check: bool,
    /// Do not ask for confirmation
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Install this exact version (also allows downgrades)
    #[arg(long, value_name = "VERSION")]
    pub version: Option<String>,
}

#[derive(Args, Debug)]
pub struct UninstallArgs {
    /// Do not ask for confirmation
    #[arg(long, short = 'y')]
    pub yes: bool,
}

/// Exit-code table, exposed so docs and tests can reuse it.
pub fn exit_code_help() -> &'static str {
    EXIT_CODES
}

/// Paints the hand-written help text (`after_help`) with the same palette as [`STYLES`]:
/// section headings, example commands, `<ARGS>`, and exit-code numbers.
pub fn paint_help(text: &str) -> String {
    const HEADINGS: [&str; 6] = [
        "Examples:",
        "Arguments:",
        "Categories:",
        "Exit codes:",
        "Get started:",
        "Git commands:",
    ];
    let header = STYLES.get_header();
    let literal = STYLES.get_literal();
    let placeholder = STYLES.get_placeholder();
    let wrap = |style: &Style, s: &str| format!("{style}{s}{style:#}");

    let mut section = "";
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            if let Some(h) = HEADINGS.iter().find(|h| line.starts_with(**h)) {
                section = h;
                return format!("{}{}", wrap(header, h), &line[h.len()..]);
            }
            if line.trim().is_empty() {
                section = "";
                return line.to_string();
            }
            let Some(body) = line.strip_prefix("  ") else {
                return line.to_string();
            };
            match section {
                "Examples:" | "Get started:" if body.starts_with("speq ") => {
                    format!("  {}", wrap(literal, body))
                }
                // The command is painted; its description, after the column gap, stays plain.
                "Git commands:" if body.starts_with("speq ") => {
                    let end = body.find("  ").unwrap_or(body.len());
                    format!("  {}{}", wrap(literal, &body[..end]), &body[end..])
                }
                "Arguments:" if body.starts_with('<') => {
                    let end = body.find('>').map_or(body.len(), |i| i + 1);
                    format!("  {}{}", wrap(placeholder, &body[..end]), &body[end..])
                }
                "Exit codes:" => {
                    let end = body.find(' ').unwrap_or(body.len());
                    format!("  {}{}", wrap(literal, &body[..end]), &body[end..])
                }
                _ => line.to_string(),
            }
        })
        .collect();
    lines.join("\n")
}

const BANNER: [&str; 5] = [
    r" ____  ____  _____  ___      ____ _     ___ ",
    r"/ ___||  _ \| ____|/ _ \    / ___| |   |_ _|",
    r"\___ \| |_) |  _| | | | |  | |   | |    | | ",
    r" ___) |  __/| |___| |_| |  | |___| |___ | | ",
    r"|____/|_|   |_____|\__\_\   \____|_____|___|",
];

/// The `SPEQ CLI` logo shown above the top-level help. Clap drops the colors when output is not a terminal.
pub fn banner() -> String {
    let style = AnsiColor::Cyan.on_default().effects(Effects::BOLD);
    BANNER
        .iter()
        .map(|line| format!("{style}{line}{style:#}"))
        .collect::<Vec<_>>()
        .join("\n")
}
