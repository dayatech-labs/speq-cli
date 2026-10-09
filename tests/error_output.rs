//! The human error template: one layout for every error, colors only on request.
use speq::error::{ApiError, Error};
use speq::output::render_error;

fn api(status: u16, code: &str, message: &str) -> Error {
    Error::Api(Box::new(ApiError {
        status,
        code: code.into(),
        message: message.into(),
        request_id: Some("req_123".into()),
        details: None,
        retry_after: None,
    }))
}

#[test]
fn every_error_follows_the_title_detail_hint_meta_layout() {
    assert_eq!(
        render_error(&Error::NotLoggedIn, false),
        "✖ Not logged in\n\n  You are not signed in on this machine.\n\n  → Run `speq login`\n\n  code not_logged_in · exit 3"
    );
}

#[test]
fn an_error_without_a_next_step_has_no_hint_line() {
    let out = render_error(&Error::invalid("path escapes the workspace"), false);
    assert_eq!(
        out,
        "✖ Invalid request\n\n  path escapes the workspace\n\n  code invalid · exit 7"
    );
    assert!(!out.contains('→'));
}

#[test]
fn api_errors_show_status_and_request_id_for_support() {
    let out = render_error(
        &api(403, "forbidden", "You cannot push to this repository."),
        false,
    );
    assert!(out.starts_with("✖ Access denied\n"));
    assert!(out.contains("  You cannot push to this repository.\n"));
    assert!(out.ends_with("  code forbidden · HTTP 403 · request req_123 · exit 6"));
}

#[test]
fn rate_limits_say_when_to_retry() {
    let Error::Api(mut e) = api(429, "rate_limited", "Too many requests.") else {
        unreachable!()
    };
    e.retry_after = Some(30);
    let out = render_error(&Error::Api(e), false);
    assert!(out.contains("→ Try again in 30s"));
    assert!(out.contains("exit 4"));
}

#[test]
fn a_rejected_token_points_to_login() {
    let out = render_error(&api(401, "unauthorized", "Token revoked."), false);
    assert!(out.contains("→ Run `speq login`"));
}

#[test]
fn multi_line_details_are_indented_under_the_heading() {
    let out = render_error(&Error::conflict("2 conflicts:\n  a.md\n  b.md"), false);
    assert!(out.contains("\n  2 conflicts:\n    a.md\n    b.md\n"));
}

#[test]
fn plain_output_has_no_escape_codes() {
    for error in [
        Error::NotLoggedIn,
        Error::NotInitialized,
        Error::Network("timeout".into()),
        api(500, "internal", "boom"),
    ] {
        assert!(!render_error(&error, false).contains('\x1b'));
    }
}

#[test]
fn colored_output_highlights_the_title_and_commands_and_keeps_the_text() {
    let colored = render_error(&Error::NotLoggedIn, true);
    assert!(colored.contains('\x1b'));
    // Same words as the plain version once the escape sequences are removed.
    let mut stripped = String::new();
    let mut in_escape = false;
    for c in colored.chars() {
        match (in_escape, c) {
            (_, '\x1b') => in_escape = true,
            (true, 'm') => in_escape = false,
            (false, c) => stripped.push(c),
            _ => {}
        }
    }
    assert_eq!(stripped, render_error(&Error::NotLoggedIn, false));
}
