//! `git-credential-speq`: Git's credential helper for the API's read-only Git endpoint. `speq clone` configures it
//! for the API origin only, with `useHttpPath` so Git names the project. `get` trades the stored CLI token for a
//! Git fetch token of that project (`POST git/token`) and answers with that: valid for minutes, only for fetching
//! that one project. Any program of the user can run this helper, so it never hands out the CLI token itself, which
//! pushes and signs out everywhere; and it answers nothing for any other host.
use crate::api::parse_api_url;
use crate::credentials::StoredCredential;
use crate::error::{Error, Result};
use secrecy::{ExposeSecret, SecretString};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use url::Url;

/// What `get` decided, so callers (and tests) can tell the cases apart.
#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    /// Not an origin speq knows: say nothing, so Git asks its other helpers.
    Unknown,
    /// Speq's origin without a usable session: tell Git to stop instead of prompting for a password.
    NotLoggedIn(Url),
    /// Git did not name the project (a repository cloned before `useHttpPath` was set).
    NoProject,
    /// The API would not issue a token; the reason is for the user.
    Failed(String),
    /// A Git fetch token.
    Token(String),
}

/// Reads Git's `key=value` lines up to the blank line that ends them.
pub fn read_request(input: impl BufRead) -> BTreeMap<String, String> {
    input
        .lines()
        .map_while(|line| line.ok())
        .take_while(|line| !line.is_empty())
        .filter_map(|line| {
            line.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect()
}

/// The API origin a request is for: `protocol://host[:port]`, valid only if it is one `--api-url` would accept.
pub fn origin_of(request: &BTreeMap<String, String>) -> Option<Url> {
    let protocol = request.get("protocol")?;
    let host = request.get("host")?;
    parse_api_url(&format!("{protocol}://{host}")).ok()
}

/// The project of the fetch URL `<api>/v1/projects/<id>/git`, from the `path` Git sends with `useHttpPath`.
pub fn project_of(request: &BTreeMap<String, String>) -> Option<String> {
    let path = request.get("path")?;
    let mut segments = path.trim_start_matches('/').split('/');
    let (Some("v1"), Some("projects"), Some(id), Some("git")) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return None;
    };
    crate::repo::valid_project_id(id).then(|| id.to_string())
}

/// `load` reads the stored credential of an origin (the store is keyed by origin); `exchange` trades it for a Git
/// fetch token of one project.
pub fn answer(
    request: &BTreeMap<String, String>,
    load: impl Fn(&Url) -> Result<Option<StoredCredential>>,
    exchange: impl Fn(&Url, &StoredCredential, &str) -> Result<SecretString>,
    now: time::OffsetDateTime,
) -> Answer {
    let Some(origin) = origin_of(request) else {
        return Answer::Unknown;
    };
    // Never fall back to the CLI token: any program could then get it by leaving the path out.
    let Some(project) = project_of(request) else {
        return Answer::NoProject;
    };
    let credential = match load(&origin) {
        Ok(Some(credential)) if !credential.is_expired(now) => credential,
        _ => return Answer::NotLoggedIn(origin),
    };
    match exchange(&origin, &credential, &project) {
        Ok(token) => Answer::Token(token.expose_secret().to_string()),
        Err(Error::NotLoggedIn | Error::SessionExpired) => Answer::NotLoggedIn(origin),
        Err(error) => Answer::Failed(error.to_string()),
    }
}

/// Runs one helper action (`get`, `store`, `erase`) on Git's stdin/stdout.
pub fn run(
    action: &str,
    input: impl BufRead,
    mut output: impl Write,
    mut errors: impl Write,
    load: impl Fn(&Url) -> Result<Option<StoredCredential>>,
    exchange: impl Fn(&Url, &StoredCredential, &str) -> Result<SecretString>,
) -> std::io::Result<()> {
    let request = read_request(input);
    match action {
        "get" => match answer(&request, load, exchange, crate::snapshot::now()) {
            Answer::Unknown => {}
            Answer::NotLoggedIn(origin) => {
                writeln!(
                    errors,
                    "speq: not logged in to {}; run `speq login`",
                    origin.as_str().trim_end_matches('/')
                )?;
                writeln!(output, "quit=1")?;
            }
            Answer::NoProject => {
                writeln!(
                    errors,
                    "speq: this repository predates Git fetch tokens; run any speq command in it once (for example \
                     `speq status`) to update its settings"
                )?;
                writeln!(output, "quit=1")?;
            }
            Answer::Failed(why) => {
                writeln!(errors, "speq: cannot get a Git token: {why}")?;
                writeln!(output, "quit=1")?;
            }
            Answer::Token(token) => {
                writeln!(output, "username=speq")?;
                writeln!(output, "password={token}")?;
            }
        },
        // Git reports a rejected credential: it may have expired or been revoked. The stored one is cleared by
        // the next speq command that sees the rejection; here the user just learns what to do.
        "erase" if origin_of(&request).is_some() => {
            writeln!(
                errors,
                "speq: the Speq API rejected the session; run `speq login`"
            )?;
        }
        // `store` and anything else: speq keeps its own token, Git has nothing to save.
        _ => {}
    }
    output.flush()
}

/// Entry point when the binary runs as `git-credential-speq <action>`.
pub fn main(args: &[std::ffi::OsString]) -> u8 {
    let action = args.get(1).and_then(|a| a.to_str()).unwrap_or_default();
    let load = |origin: &Url| -> Result<Option<StoredCredential>> {
        let env = crate::env::Env::production(Some(origin.as_str()), false)?;
        env.creds.load(&env.account())
    };
    let exchange =
        |origin: &Url, credential: &StoredCredential, project: &str| -> Result<SecretString> {
            let env = crate::env::Env::production(Some(origin.as_str()), false)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| Error::Other(format!("cannot start the runtime: {e}")))?;
            let token = SecretString::from(credential.access_token.clone());
            runtime.block_on(env.client.git_token(&token, project))
        };
    let stdin = std::io::stdin();
    match run(
        action,
        stdin.lock(),
        std::io::stdout(),
        std::io::stderr(),
        load,
        exchange,
    ) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("speq: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Duration, OffsetDateTime};

    fn request(text: &str) -> BTreeMap<String, String> {
        read_request(text.as_bytes())
    }

    fn credential(expires_in: Duration) -> StoredCredential {
        StoredCredential {
            user_id: Some("u1".into()),
            access_token: "tok_secret".into(),
            expires_at: OffsetDateTime::now_utc() + expires_in,
        }
    }

    fn store(
        origin: &'static str,
        c: StoredCredential,
    ) -> impl Fn(&Url) -> Result<Option<StoredCredential>> {
        move |url: &Url| Ok((url.as_str().trim_end_matches('/') == origin).then(|| c.clone()))
    }

    /// Stands in for `POST git/token`: a token naming the project, issued only for the stored CLI token.
    fn exchange(_: &Url, c: &StoredCredential, project: &str) -> Result<SecretString> {
        assert_eq!(c.access_token, "tok_secret");
        Ok(SecretString::from(format!("sgr_for_{project}")))
    }

    /// Runs `get` for `request`; returns (stdout, stderr).
    fn get(
        request: &str,
        load: impl Fn(&Url) -> Result<Option<StoredCredential>>,
        exchange: impl Fn(&Url, &StoredCredential, &str) -> Result<SecretString>,
    ) -> (String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        run(
            "get",
            request.as_bytes(),
            &mut out,
            &mut err,
            load,
            exchange,
        )
        .unwrap();
        (
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    const FETCH: &str =
        "protocol=https\nhost=speq-api.example.com\npath=v1/projects/proj_1/git\n\n";

    #[test]
    fn get_gives_git_a_fetch_token_for_the_project_never_the_cli_token() {
        let load = store(
            "https://speq-api.example.com",
            credential(Duration::days(1)),
        );
        let (out, _) = get(FETCH, &load, exchange);
        assert_eq!(out, "username=speq\npassword=sgr_for_proj_1\n");

        for other in [
            "protocol=https\nhost=github.com\npath=v1/projects/proj_1/git\n\n",
            "protocol=https\nhost=speq-api.example.com.evil.test\npath=v1/projects/proj_1/git\n\n",
            // plain http off loopback is never an API origin
            "protocol=http\nhost=speq-api.example.com\npath=v1/projects/proj_1/git\n\n",
            "host=speq-api.example.com\npath=v1/projects/proj_1/git\n\n",
            // No project, or not one: never the CLI token instead.
            "protocol=https\nhost=speq-api.example.com\n\n",
            "protocol=https\nhost=speq-api.example.com\npath=v1/projects/proj_1\n\n",
            "protocol=https\nhost=speq-api.example.com\npath=v1/projects/a%2Fb/git\n\n",
            "protocol=https\nhost=speq-api.example.com\npath=v1/me\n\n",
        ] {
            let (out, err) = get(other, &load, exchange);
            assert!(
                !out.contains("tok_secret") && !err.contains("tok_secret"),
                "{other}"
            );
            assert!(!out.contains("password="), "{other}");
        }
    }

    #[test]
    fn a_repository_that_does_not_name_its_project_is_told_how_to_update() {
        let load = store(
            "https://speq-api.example.com",
            credential(Duration::days(1)),
        );
        let (out, err) = get(
            "protocol=https\nhost=speq-api.example.com\n\n",
            &load,
            |_: &Url, _: &StoredCredential, _: &str| -> Result<SecretString> {
                panic!("no token is requested without a project")
            },
        );
        assert_eq!(out, "quit=1\n");
        assert!(err.contains("speq status"), "{err}");
    }

    #[test]
    fn get_stops_git_from_prompting_when_the_session_is_missing_expired_or_refused() {
        let missing = |_: &Url, _: &StoredCredential, _: &str| -> Result<SecretString> {
            panic!("no token is requested without a session")
        };
        for load in [
            Box::new(store(
                "https://speq-api.example.com",
                credential(-Duration::minutes(1)),
            )) as Box<dyn Fn(&Url) -> Result<Option<StoredCredential>>>,
            Box::new(|_: &Url| Ok(None)),
        ] {
            let (out, err) = get(FETCH, &load, missing);
            assert_eq!(out, "quit=1\n");
            assert!(err.contains("run `speq login`"), "{err}");
        }

        let load = store(
            "https://speq-api.example.com",
            credential(Duration::days(1)),
        );
        let revoked = |_: &Url, _: &StoredCredential, _: &str| -> Result<SecretString> {
            Err(Error::SessionExpired)
        };
        let (out, err) = get(FETCH, &load, revoked);
        assert_eq!(out, "quit=1\n");
        assert!(err.contains("run `speq login`"), "{err}");

        let down = |_: &Url, _: &StoredCredential, _: &str| -> Result<SecretString> {
            Err(Error::Network("cannot connect to the Speq API".into()))
        };
        let (out, err) = get(FETCH, &load, down);
        assert_eq!(out, "quit=1\n");
        assert!(err.contains("cannot connect"), "{err}");
    }

    #[test]
    fn the_project_comes_from_the_fetch_url_path() {
        let project = |path: &str| project_of(&request(&format!("path={path}\n\n")));
        assert_eq!(project("v1/projects/proj_1/git").as_deref(), Some("proj_1"));
        assert_eq!(
            project("/v1/projects/proj_1/git/info/refs").as_deref(),
            Some("proj_1")
        );
        for bad in [
            "",
            "v1/projects//git",
            "v1/projects/p q/git",
            "v2/projects/p/git",
            "x/v1/projects/p/git",
        ] {
            assert_eq!(project(bad), None, "{bad:?}");
        }
        assert_eq!(project_of(&request("host=a\n\n")), None);
    }

    #[test]
    fn local_apis_keep_their_port() {
        let r = request("protocol=http\nhost=127.0.0.1:8787\n\n");
        assert_eq!(origin_of(&r).unwrap().as_str(), "http://127.0.0.1:8787/");
    }

    #[test]
    fn store_and_erase_never_write_to_git() {
        let load = |_: &Url| Ok(None);
        for action in ["store", "erase", "unknown"] {
            let mut out = Vec::new();
            run(
                action,
                "protocol=https\nhost=a.example\nusername=speq\npassword=x\n\n".as_bytes(),
                &mut out,
                Vec::new(),
                load,
                exchange,
            )
            .unwrap();
            assert!(out.is_empty(), "{action}");
        }
    }
}
