use speq::commands::{Route, leading_specs_dir, parse_args, route, run, run_git};
use speq::env::Env;
use speq::error::{Error, exit};
use speq::links::{Invocation, invocation};
use speq::output::{render_error, stderr_color};
use std::ffi::OsString;
use std::process::ExitCode;
use tracing_subscriber::EnvFilter;

fn report_error(json: bool, error: &Error) {
    if json {
        let body = serde_json::json!({ "error": { "code": error.code(), "message": error.to_string(), "request_id": error.request_id() } });
        eprintln!("{body}");
    } else {
        eprintln!("{}", render_error(error, stderr_color()));
    }
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    // Logs go to stderr, at `warn` unless SPEQ_LOG says otherwise. Tokens and content are never logged.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("SPEQ_LOG").unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    // The same binary is Git's remote helper and credential helper, under those names.
    let argv0 = args
        .first()
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    match invocation(&argv0) {
        Invocation::RemoteHelper => return ExitCode::from(speq::remote_helper::main(&args)),
        Invocation::CredentialHelper => return ExitCode::from(speq::git_auth::main(&args)),
        Invocation::Speq => {}
    }

    let json = args.iter().skip(1).any(|a| a == "--json");
    match route(args.get(1..).unwrap_or_default()) {
        Route::Speq => {}
        git => {
            let specs_dir = speq::repo::specs_dir(leading_specs_dir(&args[1..]).as_deref());
            return match run_git(git, &specs_dir) {
                Ok(code) => ExitCode::from(code),
                Err(e) => {
                    report_error(json, &e);
                    ExitCode::from(e.exit_code())
                }
            };
        }
    }

    let cli = match parse_args(args.iter().map(|a| a.to_string_lossy().into_owned())) {
        Ok(cli) => cli,
        // Clap prints help/version/usage errors itself and picks the exit code (0 or 2).
        Err(e) => e.exit(),
    };
    let open_browser = !matches!(&cli.command, speq::cli::Command::Login(a) if a.no_browser);
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            report_error(
                json,
                &Error::Other(format!("cannot start the runtime: {e}")),
            );
            return ExitCode::from(exit::FAILURE);
        }
    };
    let outcome = Env::production(cli.api_url.as_deref(), open_browser)
        .and_then(|env| runtime.block_on(run(cli, &env)));
    match outcome {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            report_error(json, &e);
            ExitCode::from(e.exit_code())
        }
    }
}
