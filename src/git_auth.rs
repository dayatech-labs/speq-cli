//! `git-credential-speq`: Git's credential helper for the API's read-only Git endpoint. `speq clone` configures it
//! for the API origin only. `get` answers with the stored CLI token (as the password) for an origin that has one,
//! and nothing for any other host, so the token never reaches a server it was not issued by. No network is used.
use crate::api::parse_api_url;
use crate::credentials::StoredCredential;
use crate::error::Result;
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

/// `load` reads the stored credential of an origin (the store is keyed by origin).
pub fn answer(
    request: &BTreeMap<String, String>,
    load: impl Fn(&Url) -> Result<Option<StoredCredential>>,
    now: time::OffsetDateTime,
) -> Answer {
    let Some(origin) = origin_of(request) else {
        return Answer::Unknown;
    };
    match load(&origin) {
        Ok(Some(credential)) if !credential.is_expired(now) => {
            Answer::Token(credential.access_token)
        }
        _ => Answer::NotLoggedIn(origin),
    }
}

/// Runs one helper action (`get`, `store`, `erase`) on Git's stdin/stdout.
pub fn run(
    action: &str,
    input: impl BufRead,
    mut output: impl Write,
    mut errors: impl Write,
    load: impl Fn(&Url) -> Result<Option<StoredCredential>>,
) -> std::io::Result<()> {
    let request = read_request(input);
    match action {
        "get" => match answer(&request, load, crate::snapshot::now()) {
            Answer::Unknown => {}
            Answer::NotLoggedIn(origin) => {
                writeln!(
                    errors,
                    "speq: not logged in to {}; run `speq login`",
                    origin.as_str().trim_end_matches('/')
                )?;
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
    let stdin = std::io::stdin();
    match run(
        action,
        stdin.lock(),
        std::io::stdout(),
        std::io::stderr(),
        load,
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

    #[test]
    fn get_answers_only_for_the_origin_that_holds_the_token() {
        let load = store(
            "https://speq-api.example.com",
            credential(Duration::days(1)),
        );
        let mut out = Vec::new();
        run(
            "get",
            "protocol=https\nhost=speq-api.example.com\npath=v1/projects/p/git/info/refs\n\n"
                .as_bytes(),
            &mut out,
            Vec::new(),
            &load,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "username=speq\npassword=tok_secret\n"
        );

        for other in [
            "protocol=https\nhost=github.com\n\n",
            "protocol=https\nhost=speq-api.example.com.evil.test\n\n",
            "protocol=http\nhost=speq-api.example.com\n\n", // plain http off loopback is never an API origin
            "host=speq-api.example.com\n\n",
        ] {
            let mut out = Vec::new();
            let mut err = Vec::new();
            run("get", other.as_bytes(), &mut out, &mut err, &load).unwrap();
            let out = String::from_utf8(out).unwrap();
            assert!(!out.contains("tok_secret"), "{other}");
        }
    }

    #[test]
    fn get_stops_git_from_prompting_when_the_session_is_missing_or_expired() {
        for load in [
            Box::new(store(
                "https://a.example",
                credential(-Duration::minutes(1)),
            )) as Box<dyn Fn(&Url) -> Result<Option<StoredCredential>>>,
            Box::new(|_: &Url| Ok(None)),
        ] {
            let mut out = Vec::new();
            let mut err = Vec::new();
            run(
                "get",
                "protocol=https\nhost=a.example\n\n".as_bytes(),
                &mut out,
                &mut err,
                &load,
            )
            .unwrap();
            assert_eq!(String::from_utf8(out).unwrap(), "quit=1\n");
            assert!(String::from_utf8(err).unwrap().contains("run `speq login`"));
        }
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
            )
            .unwrap();
            assert!(out.is_empty(), "{action}");
        }
    }
}
