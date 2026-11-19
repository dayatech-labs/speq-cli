//! `speq ui get <url>`: the HTML of one prototype page of a product's `<product>-ui` repository, for a coding agent
//! that was handed a link from the Speq Web app. The link is only read for the UI product and the page it names; the
//! page itself comes from the Speq API with this device's session, so the token never goes to the link's host.
//! The HTML is untrusted text: speq prints it and never renders or runs it.
use crate::api::{self, Persona, is_invisible, printable};
use crate::env::Env;
use crate::error::{Error, Result};
use crate::session::Session;
use serde::Serialize;
use url::Url;

/// What the Web app puts after `#` to select a component: `#component=<id>`.
const COMPONENT_KEY: &str = "component";

/// The page a link points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Origin of an absolute link; a relative `/ui/...` carries none.
    pub origin: Option<String>,
    pub ui_product_id: String,
    /// `pages/<slug>.html`, as in the ui repository.
    pub path: String,
    pub component: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct UiPageReport {
    pub ui_product_id: String,
    pub path: String,
    /// The commit of the ui repository the page was read at.
    pub head_sha: String,
    /// The blob ID of the page.
    pub sha: String,
    /// The component the link selects, when it names one: the whole page is printed either way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    pub content: String,
}

fn not_a_page(input: &str) -> Error {
    Error::invalid(format!(
        "{} is not the link of a prototype page; expected <speq-web>/ui/<ui-product>/<page>",
        printable(input)
    ))
}

/// One page-name segment: what the API accepts in a path (`A-Za-z0-9._-`, no hidden file).
fn is_page_segment(segment: &str) -> bool {
    (1..=100).contains(&segment.len())
        && segment.starts_with(|c: char| c.is_ascii_alphanumeric())
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// A component id as the Web app writes it: a short safe token.
fn is_component_id(id: &str) -> bool {
    (1..=100).contains(&id.len())
        && id.starts_with(|c: char| c.is_ascii_alphanumeric())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
}

/// The parts of a Web link every `speq ui` command reads: where it came from, the UI product, and the page segments
/// after it (none for the page list of a product). The query string (`?persona=admin&state=empty`: only how the Web
/// preview looks) and the fragment are not part of the path and are not read here.
struct LinkParts {
    origin: Option<String>,
    ui_product_id: String,
    slug: Vec<String>,
    url: Url,
}

fn split_link(input: &str) -> Result<LinkParts> {
    let input = input.trim();
    let (origin, url) = match Url::parse(input) {
        Ok(url) if matches!(url.scheme(), "http" | "https") && url.host().is_some() => {
            (Some(url.origin().ascii_serialization()), url)
        }
        // `//host/...` (and `/\host/...`, which URL parsing reads the same way) would name a host of its own; only a
        // plain absolute path is relative.
        Err(url::ParseError::RelativeUrlWithoutBase)
            if input.starts_with('/') && !input.starts_with("//") && !input.starts_with("/\\") =>
        {
            let base = Url::parse("https://speq.invalid").expect("a constant URL parses");
            (None, base.join(input).map_err(|_| not_a_page(input))?)
        }
        _ => return Err(not_a_page(input)),
    };

    let mut segments: Vec<&str> = url
        .path_segments()
        .map(Iterator::collect)
        .unwrap_or_default();
    if segments.last() == Some(&"") {
        segments.pop();
    }
    let [first, ui_product_id, slug @ ..] = segments.as_slice() else {
        return Err(not_a_page(input));
    };
    if *first != "ui" {
        return Err(not_a_page(input));
    }
    if !api::is_project_id(ui_product_id) {
        return Err(Error::invalid(format!(
            "{:?} is not a UI product id",
            printable(ui_product_id)
        )));
    }
    let ui_product_id = (*ui_product_id).to_string();
    let slug = slug.iter().map(|s| (*s).to_string()).collect();
    Ok(LinkParts {
        origin,
        ui_product_id,
        slug,
        url,
    })
}

/// The `pages/<slug>.html` path of page segments, each checked against what the API accepts.
fn page_path(slug: &[String]) -> Result<String> {
    let (last, folders) = slug.split_last().expect("a page has a name");
    let stem = last.strip_suffix(".html").unwrap_or(last);
    if !folders
        .iter()
        .map(String::as_str)
        .chain([stem])
        .all(is_page_segment)
    {
        return Err(Error::invalid(format!(
            "{:?} is not a prototype page name (letters, digits, `.`, `_` and `-`)",
            printable(&slug.join("/"))
        )));
    }
    Ok(format!(
        "pages/{}.html",
        folders
            .iter()
            .map(String::as_str)
            .chain([stem])
            .collect::<Vec<_>>()
            .join("/")
    ))
}

/// Reads `https://<speq-web>/ui/<ui-product>/<page>[?persona=..][#component=<id>]`, or just its path. Nothing is
/// fetched, and nothing is taken from the link but the UI product, the page and the component: every part is checked
/// against what the API allows, so a link can never reach another endpoint. The query string is ignored.
pub fn parse_target(input: &str) -> Result<Target> {
    let parts = split_link(input)?;
    if parts.slug.is_empty() {
        return Err(Error::invalid(
            "that link is the page list of a UI product, not one page; give the link of a page: <speq-web>/ui/<ui-product>/<page>",
        ));
    }
    let path = page_path(&parts.slug)?;
    let component = parts
        .url
        .fragment()
        .and_then(|fragment| {
            url::form_urlencoded::parse(fragment.as_bytes()).find(|(key, _)| key == COMPONENT_KEY)
        })
        .map(|(_, id)| id.into_owned());
    if let Some(id) = &component
        && !is_component_id(id)
    {
        return Err(Error::invalid(format!(
            "{:?} is not a component id",
            printable(id)
        )));
    }
    Ok(Target {
        origin: parts.origin,
        ui_product_id: parts.ui_product_id,
        path,
        component,
    })
}

/// The UI product a link names, for commands that read the whole product (`speq ui personas`): the page part is
/// optional, and when a link has one it must still be a valid page name.
pub fn parse_product(input: &str) -> Result<ProductTarget> {
    let parts = split_link(input)?;
    if !parts.slug.is_empty() {
        page_path(&parts.slug)?;
    }
    Ok(ProductTarget {
        origin: parts.origin,
        ui_product_id: parts.ui_product_id,
    })
}

/// The UI product a link points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductTarget {
    pub origin: Option<String>,
    pub ui_product_id: String,
}

/// A link has to come from the Speq Web app this device is signed in to, so a link of another deployment is not
/// answered with this one's data of the same UI product id. A link without an origin is taken as it is, and so is
/// any link when the API did not say where its Web app lives.
fn check_origin(origin: Option<&str>, web_url: Option<&str>) -> Result<()> {
    let (Some(origin), Some(web)) = (origin, web_url) else {
        return Ok(());
    };
    let expected = Url::parse(web)
        .ok()
        .map(|url| url.origin().ascii_serialization());
    if expected.as_deref() == Some(origin) {
        return Ok(());
    }
    Err(Error::invalid(format!(
        "that link is for {}, but this device is signed in to the Speq at {}; use a link from there",
        printable(origin),
        printable(web)
    )))
}

impl ProductTarget {
    pub fn check_origin(&self, web_url: Option<&str>) -> Result<()> {
        check_origin(self.origin.as_deref(), web_url)
    }
}

impl Target {
    /// See [`check_origin`]: a link of another deployment is refused.
    pub fn check_origin(&self, web_url: Option<&str>) -> Result<()> {
        check_origin(self.origin.as_deref(), web_url)
    }
}

/// `speq ui get`: the page a link names, read with this device's session. The link is checked before anything is sent.
pub async fn get(env: &Env, input: &str) -> Result<UiPageReport> {
    let target = parse_target(input)?;
    let session = Session::new(env);
    let auth = session.authenticate().await?;
    target.check_origin(session.snapshot().await?.web_url.as_deref())?;
    let page = env
        .client
        .ui_page(&auth.token, &target.ui_product_id, &target.path)
        .await?;
    Ok(UiPageReport {
        ui_product_id: target.ui_product_id,
        path: printable(&page.path),
        head_sha: page.head_sha,
        sha: page.sha,
        component: target.component,
        content: page.content,
    })
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PersonasReport {
    pub ui_product_id: String,
    /// The commit of the ui repository `personas.json` was read at.
    pub head_sha: String,
    /// `invalid` or `too_large` when `personas.json` could not be used; the list is then empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    pub personas: Vec<Persona>,
}

/// `speq ui personas`: the mock personas a UI product declares, read with this device's session. The link is checked
/// before anything is sent.
pub async fn personas(env: &Env, input: &str) -> Result<PersonasReport> {
    let target = parse_product(input)?;
    let session = Session::new(env);
    let auth = session.authenticate().await?;
    target.check_origin(session.snapshot().await?.web_url.as_deref())?;
    let response = env
        .client
        .ui_personas(&auth.token, &target.ui_product_id)
        .await?;
    Ok(PersonasReport {
        ui_product_id: target.ui_product_id,
        head_sha: response.head_sha,
        // A broken file declares nothing, whatever else the answer carries.
        personas: if response.problem.is_some() {
            vec![]
        } else {
            response.personas
        },
        problem: response.problem,
    })
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct DesignReport {
    pub ui_product_id: String,
    /// The commit of the ui repository `DESIGN.md` was read at.
    pub head_sha: String,
    /// `too_large` or `invalid` when `DESIGN.md` could not be used; `content` is then `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// The text of `DESIGN.md`; `None` when the repository has none or it cannot be shown. `Some("")` is an empty file.
    pub content: Option<String>,
}

/// `speq ui design`: `DESIGN.md` of a UI product, read with this device's session. The link is checked before
/// anything is sent.
pub async fn design(env: &Env, input: &str) -> Result<DesignReport> {
    let target = parse_product(input)?;
    let session = Session::new(env);
    let auth = session.authenticate().await?;
    target.check_origin(session.snapshot().await?.web_url.as_deref())?;
    let response = env
        .client
        .ui_design(&auth.token, &target.ui_product_id)
        .await?;
    Ok(DesignReport {
        ui_product_id: target.ui_product_id,
        head_sha: response.head_sha,
        problem: response.problem,
        content: response.content,
    })
}

/// The page as text for a terminal or a pipe: line breaks and tabs stay, every other control character (terminal
/// escape sequences, a lone carriage return that would overwrite a line) becomes a space, and every invisible
/// character (zero-width and direction marks, bidirectional overrides, the Unicode tag block that can spell a hidden
/// instruction) becomes a visible `<U+XXXX>` marker, so a hostile page can neither forge what the terminal shows nor
/// hide anything from whoever reads the output. `--json` carries the exact text, with those characters escaped.
pub fn terminal_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.replace("\r\n", "\n").chars() {
        if matches!(c, '\n' | '\t') {
            out.push(c);
        } else if c.is_control() {
            out.push(' ');
        } else if is_invisible(c) {
            out.push_str(&format!("<U+{:04X}>", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(input: &str) -> Target {
        parse_target(input).unwrap_or_else(|e| panic!("{input}: {e}"))
    }

    fn rejected(input: &str) -> String {
        parse_target(input).expect_err(input).to_string()
    }

    #[test]
    fn reads_the_ui_product_page_and_component_of_a_web_link() {
        assert_eq!(
            target("https://speq.example/ui/proj_1/checkout/payment#component=pay-button"),
            Target {
                origin: Some("https://speq.example".into()),
                ui_product_id: "proj_1".into(),
                path: "pages/checkout/payment.html".into(),
                component: Some("pay-button".into()),
            }
        );
    }

    #[test]
    fn a_relative_link_has_no_origin() {
        let t = target("/ui/proj_1/index");
        assert_eq!(t.origin, None);
        assert_eq!(t.path, "pages/index.html");
        assert_eq!(t.component, None);
    }

    #[test]
    fn forgives_a_trailing_html_slash_query_and_surrounding_space() {
        for link in [
            "  https://speq.example/ui/p/a/b.html ",
            "https://speq.example/ui/p/a/b/",
            "https://speq.example/ui/p/a/b?utm=1",
            "http://localhost:5173/ui/p/a/b",
        ] {
            let t = target(link);
            assert_eq!(
                (t.ui_product_id.as_str(), t.path.as_str()),
                ("p", "pages/a/b.html"),
                "{link}"
            );
        }
    }

    #[test]
    fn ignores_a_fragment_that_is_not_a_component_and_reads_one_among_others() {
        assert_eq!(target("/ui/p/a#top").component, None);
        assert_eq!(
            target("/ui/p/a#x=1&component=card:1&y=2")
                .component
                .as_deref(),
            Some("card:1")
        );
        assert_eq!(
            target("/ui/p/a#component=a%3Ab").component.as_deref(),
            Some("a:b")
        );
    }

    #[test]
    fn rejects_a_component_id_that_is_not_a_safe_token() {
        for fragment in [
            "component=",
            "component=a%20b",
            "component=-x",
            "component=a%22%3E",
            "component=%0A",
        ] {
            let error = rejected(&format!("/ui/p/a#{fragment}"));
            assert!(error.contains("not a component id"), "{fragment}: {error}");
        }
    }

    #[test]
    fn rejects_what_is_not_a_page_link() {
        for link in [
            "",
            "ui/p/a",
            "//evil.example/ui/p/a",
            "ftp://speq.example/ui/p/a",
            "javascript:alert(1)",
            "file:///ui/p/a",
            "https://speq.example/p/proj_1/epics/prd.md",
            "https://speq.example/ui",
            "https://speq.example/",
            "/v1/ui-products/p/pages",
            "not a url",
        ] {
            assert!(
                rejected(link).contains("not the link of a prototype page"),
                "{link:?}"
            );
        }
    }

    #[test]
    fn the_list_of_a_product_is_not_a_page() {
        for link in ["/ui/proj_1", "https://speq.example/ui/proj_1/"] {
            assert!(rejected(link).contains("page list"), "{link}");
        }
    }

    #[test]
    fn a_link_can_never_name_another_endpoint_or_leave_pages() {
        for link in [
            "/ui/p%2F..%2Fgit/a",
            "/ui/p/../../x",
            "/ui/p/a/../b",
            "/ui/p/.hidden",
            "/ui/p/a/.secret/b",
            "/ui/p/a%2Fb",
            "/ui/p/a%00",
            "/ui/p/a b",
            "/ui/p/a%0A",
            "/ui/p/é",
            "/ui/p/.html",
            "/ui/p/a?path=llms.txt",
        ] {
            // Either refused outright, or reduced to a page under `pages/` of that one UI product.
            match parse_target(link) {
                Ok(t) => {
                    assert!(
                        t.path.starts_with("pages/") && t.path.ends_with(".html"),
                        "{link}: {}",
                        t.path
                    );
                    assert!(!t.path.contains(".."), "{link}: {}", t.path);
                    assert!(is_page_path_only(&t.path), "{link}: {}", t.path);
                }
                Err(e) => assert!(!e.to_string().is_empty(), "{link}"),
            }
        }
    }

    fn is_page_path_only(path: &str) -> bool {
        path.strip_prefix("pages/")
            .and_then(|rest| rest.strip_suffix(".html"))
            .is_some_and(|stem| stem.split('/').all(is_page_segment))
    }

    #[test]
    fn rejects_a_ui_product_id_that_could_change_the_request() {
        for link in ["/ui/..%2Fx/a", "/ui/a%2Fb/a", "/ui/a.b/a", "/ui/%20/a"] {
            assert!(parse_target(link).is_err(), "{link}");
        }
        assert_eq!(target("/ui/0b9e-4c1f_A/a").ui_product_id, "0b9e-4c1f_A");
    }

    #[test]
    fn a_link_of_another_deployment_is_refused_but_a_relative_or_unverifiable_one_is_not() {
        let link = target("https://other.example/ui/p/a");
        assert!(
            link.check_origin(Some("https://speq.example"))
                .unwrap_err()
                .to_string()
                .contains("other.example")
        );
        assert!(link.check_origin(Some("https://speq.example")).is_err());
        assert!(link.check_origin(Some("https://other.example")).is_ok());
        assert!(link.check_origin(None).is_ok());
        assert!(
            target("/ui/p/a")
                .check_origin(Some("https://speq.example"))
                .is_ok()
        );
    }

    #[test]
    fn the_origin_includes_scheme_and_port() {
        let link = target("http://speq.example/ui/p/a");
        assert!(link.check_origin(Some("https://speq.example")).is_err());
        let link = target("https://speq.example:8443/ui/p/a");
        assert!(link.check_origin(Some("https://speq.example")).is_err());
        assert!(link.check_origin(Some("https://speq.example:8443")).is_ok());
    }

    #[test]
    fn a_query_string_changes_nothing() {
        let plain = target("https://speq.example/ui/p/checkout/payment#component=pay-button");
        for link in [
            "https://speq.example/ui/p/checkout/payment?persona=admin#component=pay-button",
            "https://speq.example/ui/p/checkout/payment?state=empty&persona=member#component=pay-button",
            "https://speq.example/ui/p/checkout/payment?view=full&state=empty&persona=a#component=pay-button",
            "https://speq.example/ui/p/checkout/payment/?persona=admin#component=pay-button",
            "https://speq.example/ui/p/checkout/payment.html?persona=admin#component=pay-button",
            "https://speq.example/ui/p/checkout/payment?#component=pay-button",
        ] {
            assert_eq!(target(link), plain, "{link}");
        }
        assert_eq!(
            target("/ui/p/a?persona=admin"),
            Target {
                origin: None,
                ui_product_id: "p".into(),
                path: "pages/a.html".into(),
                component: None
            }
        );
    }

    #[test]
    fn a_query_with_odd_content_is_still_ignored() {
        for link in [
            "/ui/p/a?persona=a%20b%0A&persona=%E2%80%AE",
            "/ui/p/a?persona=admin&persona=member&persona=",
            "/ui/p/a?persona=../../x&state=%2e%2e%2f",
            "/ui/p/a?path=llms.txt&component=evil",
            "/ui/p/a?persona=%00&=&&=x&%",
            "/ui/p/a?persona=admin?x=1",
        ] {
            let t = target(link);
            assert_eq!(
                (t.path.as_str(), t.component.clone()),
                ("pages/a.html", None),
                "{link}"
            );
        }
        // The query is not the fragment: `component` in it selects nothing, and a bad one in the fragment still fails.
        assert!(rejected("/ui/p/a?persona=x#component=a%20b").contains("not a component id"));
    }

    #[test]
    fn a_query_does_not_loosen_what_is_refused() {
        for link in [
            "//evil.example/ui/p/a?persona=admin",
            "/\\evil.example/ui/p/a?persona=admin",
            "/ui/p/.hidden?persona=admin",
            "/ui/p/a%2Fb?persona=admin",
            "/ui/..%2Fx/a?persona=admin",
            "/ui/p?persona=admin",
            "javascript:alert(1)?persona=admin",
        ] {
            assert!(parse_target(link).is_err(), "{link}");
        }
        assert!(rejected("/ui/p?persona=admin").contains("page list"));
    }

    #[test]
    fn a_product_link_may_leave_out_the_page_and_may_carry_a_query() {
        for link in [
            "https://speq.example/ui/p",
            "https://speq.example/ui/p/",
            "https://speq.example/ui/p?persona=admin",
            "https://speq.example/ui/p/admin/orders?persona=admin&state=empty#component=x",
            "https://speq.example/ui/p/admin/orders.html",
        ] {
            let t = parse_product(link).unwrap_or_else(|e| panic!("{link}: {e}"));
            assert_eq!(
                (t.origin.as_deref(), t.ui_product_id.as_str()),
                (Some("https://speq.example"), "p"),
                "{link}"
            );
        }
        assert_eq!(parse_product("/ui/p").unwrap().origin, None);
    }

    #[test]
    fn a_product_link_is_checked_as_strictly_as_a_page_link() {
        for link in [
            "",
            "/ui",
            "/p/p/a",
            "//evil.example/ui/p",
            "/\\evil.example/ui/p",
            "ftp://speq.example/ui/p",
            "/ui/..%2Fx",
            "/ui/a.b",
            "/ui/p/.hidden",
            "/ui/p/a%2Fb",
        ] {
            assert!(parse_product(link).is_err(), "{link:?}");
        }
        let link = parse_product("https://other.example/ui/p").unwrap();
        assert!(link.check_origin(Some("https://speq.example")).is_err());
        assert!(link.check_origin(Some("https://other.example")).is_ok());
        assert!(link.check_origin(None).is_ok());
    }

    #[test]
    fn terminal_text_keeps_a_design_document_readable() {
        let design =
            "# Design\n\n| token | value |\n|---|---|\n| `--c` | #fff |\n\n- naïve — ✓ 日本\n";
        assert_eq!(terminal_text(design), design);
        assert_eq!(terminal_text(""), "");
    }

    #[test]
    fn terminal_text_keeps_the_page_but_not_what_forges_a_terminal() {
        assert_eq!(
            terminal_text("<p>a</p>\n\t<b>b</b>\r\n"),
            "<p>a</p>\n\t<b>b</b>\n"
        );
        assert_eq!(terminal_text("ok\u{1b}[31mred\u{1b}[0m"), "ok [31mred [0m");
        assert_eq!(terminal_text("safe\rforged"), "safe forged");
        assert_eq!(
            terminal_text("a\u{202E}b\u{2066}c\u{0}d"),
            "a<U+202E>b<U+2066>c d"
        );
        assert_eq!(terminal_text("héllo — ✓ 日本"), "héllo — ✓ 日本");
    }
}
