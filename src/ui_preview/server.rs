//! The preview's HTTP server: the viewer's files from the binary, and the folder's pages through a small read-only API.
//! It is a local development tool, so it is as small as it can be (no framework, no keep-alive, `GET` and `HEAD` only),
//! and as closed as it can be:
//! - it listens on `127.0.0.1` only, and answers only requests whose `Host` is the address it listens on (so a web page
//!   that makes the browser resolve its own name to this machine, "DNS rebinding", gets nothing) and whose `Origin`, when
//!   the browser sends one, is this server's own (so the prototype in its sandbox, whose origin is `null`, and other
//!   sites cannot read it); it sends no CORS header;
//! - it serves only what the pack holds, found by exact key, and the folder's files through [`Folder`], which exposes
//!   nothing but pages, `personas.json` and `DESIGN.md`;
//! - a request is bounded in size and time, and so is the number of connections served at once.
//!
//! The viewer's page (`local.html`) answers every address that is not a file or an API call, so `/p/<page>` can be opened
//! directly. No `Content-Security-Policy` is sent with it: a prototype is shown in an `about:srcdoc` frame, which inherits
//! the policy of the page that holds it, and a policy for the viewer would take away the script the prototype needs; the
//! prototype carries its own policy instead (see speq-app's `prototype-preview.ts`).
use super::bundle::{Bundle, SHELL};
use super::folder::{Folder, PageError};
use crate::error::{Error, Result};
use serde::Serialize;
use serde_json::json;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use url::Url;

const MAX_HEAD: usize = 16 * 1024;
const MAX_HEADERS: usize = 64;
const MAX_CONNECTIONS: usize = 32;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// Where the viewer's files are served, and where its API is.
const APP_PREFIX: &str = "/__speq/app/";
const API_PREFIX: &str = "/__speq/api/";
const RESERVED_PREFIX: &str = "/__speq/";

struct Shared {
    folder: Folder,
    bundle: Bundle,
    /// The `Host` values this server answers to.
    hosts: [String; 3],
}

pub struct Server {
    listener: TcpListener,
    shared: Arc<Shared>,
    port: u16,
}

/// A server running on its own thread, for tests.
pub struct Running {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Running {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stops accepting and waits for the loop to end.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // `accept` blocks: one connection wakes it to see the flag.
        let _ = TcpStream::connect(self.addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Server {
    /// Listens on `127.0.0.1:port` (0 picks a free port).
    pub fn bind(folder: Folder, bundle: Bundle, port: u16) -> Result<Server> {
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(|source| Error::Io {
                context: format!("cannot listen on 127.0.0.1:{port}"),
                source,
            })?;
        let port = listener
            .local_addr()
            .map_err(|source| Error::Io {
                context: "cannot read the address the preview listens on".into(),
                source,
            })?
            .port();
        let hosts = [
            format!("127.0.0.1:{port}"),
            format!("localhost:{port}"),
            format!("[::1]:{port}"),
        ];
        Ok(Server {
            listener,
            shared: Arc::new(Shared {
                folder,
                bundle,
                hosts,
            }),
            port,
        })
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }

    pub fn addr(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
    }

    /// Serves until `stop` is set (and one more connection arrives) or the process ends.
    pub fn run(self, stop: &AtomicBool) {
        let active = Arc::new(AtomicUsize::new(0));
        for stream in self.listener.incoming() {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = stream else {
                // Out of file descriptors and the like: wait instead of spinning.
                thread::sleep(Duration::from_millis(50));
                continue;
            };
            if active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                active.fetch_sub(1, Ordering::SeqCst);
                continue;
            }
            let shared = Arc::clone(&self.shared);
            let counter = Arc::clone(&active);
            let spawned = thread::Builder::new().spawn(move || {
                serve_connection(stream, &shared);
                counter.fetch_sub(1, Ordering::SeqCst);
            });
            if spawned.is_err() {
                active.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    pub fn spawn(self) -> Running {
        let addr = self.addr();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = thread::spawn(move || self.run(&flag));
        Running {
            addr,
            stop,
            thread: Some(thread),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------- replies

struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Content Too Large",
        422 => "Unprocessable Content",
        431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    }
}

impl Reply {
    fn new(status: u16, content_type: &str, cache: &str, body: Vec<u8>) -> Reply {
        Reply {
            status,
            headers: vec![
                ("Content-Type", content_type.to_string()),
                ("Cache-Control", cache.to_string()),
            ],
            body,
        }
    }

    fn text(status: u16, message: &str) -> Reply {
        Reply::new(
            status,
            "text/plain; charset=utf-8",
            "no-store",
            format!("{message}\n").into_bytes(),
        )
    }

    fn json(status: u16, value: &impl Serialize) -> Reply {
        let body = serde_json::to_vec(value).expect("the preview's replies serialise");
        Reply::new(status, "application/json; charset=utf-8", "no-store", body)
    }

    fn error(status: u16, code: &str) -> Reply {
        Reply::json(status, &json!({ "error": code }))
    }

    /// A file of the pack, as it is stored: gzip.
    fn gzip_file(content_type: &str, cache: &str, bytes: &[u8]) -> Reply {
        let mut reply = Reply::new(200, content_type, cache, bytes.to_vec());
        reply.headers.push(("Content-Encoding", "gzip".into()));
        reply.headers.push(("Vary", "Accept-Encoding".into()));
        reply
    }
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn write_reply(stream: &mut TcpStream, reply: &Reply, head_only: bool) {
    let mut head = format!("HTTP/1.1 {} {}\r\n", reply.status, reason(reply.status));
    let mut line = |name: &str, value: &str| head.push_str(&format!("{name}: {value}\r\n"));
    for (name, value) in &reply.headers {
        line(name, value);
    }
    line("Content-Length", &reply.body.len().to_string());
    line("X-Content-Type-Options", "nosniff");
    line("X-Frame-Options", "DENY");
    line("Referrer-Policy", "same-origin");
    line("Cross-Origin-Resource-Policy", "same-origin");
    line(
        "Permissions-Policy",
        "camera=(), microphone=(), geolocation=(), payment=()",
    );
    line("Connection", "close");
    head.push_str("\r\n");
    // The client may be gone already; there is nobody to tell.
    let _ = stream.write_all(head.as_bytes());
    if !head_only {
        let _ = stream.write_all(&reply.body);
    }
    let _ = stream.flush();
}

// --------------------------------------------------------------------------------------------------------- requests

#[derive(Debug, PartialEq, Eq)]
enum Method {
    Get,
    Head,
}

struct Request {
    method: Method,
    target: String,
    host: Option<String>,
    origin: Option<String>,
}

enum Head {
    Complete(Vec<u8>),
    TooLarge,
    Gone,
}

fn read_head(stream: &mut TcpStream) -> Head {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Head::Gone,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
        if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            if end > MAX_HEAD {
                return Head::TooLarge;
            }
            buffer.truncate(end);
            return Head::Complete(buffer);
        }
        if buffer.len() > MAX_HEAD {
            return Head::TooLarge;
        }
    }
}

/// The request line and the two headers that matter. Anything malformed is a 400, never guessed at.
fn parse_request(head: &[u8]) -> std::result::Result<Request, Reply> {
    let bad = |why: &str| Reply::text(400, why);
    let text = std::str::from_utf8(head).map_err(|_| bad("the request is not text"))?;
    let mut lines = text.split("\r\n");
    let mut parts = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(bad("malformed request line"));
    };
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(bad("unsupported HTTP version"));
    }
    let method = match method {
        "GET" => Method::Get,
        "HEAD" => Method::Head,
        _ => {
            let mut reply = Reply::text(405, "only GET and HEAD are served");
            reply.headers.push(("Allow", "GET, HEAD".into()));
            return Err(reply);
        }
    };
    // Origin form only (`/path?query`): no `*`, no absolute URL, and no `//host`, which a join would read as a host.
    if !target.starts_with('/') || target.starts_with("//") || target.chars().any(char::is_control)
    {
        return Err(bad("unsupported request target"));
    }
    let (mut host, mut origin) = (None, None);
    for (count, line) in lines.enumerate() {
        if count >= MAX_HEADERS {
            return Err(Reply::text(431, "too many headers"));
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| bad("malformed header"))?;
        if name.is_empty() || name.contains(char::is_whitespace) {
            return Err(bad("malformed header name"));
        }
        let slot = match name.to_ascii_lowercase().as_str() {
            "host" => &mut host,
            "origin" => &mut origin,
            _ => continue,
        };
        if slot.replace(value.trim().to_string()).is_some() {
            return Err(bad("a header is repeated"));
        }
    }
    Ok(Request {
        method,
        target: target.to_string(),
        host,
        origin,
    })
}

/// Whether the request is addressed to this server by one of its own names, and not sent by another site.
fn is_addressed_to_us(shared: &Shared, request: &Request) -> bool {
    let Some(host) = request.host.as_deref().map(str::to_ascii_lowercase) else {
        return false;
    };
    if !shared.hosts.contains(&host) {
        return false;
    }
    request
        .origin
        .as_deref()
        .is_none_or(|origin| origin.eq_ignore_ascii_case(&format!("http://{host}")))
}

fn serve_connection(mut stream: TcpStream, shared: &Shared) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let head = match read_head(&mut stream) {
        Head::Complete(head) => head,
        Head::TooLarge => {
            return write_reply(
                &mut stream,
                &Reply::text(431, "the request is too large"),
                false,
            );
        }
        Head::Gone => return,
    };
    let request = match parse_request(&head) {
        Ok(request) => request,
        Err(reply) => return write_reply(&mut stream, &reply, false),
    };
    let reply = if is_addressed_to_us(shared, &request) {
        route(shared, &request)
    } else {
        Reply::text(403, "this server answers only to the address it prints")
    };
    write_reply(&mut stream, &reply, request.method == Method::Head);
}

// ----------------------------------------------------------------------------------------------------------- routes

fn route(shared: &Shared, request: &Request) -> Reply {
    let Ok(url) = Url::parse(&format!("http://localhost{}", request.target)) else {
        return Reply::text(400, "unsupported request target");
    };
    let path = url.path();
    if let Some(api) = path.strip_prefix(API_PREFIX) {
        return match api {
            "manifest" => Reply::json(200, &shared.folder.manifest()),
            "version" => Reply::json(200, &json!({ "version": shared.folder.version() })),
            "page" => page(shared, &url),
            _ => Reply::error(404, "not_found"),
        };
    }
    if let Some(file) = path.strip_prefix(APP_PREFIX) {
        return match shared.bundle.get(file) {
            Some(bytes) => {
                // Built files carry their content in their name, so they never change; the two fixed names do.
                let cache = if file.starts_with("assets/") {
                    "public, max-age=31536000, immutable"
                } else {
                    "no-cache"
                };
                Reply::gzip_file(content_type(file), cache, bytes)
            }
            None => Reply::error(404, "not_found"),
        };
    }
    if path.starts_with(RESERVED_PREFIX) {
        return Reply::error(404, "not_found");
    }
    match shared.bundle.get(SHELL) {
        Some(bytes) => Reply::gzip_file("text/html; charset=utf-8", "no-store", bytes),
        None => Reply::text(500, "the embedded preview viewer has no page"),
    }
}

fn page(shared: &Shared, url: &Url) -> Reply {
    let mut paths = url.query_pairs().filter(|(key, _)| key == "path");
    let (Some((_, path)), None) = (paths.next(), paths.next()) else {
        return Reply::error(400, "path_required");
    };
    match shared.folder.page(&path) {
        Ok(content) => Reply::json(200, &json!({ "path": path, "content": content })),
        Err(PageError::NotFound) => Reply::error(404, "not_found"),
        Err(PageError::TooLarge) => Reply::error(413, "too_large"),
        Err(PageError::NotText) => Reply::error(422, "not_text"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::bundle::pack_for_tests;
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::TempDir;

    const GZ: &[u8] = &[0x1f, 0x8b, 8, 0, b'x'];
    const SHELL_GZ: &[u8] = &[0x1f, 0x8b, 8, 0, b's'];

    struct Fixture {
        dir: TempDir,
        server: Running,
    }

    impl Fixture {
        fn new() -> Fixture {
            let dir = TempDir::new().unwrap();
            fs::create_dir_all(dir.path().join("pages")).unwrap();
            fs::write(dir.path().join("pages/index.html"), "<main>hello</main>").unwrap();
            fs::write(dir.path().join("secret.txt"), "top secret").unwrap();
            let bundle = Bundle::parse(&pack_for_tests(&[
                (SHELL, SHELL_GZ),
                ("assets/app-1.js", GZ),
                ("theme-init.js", GZ),
            ]))
            .unwrap();
            let server = Server::bind(Folder::open(dir.path()).unwrap(), bundle, 0)
                .unwrap()
                .spawn();
            Fixture { dir, server }
        }

        fn port(&self) -> u16 {
            self.server.addr().port()
        }

        /// A request with a valid `Host` and the given extra header lines.
        fn get(&self, target: &str, extra: &[&str]) -> Response {
            let mut raw = format!(
                "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n",
                self.port()
            );
            for line in extra {
                raw.push_str(line);
                raw.push_str("\r\n");
            }
            raw.push_str("\r\n");
            self.raw(raw.as_bytes())
        }

        fn raw(&self, bytes: &[u8]) -> Response {
            let mut stream = TcpStream::connect(self.server.addr()).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream.write_all(bytes).unwrap();
            let mut answer = Vec::new();
            let _ = stream.read_to_end(&mut answer);
            Response::parse(&answer)
        }
    }

    struct Response {
        status: u16,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    }

    impl Response {
        fn parse(answer: &[u8]) -> Response {
            let split = answer
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .expect("a complete head");
            let head = std::str::from_utf8(&answer[..split]).unwrap();
            let mut lines = head.split("\r\n");
            let status = lines
                .next()
                .unwrap()
                .split(' ')
                .nth(1)
                .unwrap()
                .parse()
                .unwrap();
            let headers = lines
                .map(|l| {
                    let (k, v) = l.split_once(": ").unwrap();
                    (k.to_ascii_lowercase(), v.to_string())
                })
                .collect();
            Response {
                status,
                headers,
                body: answer[split + 4..].to_vec(),
            }
        }

        fn header(&self, name: &str) -> Option<&str> {
            self.headers.get(name).map(String::as_str)
        }

        fn json(&self) -> serde_json::Value {
            serde_json::from_slice(&self.body).expect("a JSON body")
        }
    }

    #[test]
    fn listens_on_the_loopback_only() {
        let fixture = Fixture::new();
        assert!(fixture.server.addr().ip().is_loopback());
        fixture.server.stop();
    }

    #[test]
    fn serves_the_manifest_in_the_shape_the_viewer_reads() {
        let fixture = Fixture::new();
        let reply = fixture.get("/__speq/api/manifest", &[]);
        assert_eq!(reply.status, 200);
        assert_eq!(
            reply.header("content-type"),
            Some("application/json; charset=utf-8")
        );
        assert_eq!(reply.header("cache-control"), Some("no-store"));
        let manifest = reply.json();
        assert_eq!(manifest["pages"], json!(["pages/index.html"]));
        assert_eq!(
            manifest["personas"],
            json!({ "problem": null, "personas": [] })
        );
        assert_eq!(
            manifest["design"],
            json!({ "problem": null, "content": null })
        );
        assert!(manifest["name"].is_string() && manifest["version"].is_string());
        fixture.server.stop();
    }

    #[test]
    fn serves_a_page_and_never_a_file_that_is_not_one() {
        let fixture = Fixture::new();
        let reply = fixture.get("/__speq/api/page?path=pages%2Findex.html", &[]);
        assert_eq!(reply.status, 200);
        assert_eq!(reply.json()["content"], "<main>hello</main>");

        for (target, status) in [
            ("/__speq/api/page?path=pages%2Fmissing.html", 404),
            ("/__speq/api/page?path=secret.txt", 404),
            ("/__speq/api/page?path=pages%2F..%2Fsecret.txt", 404),
            ("/__speq/api/page?path=..%2Fsecret.txt", 404),
            ("/__speq/api/page?path=%2Fetc%2Fpasswd", 404),
            ("/__speq/api/page", 400),
            ("/__speq/api/page?path=a&path=b", 400),
            ("/__speq/api/nothing", 404),
        ] {
            let reply = fixture.get(target, &[]);
            assert_eq!(reply.status, status, "{target}");
            assert!(
                !String::from_utf8_lossy(&reply.body).contains("top secret"),
                "{target}"
            );
        }
        fixture.server.stop();
    }

    #[test]
    fn the_version_moves_when_a_page_changes() {
        let fixture = Fixture::new();
        let before = fixture.get("/__speq/api/version", &[]).json()["version"].clone();
        assert_eq!(
            fixture.get("/__speq/api/version", &[]).json()["version"],
            before
        );
        fs::write(
            fixture.dir.path().join("pages/index.html"),
            "<main>changed, and longer</main>",
        )
        .unwrap();
        assert_ne!(
            fixture.get("/__speq/api/version", &[]).json()["version"],
            before
        );
        fixture.server.stop();
    }

    #[test]
    fn serves_the_viewers_files_gzipped_as_they_are_stored() {
        let fixture = Fixture::new();
        let asset = fixture.get("/__speq/app/assets/app-1.js", &[]);
        assert_eq!(asset.status, 200);
        assert_eq!(asset.body, GZ);
        assert_eq!(asset.header("content-encoding"), Some("gzip"));
        assert_eq!(
            asset.header("content-type"),
            Some("text/javascript; charset=utf-8")
        );
        assert_eq!(
            asset.header("cache-control"),
            Some("public, max-age=31536000, immutable")
        );
        let fixed = fixture.get("/__speq/app/theme-init.js", &[]);
        assert_eq!(fixed.header("cache-control"), Some("no-cache"));

        for target in [
            "/__speq/app/",
            "/__speq/app/missing.js",
            "/__speq/app/local.html/",
            "/__speq/app/%2e%2e/secret.txt",
            "/__speq/app/assets/../../secret.txt",
        ] {
            let reply = fixture.get(target, &[]);
            assert!(
                reply.status == 404 || reply.body == SHELL_GZ,
                "{target} gave {}",
                reply.status
            );
            assert!(
                !String::from_utf8_lossy(&reply.body).contains("top secret"),
                "{target}"
            );
        }
        fixture.server.stop();
    }

    #[test]
    fn answers_every_address_that_is_not_a_file_with_the_viewer_page() {
        let fixture = Fixture::new();
        for target in [
            "/",
            "/p/index",
            "/p/admin/orders?persona=a#x",
            "/design",
            "/anything",
        ] {
            let reply = fixture.get(target, &[]);
            assert_eq!(reply.status, 200, "{target}");
            assert_eq!(reply.body, SHELL_GZ, "{target}");
            assert_eq!(
                reply.header("content-type"),
                Some("text/html; charset=utf-8"),
                "{target}"
            );
            assert_eq!(reply.header("cache-control"), Some("no-store"), "{target}");
        }
        // The preview's own namespace is never the page.
        assert_eq!(fixture.get("/__speq/unknown", &[]).status, 404);
        fixture.server.stop();
    }

    #[test]
    fn marks_every_answer_as_unembeddable_and_unshared() {
        let fixture = Fixture::new();
        for target in [
            "/",
            "/__speq/api/manifest",
            "/__speq/app/assets/app-1.js",
            "/__speq/api/nothing",
        ] {
            let reply = fixture.get(target, &[]);
            assert_eq!(
                reply.header("x-content-type-options"),
                Some("nosniff"),
                "{target}"
            );
            assert_eq!(reply.header("x-frame-options"), Some("DENY"), "{target}");
            assert_eq!(
                reply.header("referrer-policy"),
                Some("same-origin"),
                "{target}"
            );
            assert_eq!(
                reply.header("cross-origin-resource-policy"),
                Some("same-origin"),
                "{target}"
            );
            assert!(
                reply.header("access-control-allow-origin").is_none(),
                "{target}: no CORS"
            );
            assert!(
                reply.header("content-security-policy").is_none(),
                "{target}: a policy would reach the prototype's frame"
            );
        }
        fixture.server.stop();
    }

    #[test]
    fn answers_only_to_its_own_address() {
        let fixture = Fixture::new();
        let port = fixture.port();
        let with_host = |host: &str| {
            fixture.raw(
                format!("GET /__speq/api/manifest HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes(),
            )
        };
        for host in [
            format!("127.0.0.1:{port}"),
            format!("localhost:{port}"),
            format!("LOCALHOST:{port}"),
            format!("[::1]:{port}"),
        ] {
            assert_eq!(with_host(&host).status, 200, "{host}");
        }
        for host in [
            "evil.example".to_string(),
            format!("evil.example:{port}"),
            format!("127.0.0.1:{}", port.wrapping_add(1)),
            "127.0.0.1".to_string(),
            format!("127.0.0.1.evil.example:{port}"),
            String::new(),
        ] {
            assert_eq!(with_host(&host).status, 403, "{host:?}");
        }
        assert_eq!(
            fixture
                .raw(b"GET /__speq/api/manifest HTTP/1.1\r\n\r\n")
                .status,
            403,
            "no Host"
        );
        fixture.server.stop();
    }

    #[test]
    fn refuses_a_request_another_origin_sent() {
        let fixture = Fixture::new();
        let own = format!("Origin: http://127.0.0.1:{}", fixture.port());
        assert_eq!(fixture.get("/__speq/api/manifest", &[&own]).status, 200);
        for origin in [
            "Origin: https://evil.example",
            "Origin: null",
            "Origin: http://localhost:1",
            &format!("Origin: https://127.0.0.1:{}", fixture.port()),
        ] {
            assert_eq!(
                fixture.get("/__speq/api/manifest", &[origin]).status,
                403,
                "{origin}"
            );
        }
        fixture.server.stop();
    }

    #[test]
    fn serves_get_and_head_and_nothing_else() {
        let fixture = Fixture::new();
        let head = fixture.raw(
            format!(
                "HEAD /__speq/api/manifest HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
                fixture.port()
            )
            .as_bytes(),
        );
        assert_eq!(head.status, 200);
        assert!(head.body.is_empty());
        assert!(
            head.header("content-length")
                .unwrap()
                .parse::<usize>()
                .unwrap()
                > 0
        );

        for method in [
            "POST", "PUT", "DELETE", "OPTIONS", "PATCH", "TRACE", "CONNECT",
        ] {
            let reply = fixture.raw(
                format!(
                    "{method} /__speq/api/manifest HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
                    fixture.port()
                )
                .as_bytes(),
            );
            assert_eq!(reply.status, 405, "{method}");
            assert_eq!(reply.header("allow"), Some("GET, HEAD"), "{method}");
        }
        fixture.server.stop();
    }

    #[test]
    fn refuses_a_malformed_or_oversized_request_without_serving_anything() {
        let fixture = Fixture::new();
        let host = format!("Host: 127.0.0.1:{}", fixture.port());
        for (what, raw) in [
            ("no version", format!("GET /\r\n{host}\r\n\r\n")),
            ("http/2 line", format!("GET / HTTP/2\r\n{host}\r\n\r\n")),
            ("extra part", format!("GET / HTTP/1.1 x\r\n{host}\r\n\r\n")),
            (
                "absolute target",
                format!("GET http://evil.example/ HTTP/1.1\r\n{host}\r\n\r\n"),
            ),
            (
                "authority-like target",
                format!("GET //evil.example/x HTTP/1.1\r\n{host}\r\n\r\n"),
            ),
            ("star target", format!("GET * HTTP/1.1\r\n{host}\r\n\r\n")),
            (
                "header without a colon",
                format!("GET / HTTP/1.1\r\n{host}\r\nbroken\r\n\r\n"),
            ),
            (
                "space in a header name",
                format!("GET / HTTP/1.1\r\n{host}\r\nX Y: z\r\n\r\n"),
            ),
            (
                "repeated Host",
                format!("GET / HTTP/1.1\r\n{host}\r\n{host}\r\n\r\n"),
            ),
            (
                "repeated Origin",
                format!("GET / HTTP/1.1\r\n{host}\r\nOrigin: a\r\nOrigin: a\r\n\r\n"),
            ),
        ] {
            assert_eq!(fixture.raw(raw.as_bytes()).status, 400, "{what}");
        }
        let many = (0..=MAX_HEADERS)
            .map(|i| format!("X-{i}: v\r\n"))
            .collect::<String>();
        assert_eq!(
            fixture
                .raw(format!("GET / HTTP/1.1\r\n{host}\r\n{many}\r\n").as_bytes())
                .status,
            431
        );
        let big = format!("GET /{} HTTP/1.1\r\n{host}\r\n\r\n", "a".repeat(MAX_HEAD));
        assert_eq!(fixture.raw(big.as_bytes()).status, 431);
        fixture.server.stop();
    }

    #[test]
    fn keeps_serving_after_a_client_hangs_up_or_sends_nothing() {
        let fixture = Fixture::new();
        drop(TcpStream::connect(fixture.server.addr()).unwrap());
        let mut partial = TcpStream::connect(fixture.server.addr()).unwrap();
        partial.write_all(b"GET / HTT").unwrap();
        drop(partial);
        assert_eq!(fixture.get("/__speq/api/version", &[]).status, 200);
        fixture.server.stop();
    }

    #[test]
    fn stops_when_asked() {
        let fixture = Fixture::new();
        let addr = fixture.server.addr();
        fixture.server.stop();
        assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_err());
    }

    #[test]
    fn a_taken_port_is_an_error_not_a_panic() {
        let fixture = Fixture::new();
        let again = Server::bind(
            Folder::open(fixture.dir.path()).unwrap(),
            Bundle::default(),
            fixture.port(),
        );
        assert!(matches!(again, Err(Error::Io { .. })));
        fixture.server.stop();
    }
}
