//! `speq ui preview [dir]`: a `<product>-ui` folder on disk, shown in the browser the way the Speq Web app shows it.
//! The viewer is the Web app's own prototype view (the same sandboxed frame, personas, states and component list),
//! built into this binary; it reads the folder's pages through a small local server and reloads when a file changes.
//! Nothing here touches the Speq API or the device's session: a preview needs no login and sends nothing anywhere.
pub mod bundle;
pub mod folder;
pub mod rules;
pub mod server;

use crate::api::printable;
use crate::error::{Result, exit};
use crate::output::emit;
use crate::session::open_in_browser;
use bundle::Bundle;
use folder::Folder;
use serde::Serialize;
use server::Server;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use url::Url;

/// What a started preview tells: where to open it. With `--json` this is the document on stdout, printed as soon as the
/// server is listening, so a script or an agent can read the address and go on while the preview keeps running.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PreviewReport {
    pub url: String,
    /// The folder being previewed.
    pub folder: String,
    pub pages: usize,
}

/// Opens `dir` and starts listening, without serving yet.
pub fn start(dir: &Path, port: u16) -> Result<(Server, PreviewReport)> {
    let folder = Folder::open(dir)?;
    let report_folder = printable(&folder.root().display().to_string());
    let pages = folder.manifest().pages.len();
    let server = Server::bind(folder, Bundle::embedded()?, port)?;
    let report = PreviewReport {
        url: server.url(),
        folder: report_folder,
        pages,
    };
    Ok((server, report))
}

/// Starts the preview and serves it until the process is stopped (Ctrl+C).
pub fn run(json: bool, dir: &Path, port: u16, open: bool) -> Result<u8> {
    let (server, report) = start(dir, port)?;
    emit(json, &report);
    if report.pages == 0 {
        eprintln!(
            "note: no pages yet; add HTML files to pages/ in this folder (the preview reloads on its own)"
        );
    }
    if open && !Url::parse(&report.url).is_ok_and(|url| open_in_browser(&url)) {
        eprintln!(
            "note: could not open a browser; open {} yourself",
            report.url
        );
    }
    server.run(&AtomicBool::new(false));
    Ok(exit::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn starts_on_a_free_loopback_port_and_says_where_with_the_page_count() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("pages")).unwrap();
        std::fs::write(dir.path().join("pages/index.html"), "<p>hi</p>").unwrap();
        let (server, report) = start(dir.path(), 0).unwrap();
        assert!(report.url.starts_with("http://127.0.0.1:"));
        assert!(report.url.ends_with('/'));
        assert_eq!(report.url, server.url());
        assert_eq!(report.pages, 1);
        assert!(server.addr().ip().is_loopback());
        assert!(server.addr().port() > 0);
    }

    #[test]
    fn refuses_a_folder_that_is_not_there() {
        let dir = TempDir::new().unwrap();
        assert!(start(&dir.path().join("nope"), 0).is_err());
    }
}
