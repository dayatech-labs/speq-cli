//! Dispatch from the command line: speq's own commands go to the library, everything else to Git.
use crate::cli::{Cli, Command, SkillsAction, banner, exit_code_help, paint_help};
use crate::clone::{self, CloneOptions};
use crate::context;
use crate::deinit;
use crate::env::Env;
use crate::error::{Error, Result};
use crate::git::Git;
use crate::links;
use crate::output::emit;
use crate::repo::{self, SpecsRepo};
use crate::session::Session;
use crate::skills;
use crate::squash;
use crate::uninstall;
use crate::upgrade::{self, Install};
use clap::{CommandFactory, FromArgMatches};
use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

/// Commands speq answers itself; any other command is Git's.
pub const SPEQ_COMMANDS: [&str; 9] = [
    "login",
    "logout",
    "list",
    "clone",
    "deinit",
    "context",
    "skills",
    "upgrade",
    "uninstall",
];

/// Where a command line goes.
#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    /// One of [`SPEQ_COMMANDS`], help, or version: parsed by Clap.
    Speq,
    /// `git <args>` in the specs repository.
    Git(Vec<OsString>),
    /// `speq push --squash [push args]`: squash, then `git push <push args>`.
    SquashPush(Vec<OsString>),
    /// `speq help <git command>`: Git's manual, needs no repository.
    GitHelp(Vec<OsString>),
}

/// The value of speq's leading `--specs-dir` flag, which `route` strips before the rest goes to Git.
pub fn leading_specs_dir(args: &[OsString]) -> Option<String> {
    let mut rest = args;
    while let Some(a) = rest.first().and_then(|a| a.to_str()) {
        match a {
            "--json" => rest = &rest[1..],
            "--api-url" => rest = rest.get(2..).unwrap_or(&[]),
            "--specs-dir" => return rest.get(1)?.to_str().map(str::to_string),
            _ if a.starts_with("--api-url=") => rest = &rest[1..],
            _ => return a.strip_prefix("--specs-dir=").map(str::to_string),
        }
    }
    None
}

/// Decides from the arguments after the program name. speq's global flags (`--json`, `--api-url`, `--specs-dir`) may
/// come first; anything else that starts with `-` is a Git option (`-C`, `-c`, `--no-pager`) and goes to Git with
/// the rest.
pub fn route(args: &[OsString]) -> Route {
    let mut rest = args;
    loop {
        match rest.first().and_then(|a| a.to_str()) {
            Some("--json") => rest = &rest[1..],
            Some("--api-url" | "--specs-dir") => rest = rest.get(2..).unwrap_or(&[]),
            Some(a) if a.starts_with("--api-url=") || a.starts_with("--specs-dir=") => {
                rest = &rest[1..]
            }
            _ => break,
        }
    }
    let first = rest.first().and_then(|a| a.to_str());
    match first {
        None | Some("help" | "-h" | "--help" | "-V" | "--version") => {
            let topic = rest.get(1).and_then(|a| a.to_str());
            match (first, topic) {
                (Some("help"), Some(topic)) if !SPEQ_COMMANDS.contains(&topic) => {
                    Route::GitHelp(rest.to_vec())
                }
                _ => Route::Speq,
            }
        }
        Some(command) if SPEQ_COMMANDS.contains(&command) => Route::Speq,
        Some("push") if rest.iter().any(|a| a == "--squash") => {
            Route::SquashPush(rest.iter().filter(|a| *a != "--squash").cloned().collect())
        }
        _ => Route::Git(rest.to_vec()),
    }
}

/// The directory of the running binary: Git finds speq's helpers there.
fn helper_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Best effort: a missing helper only matters for fetch/push, and `speq clone` reports it clearly.
    let _ = links::ensure(&exe);
    exe.parent().map(Path::to_path_buf)
}

fn current_dir() -> Result<PathBuf> {
    std::env::current_dir().map_err(|e| Error::Io {
        context: "read the current directory".into(),
        source: e,
    })
}

/// After a Git command, says how to restore `origin` if it no longer points at the project: checked afterwards, so
/// the command that changed it is answered at once and the one that fixes it stays quiet.
fn warn_about_origin(git: &Git, repo: &SpecsRepo) {
    if let Some(commands) = repo.origin_repair(git) {
        eprintln!(
            "speq: origin no longer points to this Speq project, so fetch, pull, and push may fail. Restore it with:"
        );
        for command in commands {
            eprintln!("  {command}");
        }
    }
}

/// Runs a Git route and returns Git's exit code.
pub fn run_git(route: Route, specs_dir: &str) -> Result<u8> {
    let git = Git::locate(helper_dir())?;
    let cwd = current_dir()?;
    match route {
        Route::GitHelp(args) => git.passthrough(None, args),
        Route::Git(args) => {
            let repo = SpecsRepo::discover(&git, &cwd, specs_dir)?;
            repo.ensure_credential_path(&git);
            let code = git.passthrough(repo.git_dir_for(&cwd).as_deref(), args);
            warn_about_origin(&git, &repo);
            code
        }
        Route::SquashPush(args) => {
            let repo = SpecsRepo::discover(&git, &cwd, specs_dir)?;
            repo.ensure_credential_path(&git);
            let dir = repo.git_dir_for(&cwd);
            let squashed = squash::squash(&git, dir.as_deref())?;
            if squashed > 1 {
                eprintln!("speq: squashed {squashed} commits into one");
            }
            let code = git.passthrough(dir.as_deref(), args);
            warn_about_origin(&git, &repo);
            code
        }
        Route::Speq => Err(Error::Other("not a Git command".into())),
    }
}

/// The Clap command with the shared exit-code table appended to every command's help.
pub fn build_command() -> clap::Command {
    let codes = exit_code_help();
    let mut cmd = Cli::command();
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect();
    for name in names {
        cmd = cmd.mut_subcommand(name, |sub| {
            let existing = sub
                .get_after_help()
                .map(|s| s.to_string())
                .unwrap_or_default();
            sub.after_help(paint_help(&format!(
                "{}{codes}",
                if existing.is_empty() {
                    String::new()
                } else {
                    format!("{}\n\n", existing.trim_end())
                }
            )))
        });
    }
    let top = cmd
        .get_after_help()
        .map(|s| s.to_string())
        .unwrap_or_default();
    cmd.before_help(banner())
        .after_help(paint_help(&format!("{top}\n\n{codes}")))
}

pub fn parse_args<I: IntoIterator<Item = String>>(
    args: I,
) -> std::result::Result<Cli, clap::Error> {
    let matches = build_command().try_get_matches_from(args)?;
    Cli::from_arg_matches(&matches)
}

fn confirm(prompt: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Err(Error::invalid(
            "confirmation needs a terminal; pass --yes to proceed without asking",
        ));
    }
    eprint!("{prompt} [y/N] ");
    std::io::stderr().flush().ok();
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|e| Error::Io {
            context: "read the answer".into(),
            source: e,
        })?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub async fn run(cli: Cli, env: &Env) -> Result<u8> {
    let json = cli.json;
    let specs_dir = repo::specs_dir(cli.specs_dir.as_deref());
    match cli.command {
        Command::Login(_) => Ok(emit(json, &Session::new(env).login().await?)),
        Command::Logout(args) => Ok(emit(json, &Session::new(env).logout(args.all).await?)),
        Command::List => Ok(emit(json, &clone::list(env).await?)),
        Command::Clone(args) => {
            let git = Git::locate(helper_dir())?;
            let options = CloneOptions {
                repo: &args.repo,
                dir: args.dir.as_deref(),
                default_dir: &specs_dir,
            };
            Ok(emit(
                json,
                &clone::clone(env, &git, &current_dir()?, options).await?,
            ))
        }
        Command::Deinit(args) => {
            let git = Git::locate(None)?;
            let cwd = current_dir()?;
            let target = deinit::find(&git, &cwd, &specs_dir)?;
            if !args.yes {
                let folders = deinit::folders(&target)
                    .iter()
                    .map(|f| f.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" and ");
                let lost = match &target {
                    deinit::Target::Repo(repo) => match deinit::at_risk(&git, &repo.root)? {
                        risk if risk.is_empty() => {
                            "Everything in it was pushed; nothing else is lost.".to_string()
                        }
                        risk => format!("This loses {}.", risk.describe()),
                    },
                    deinit::Target::Legacy { .. } => {
                        "Local edits that were never pushed are lost.".to_string()
                    }
                };
                if !confirm(&format!("Delete {folders}? {lost}"))? {
                    return Err(Error::invalid("cancelled; nothing was changed"));
                }
            }
            Ok(emit(json, &deinit::deinit(&git, &target, &cwd)?))
        }
        Command::Context(args) => {
            let git = Git::locate(None)?;
            let cwd = current_dir()?;
            let (root, project_id) = SpecsRepo::docs_root(&git, &cwd, &specs_dir)?;
            Ok(emit(
                json,
                &context::context(&git, &root, project_id, &cwd, &args.target)?,
            ))
        }
        Command::Skills(args) => match args.action {
            SkillsAction::Install => Ok(emit(json, &skills::install(&env.skills_dirs)?)),
        },
        Command::Upgrade(args) => upgrade_command(json, args).await,
        Command::Uninstall(args) => uninstall_command(json, env, args),
    }
}

async fn upgrade_command(json: bool, args: crate::cli::UpgradeArgs) -> Result<u8> {
    let base = std::env::var("SPEQ_RELEASE_URL")
        .unwrap_or_else(|_| upgrade::DEFAULT_RELEASE_BASE.to_string());
    let pinned = args
        .version
        .as_deref()
        .map(|v| {
            semver::Version::parse(v.trim_start_matches('v'))
                .map_err(|_| Error::invalid(format!("{v:?} is not a valid version")))
        })
        .transpose()?;
    let http = upgrade::http_client()?;
    let manifest = upgrade::fetch_manifest(
        &http,
        &upgrade::manifest_url(&base, pinned.as_ref())?,
        &upgrade::release_keys(),
    )
    .await?;
    let current = upgrade::current_version();
    let report = upgrade::check(&current, &manifest);

    if args.check {
        return Ok(emit(json, &report));
    }

    let (exe, install) = upgrade::current_install()?;
    match install {
        Install::Managed {
            manager, command, ..
        } => {
            return Err(Error::invalid(format!(
                "speq is managed by {manager}; update it with `{command}` (nothing was changed)"
            )));
        }
        Install::Unmanaged => {
            return Err(Error::invalid(format!(
                "{} was not installed by the official installer, so `speq upgrade` will not replace it; reinstall with the installer or your package manager",
                exe.display()
            )));
        }
        Install::Official => {}
    }

    let target = pinned.clone().unwrap_or_else(|| manifest.version.clone());
    if pinned.is_none() && manifest.version <= current {
        return Ok(emit(
            json,
            &upgrade::UpgradeReport {
                from: current.to_string(),
                to: current.to_string(),
                installed: false,
                path: None,
                skills_warning: None,
            },
        ));
    }
    if pinned.as_ref() == Some(&current) {
        return Ok(emit(
            json,
            &upgrade::UpgradeReport {
                from: current.to_string(),
                to: current.to_string(),
                installed: false,
                path: None,
                skills_warning: None,
            },
        ));
    }
    let artifact = manifest.artifacts.get(upgrade::CURRENT_TARGET).ok_or_else(|| Error::invalid(format!("this release has no build for {}; download an archive manually from the release page", upgrade::CURRENT_TARGET)))?;
    if !args.yes && !confirm(&format!("Install speq {target} (currently {current})?"))? {
        return Err(Error::invalid("cancelled; nothing was changed"));
    }
    upgrade::install_binary(&http, &artifact.binary, &exe, &target).await?;
    // On Windows the helpers are copies of the binary: bring them up to date with it.
    links::ensure(&exe);
    Ok(emit(
        json,
        &upgrade::UpgradeReport {
            from: current.to_string(),
            to: target.to_string(),
            installed: true,
            path: Some(exe.display().to_string()),
            skills_warning: upgrade::refresh_skills(&exe),
        },
    ))
}

fn uninstall_command(json: bool, env: &Env, args: crate::cli::UninstallArgs) -> Result<u8> {
    let (exe, install) = upgrade::current_install()?;
    if !args.yes {
        let binary = match install {
            Install::Official => format!(" and {}", exe.display()),
            _ => String::new(),
        };
        let prompt = format!(
            "Uninstall speq? This removes your local credentials, cached data, and agent skills{binary}"
        );
        if !confirm(&prompt)? {
            return Err(Error::invalid("cancelled; nothing was changed"));
        }
    }
    Ok(emit(json, &uninstall::uninstall(env, &exe, &install)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn every_command_clap_knows_is_routed_to_speq_and_nothing_else() {
        let mut clap: Vec<String> = build_command()
            .get_subcommands()
            .map(|c| c.get_name().to_string())
            .filter(|name| name != "help")
            .collect();
        let mut routed: Vec<String> = SPEQ_COMMANDS.iter().map(|c| c.to_string()).collect();
        clap.sort();
        routed.sort();
        assert_eq!(clap, routed);
    }

    #[test]
    fn speq_commands_help_and_version_stay_with_speq() {
        for line in [
            &[][..],
            &["login"],
            &["--json", "list"],
            &["--api-url", "http://localhost:8787", "clone", "x"],
            &["--api-url=http://localhost:8787", "context", "a/b"],
            &["help"],
            &["help", "clone"],
            &["--help"],
            &["-V"],
            &["upgrade", "--check"],
            &["deinit", "--yes"],
        ] {
            assert_eq!(route(&args(line)), Route::Speq, "{line:?}");
        }
    }

    #[test]
    fn specs_dir_flag_is_speqs_not_gits() {
        for line in [
            &["--specs-dir", "docs/specs", "context", "a/b"][..],
            &["--specs-dir=docs/specs", "context", "a/b"],
            &["--json", "--specs-dir", ".", "clone", "x"],
        ] {
            assert_eq!(route(&args(line)), Route::Speq, "{line:?}");
        }
        assert_eq!(
            route(&args(&["--specs-dir", "docs/specs", "status"])),
            Route::Git(args(&["status"]))
        );
        assert_eq!(
            route(&args(&["--specs-dir=docs/specs", "--json", "log", "-1"])),
            Route::Git(args(&["log", "-1"]))
        );
    }

    #[test]
    fn the_leading_specs_dir_is_read_past_other_global_flags() {
        let dir = |line: &[&str]| leading_specs_dir(&args(line));
        assert_eq!(
            dir(&["--specs-dir", "docs/specs", "status"]).as_deref(),
            Some("docs/specs")
        );
        assert_eq!(dir(&["--specs-dir=.", "status"]).as_deref(), Some("."));
        assert_eq!(
            dir(&[
                "--json",
                "--api-url",
                "http://x",
                "--specs-dir",
                "d",
                "status"
            ])
            .as_deref(),
            Some("d")
        );
        assert_eq!(
            dir(&["--api-url=http://x", "--specs-dir=d", "log"]).as_deref(),
            Some("d")
        );
        assert_eq!(dir(&["status"]), None);
        assert_eq!(dir(&["--specs-dir"]), None);
        // A Git option after the first command is Git's own.
        assert_eq!(dir(&["log", "--specs-dir", "d"]), None);
    }

    #[test]
    fn everything_else_is_git() {
        assert_eq!(route(&args(&["status"])), Route::Git(args(&["status"])));
        assert_eq!(
            route(&args(&["commit", "-am", "docs: x"])),
            Route::Git(args(&["commit", "-am", "docs: x"]))
        );
        assert_eq!(
            route(&args(&["--no-pager", "log", "--oneline"])),
            Route::Git(args(&["--no-pager", "log", "--oneline"]))
        );
        assert_eq!(
            route(&args(&["-C", "specs", "status"])),
            Route::Git(args(&["-C", "specs", "status"]))
        );
        // speq's global flags are not Git's.
        assert_eq!(
            route(&args(&["--json", "status"])),
            Route::Git(args(&["status"]))
        );
        assert_eq!(route(&args(&["push"])), Route::Git(args(&["push"])));
        assert_eq!(route(&args(&["init"])), Route::Git(args(&["init"])));
    }

    #[test]
    fn help_for_a_git_command_is_gits_manual() {
        assert_eq!(
            route(&args(&["help", "commit"])),
            Route::GitHelp(args(&["help", "commit"]))
        );
    }

    #[test]
    fn squash_is_only_an_opt_in_push_flag() {
        assert_eq!(
            route(&args(&["push", "--squash"])),
            Route::SquashPush(args(&["push"]))
        );
        assert_eq!(
            route(&args(&["push", "origin", "--squash", "HEAD:main"])),
            Route::SquashPush(args(&["push", "origin", "HEAD:main"]))
        );
        assert_eq!(
            route(&args(&["commit", "--squash", "HEAD"])),
            Route::Git(args(&["commit", "--squash", "HEAD"]))
        );
    }
}
