//! `speq ui preview`, the real binary: it starts without a login, says where it listens, serves the folder it was given
//! and the viewer built into it, and fails cleanly when it cannot start. The server's rules (hosts, origins, paths,
//! limits) are tested in `src/ui_preview/`; this is the command around them.
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// The binary with a home of its own, so it can neither read nor change the developer's real speq state.
fn speq(home: &Path) -> Command {
    // Windows resolves the app-data folders under `%USERPROFILE%\AppData` and needs both to exist.
    for dir in ["Local", "Roaming"] {
        std::fs::create_dir_all(home.join("AppData").join(dir)).expect("app data");
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_speq"));
    command
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove("SPEQ_API_URL")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("NO_COLOR");
    command
}

/// A running preview, stopped when it goes out of scope.
struct Preview {
    child: Child,
    addr: SocketAddr,
    report: Value,
}

impl Drop for Preview {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(home: &Path, folder: &Path) -> Preview {
    let mut child = speq(home)
        .args(["--json", "ui", "preview"])
        .arg(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("run speq");
    // `--json` prints the report as an indented document, then the command keeps serving: read up to its closing brace.
    let mut lines = BufReader::new(child.stdout.take().expect("stdout"));
    let mut text = String::new();
    loop {
        let mut line = String::new();
        let read = lines.read_line(&mut line).expect("read the report");
        assert!(
            read > 0,
            "the command ended before it said where it listens: {text}"
        );
        text.push_str(&line);
        if line.trim_end() == "}" {
            break;
        }
    }
    let report: Value = serde_json::from_str(&text).expect("the report is JSON");
    let url = report["url"].as_str().expect("a url");
    let addr = url
        .strip_prefix("http://")
        .and_then(|rest| rest.strip_suffix('/'))
        .and_then(|host| host.parse().ok())
        .expect("an http://127.0.0.1:<port>/ address");
    Preview {
        child,
        addr,
        report,
    }
}

struct Reply {
    status: u16,
    head: String,
    body: Vec<u8>,
}

fn get(preview: &Preview, target: &str) -> Reply {
    let mut stream = TcpStream::connect(preview.addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        preview.addr
    )
    .expect("send");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).expect("read");
    let split = answer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("a complete head");
    let head = String::from_utf8_lossy(&answer[..split]).into_owned();
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("a status");
    Reply {
        status,
        head,
        body: answer[split + 4..].to_vec(),
    }
}

fn prototype(dir: &Path) {
    std::fs::create_dir_all(dir.join("pages/admin")).unwrap();
    std::fs::write(
        dir.join("pages/index.html"),
        "<main data-component=\"hero\">Halo</main>",
    )
    .unwrap();
    std::fs::write(dir.join("pages/admin/orders.html"), "<p>Pesanan</p>").unwrap();
    std::fs::write(
        dir.join("personas.json"),
        r#"{"personas":[{"id":"admin","name":"Admin","start":"admin/orders","pages":["admin/*"]}]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("DESIGN.md"), "# Desain\n").unwrap();
    std::fs::write(dir.join("notes.txt"), "not part of the prototype").unwrap();
}

#[test]
fn previews_a_folder_without_a_login_and_serves_it_with_the_embedded_viewer() {
    let home = tempfile::tempdir().expect("home");
    let folder = tempfile::tempdir().expect("folder");
    prototype(folder.path());
    let preview = start(home.path(), folder.path());

    assert_eq!(preview.report["pages"], 2);
    assert!(preview.addr.ip().is_loopback());
    assert!(
        preview.report["folder"]
            .as_str()
            .is_some_and(|f| !f.is_empty())
    );

    let manifest = get(&preview, "/__speq/api/manifest");
    assert_eq!(manifest.status, 200);
    let manifest: Value = serde_json::from_slice(&manifest.body).expect("JSON");
    assert_eq!(
        manifest["pages"],
        serde_json::json!(["pages/admin/orders.html", "pages/index.html"])
    );
    assert_eq!(manifest["personas"]["personas"][0]["id"], "admin");
    assert_eq!(manifest["design"]["content"], "# Desain\n");

    let page = get(&preview, "/__speq/api/page?path=pages%2Findex.html");
    assert_eq!(page.status, 200);
    let page: Value = serde_json::from_slice(&page.body).expect("JSON");
    assert_eq!(page["content"], "<main data-component=\"hero\">Halo</main>");
    assert_eq!(get(&preview, "/__speq/api/page?path=notes.txt").status, 404);

    // The viewer: its page answers a page address, and is the built-in gzip file the server stores.
    let shell = get(&preview, "/p/admin/orders");
    assert_eq!(shell.status, 200);
    assert!(
        shell
            .head
            .to_ascii_lowercase()
            .contains("content-encoding: gzip")
    );
    assert_eq!(&shell.body[..2], &[0x1f, 0x8b], "gzip");
    assert_eq!(get(&preview, "/").body, shell.body);
}

#[test]
fn sees_a_change_on_disk_through_the_version() {
    let home = tempfile::tempdir().expect("home");
    let folder = tempfile::tempdir().expect("folder");
    prototype(folder.path());
    let preview = start(home.path(), folder.path());

    let version = |preview: &Preview| -> String {
        let reply = get(preview, "/__speq/api/version");
        let json: Value = serde_json::from_slice(&reply.body).expect("JSON");
        json["version"].as_str().expect("a version").to_string()
    };
    let before = version(&preview);
    std::fs::write(folder.path().join("pages/new.html"), "<p>baru</p>").unwrap();
    assert_ne!(version(&preview), before);
}

#[test]
fn a_taken_port_ends_the_command_with_an_error() {
    let home = tempfile::tempdir().expect("home");
    let folder = tempfile::tempdir().expect("folder");
    let taken = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = taken.local_addr().unwrap().port().to_string();
    let out = speq(home.path())
        .args(["ui", "preview"])
        .arg(folder.path())
        .args(["--port", &port])
        .stdin(Stdio::null())
        .output()
        .expect("run speq");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot listen"));
    assert!(
        out.stdout.is_empty(),
        "nothing is announced when it cannot start"
    );
}

#[test]
fn a_folder_that_is_not_there_is_invalid_usage_of_the_folder_not_a_crash() {
    let home = tempfile::tempdir().expect("home");
    let folder = tempfile::tempdir().expect("folder");
    let out = speq(home.path())
        .args(["ui", "preview"])
        .arg(folder.path().join("missing"))
        .stdin(Stdio::null())
        .output()
        .expect("run speq");
    assert_eq!(out.status.code(), Some(7));
    assert!(String::from_utf8_lossy(&out.stderr).contains("is not a folder"));
}

#[test]
fn help_describes_the_preview() {
    let home = tempfile::tempdir().expect("home");
    let out = speq(home.path())
        .args(["ui", "preview", "--help"])
        .output()
        .expect("run speq");
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    for needle in ["--port", "--open", "127.0.0.1", "Ctrl+C", "personas.json"] {
        assert!(text.contains(needle), "{needle} in:\n{text}");
    }
}
