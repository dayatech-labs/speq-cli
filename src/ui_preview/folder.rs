//! The `<product>-ui` folder being previewed, read straight from disk. It exposes exactly what the Speq API exposes of the
//! repository (`pages/**.html`, `personas.json`, `DESIGN.md`) under the same rules (see [`super::rules`]); everything
//! else in the folder does not exist for the preview. Nothing is followed through a link: a symbolic link, wherever
//! it sits on the way to a file, makes that file invisible, so a prototype folder cannot point the preview at files
//! elsewhere on the machine. (A link swapped in between the check and the read is not defended against: whoever can
//! do that already owns the folder.)
use super::rules::{
    MAX_DESIGN_BYTES, MAX_PERSONAS_BYTES, MAX_SLUG_SEGMENTS, PersonasResult, Problem,
    is_file_segment, is_page_path, parse_design, parse_personas,
};
use crate::api::printable;
use crate::error::{Error, Result};
use crate::hashing::sha256_hex;
use serde::Serialize;
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// A page is HTML a person wrote by hand or a design tool exported; this is far above either.
const MAX_PAGE_BYTES: u64 = 2 * 1024 * 1024;
/// A prototype of more pages than this is not one a person previews; it also bounds a walk of a huge folder.
const MAX_PAGES: usize = 1000;
const PERSONAS_FILE: &str = "personas.json";
const DESIGN_FILE: &str = "DESIGN.md";

/// Why a page cannot be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageError {
    NotFound,
    TooLarge,
    NotText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DesignResult {
    pub problem: Option<Problem>,
    pub content: Option<String>,
}

/// What the viewer needs to draw the folder, in the shape of the hosted app's data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Manifest {
    pub name: String,
    /// Changes whenever a file the preview reads changes.
    pub version: String,
    /// `pages/<slug>.html`, sorted.
    pub pages: Vec<String>,
    pub personas: PersonasResult,
    pub design: DesignResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified_ns: u128,
}

impl Stamp {
    fn of(meta: &Metadata) -> Stamp {
        let modified_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        Stamp {
            len: meta.len(),
            modified_ns,
        }
    }
}

/// Every file the preview reads, as it is on disk now.
struct Scan {
    pages: Vec<(String, Stamp)>,
    personas: Option<Stamp>,
    design: Option<Stamp>,
}

impl Scan {
    fn version(&self) -> String {
        let mut text = String::new();
        let mut add = |path: &str, stamp: &Stamp| {
            text.push_str(&format!("{path}\0{}\0{}\n", stamp.len, stamp.modified_ns));
        };
        for (path, stamp) in &self.pages {
            add(path, stamp);
        }
        if let Some(stamp) = &self.personas {
            add(PERSONAS_FILE, stamp);
        }
        if let Some(stamp) = &self.design {
            add(DESIGN_FILE, stamp);
        }
        sha256_hex(text.as_bytes())[..16].to_string()
    }
}

#[derive(Debug, Clone)]
pub struct Folder {
    root: PathBuf,
    name: String,
}

/// A regular file at `rel` under `root`, reached without passing a symbolic link.
fn regular_file(root: &Path, rel: &[&str]) -> Option<(PathBuf, Metadata)> {
    let mut path = root.to_path_buf();
    for (i, segment) in rel.iter().enumerate() {
        path.push(segment);
        let meta = fs::symlink_metadata(&path).ok()?;
        if meta.file_type().is_symlink() {
            return None;
        }
        if i + 1 == rel.len() {
            return meta.is_file().then_some((path, meta));
        }
        if !meta.is_dir() {
            return None;
        }
    }
    None
}

fn read_up_to(path: &Path, max: u64) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(max + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

impl Folder {
    /// The folder at `dir`. It is the one place a link is followed: the folder itself is the user's own choice.
    pub fn open(dir: &Path) -> Result<Folder> {
        let root = fs::canonicalize(dir).map_err(|_| {
            Error::invalid(format!(
                "{} is not a folder; give the folder of a <product>-ui repository",
                printable(&dir.display().to_string())
            ))
        })?;
        if !root.is_dir() {
            return Err(Error::invalid(format!(
                "{} is not a folder; give the folder of a <product>-ui repository",
                printable(&dir.display().to_string())
            )));
        }
        let name = root
            .file_name()
            .map(|n| printable(&n.to_string_lossy()).trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "ui".to_string());
        Ok(Folder { root, name })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn scan(&self) -> Scan {
        let mut pages = Vec::new();
        self.walk(&self.root.join("pages"), "", 1, &mut pages);
        pages.sort_by(|a, b| a.0.cmp(&b.0));
        let stamp =
            |name: &str| regular_file(&self.root, &[name]).map(|(_, meta)| Stamp::of(&meta));
        Scan {
            pages,
            personas: stamp(PERSONAS_FILE),
            design: stamp(DESIGN_FILE),
        }
    }

    /// The pages under `dir` (`rel` is its path under `pages/`), at depth `depth` slug segments.
    fn walk(&self, dir: &Path, rel: &str, depth: usize, out: &mut Vec<(String, Stamp)>) {
        // `pages` or a folder in it that is a link is not entered: `read_dir` would follow it.
        if fs::symlink_metadata(dir).is_ok_and(|m| !m.is_dir()) {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if out.len() >= MAX_PAGES {
                return;
            }
            let (Ok(name), Ok(kind)) = (entry.file_name().into_string(), entry.file_type()) else {
                continue;
            };
            if !is_file_segment(&name) {
                continue;
            }
            let child = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if kind.is_dir() {
                if depth < MAX_SLUG_SEGMENTS {
                    self.walk(&entry.path(), &child, depth + 1, out);
                }
            } else if kind.is_file() {
                let path = format!("pages/{child}");
                if is_page_path(&path)
                    && let Ok(meta) = entry.metadata()
                {
                    out.push((path, Stamp::of(&meta)));
                }
            }
        }
    }

    /// The folder's pages, personas and design guide, with the version they are at.
    pub fn manifest(&self) -> Manifest {
        let scan = self.scan();
        let personas = match scan.personas {
            None => PersonasResult::none(),
            Some(_) => match self.read_root_file(PERSONAS_FILE, MAX_PERSONAS_BYTES) {
                Ok(bytes) => parse_personas(&bytes),
                Err(problem) => PersonasResult::problem(problem),
            },
        };
        let design = match scan.design {
            None => DesignResult {
                problem: None,
                content: None,
            },
            Some(_) => match self
                .read_root_file(DESIGN_FILE, MAX_DESIGN_BYTES)
                .and_then(|bytes| parse_design(&bytes).ok_or(Problem::Invalid))
            {
                Ok(content) => DesignResult {
                    problem: None,
                    content: Some(content),
                },
                Err(problem) => DesignResult {
                    problem: Some(problem),
                    content: None,
                },
            },
        };
        Manifest {
            name: self.name.clone(),
            version: scan.version(),
            pages: scan.pages.iter().map(|(path, _)| path.clone()).collect(),
            personas,
            design,
        }
    }

    /// Changes whenever a file the preview reads is added, removed or changed.
    pub fn version(&self) -> String {
        self.scan().version()
    }

    /// The text of `path` (`pages/<slug>.html`), only when it is a page the folder exposes.
    pub fn page(&self, path: &str) -> std::result::Result<String, PageError> {
        if !is_page_path(path) {
            return Err(PageError::NotFound);
        }
        let rel: Vec<&str> = path.split('/').collect();
        let (file, meta) = regular_file(&self.root, &rel).ok_or(PageError::NotFound)?;
        if meta.len() > MAX_PAGE_BYTES {
            return Err(PageError::TooLarge);
        }
        let bytes = read_up_to(&file, MAX_PAGE_BYTES).ok_or(PageError::NotFound)?;
        if bytes.len() as u64 > MAX_PAGE_BYTES {
            return Err(PageError::TooLarge);
        }
        String::from_utf8(bytes).map_err(|_| PageError::NotText)
    }

    /// A file at the root, up to `max` bytes. One that is too large is `TooLarge`; one that vanished or cannot be read
    /// since the scan is `Invalid`, which the viewer shows as a file it cannot use.
    fn read_root_file(&self, name: &str, max: u64) -> std::result::Result<Vec<u8>, Problem> {
        let (path, _) = regular_file(&self.root, &[name]).ok_or(Problem::Invalid)?;
        let bytes = read_up_to(&path, max).ok_or(Problem::Invalid)?;
        if bytes.len() as u64 > max {
            return Err(Problem::TooLarge);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(dir: &TempDir, path: &str, content: &str) {
        let file = dir.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, content).unwrap();
    }

    fn folder(dir: &TempDir) -> Folder {
        Folder::open(dir.path()).unwrap()
    }

    #[test]
    fn lists_only_the_pages_the_api_would() {
        let dir = TempDir::new().unwrap();
        for path in [
            "pages/index.html",
            "pages/admin/orders.html",
            "pages/admin/deep/er/x.html",
            "pages/notes.txt",
            "pages/.hidden.html",
            "pages/has space.html",
            "pages/.git/config.html",
            "other/index.html",
            "index.html",
        ] {
            write(&dir, path, "<p>x</p>");
        }
        assert_eq!(
            folder(&dir).manifest().pages,
            [
                "pages/admin/deep/er/x.html",
                "pages/admin/orders.html",
                "pages/index.html"
            ]
        );
    }

    #[test]
    fn a_folder_without_pages_is_empty_not_an_error() {
        let dir = TempDir::new().unwrap();
        let manifest = folder(&dir).manifest();
        assert!(manifest.pages.is_empty());
        assert_eq!(manifest.personas, PersonasResult::none());
        assert_eq!(manifest.design.content, None);
        assert_eq!(manifest.design.problem, None);
    }

    #[test]
    fn stops_at_the_depth_the_api_allows() {
        let dir = TempDir::new().unwrap();
        write(&dir, "pages/a/b/c/d/e/f/g.html", "x"); // 7 segments: the deepest slug
        write(&dir, "pages/a/b/c/d/e/f/g/h.html", "x"); // 8
        assert_eq!(folder(&dir).manifest().pages, ["pages/a/b/c/d/e/f/g.html"]);
    }

    #[test]
    fn names_the_product_after_its_folder() {
        let dir = TempDir::new().unwrap();
        let named = dir.path().join("checkout-ui");
        fs::create_dir(&named).unwrap();
        assert_eq!(Folder::open(&named).unwrap().manifest().name, "checkout-ui");
    }

    #[test]
    fn refuses_what_is_not_a_folder() {
        let dir = TempDir::new().unwrap();
        write(&dir, "file.txt", "x");
        for path in [dir.path().join("missing"), dir.path().join("file.txt")] {
            let error = Folder::open(&path).unwrap_err();
            assert!(matches!(error, Error::Invalid(_)), "{error}");
        }
    }

    #[test]
    fn reads_personas_and_the_design_guide() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "personas.json",
            r#"{"personas":[{"id":"tamu","name":"Tamu"}]}"#,
        );
        write(&dir, "DESIGN.md", "\u{FEFF}# Design\n");
        let manifest = folder(&dir).manifest();
        assert_eq!(manifest.personas.problem, None);
        assert_eq!(manifest.personas.personas[0].id, "tamu");
        assert_eq!(manifest.design.content.as_deref(), Some("# Design\n"));
    }

    #[test]
    fn reports_unusable_personas_and_design_as_problems_without_their_content() {
        let dir = TempDir::new().unwrap();
        write(&dir, "personas.json", "{ nope");
        write(
            &dir,
            "DESIGN.md",
            &"x".repeat(MAX_DESIGN_BYTES as usize + 1),
        );
        let manifest = folder(&dir).manifest();
        assert_eq!(manifest.personas, PersonasResult::problem(Problem::Invalid));
        assert_eq!(manifest.design.problem, Some(Problem::TooLarge));
        assert_eq!(manifest.design.content, None);

        write(
            &dir,
            "personas.json",
            &" ".repeat(MAX_PERSONAS_BYTES as usize + 1),
        );
        fs::write(dir.path().join("DESIGN.md"), [0xff, 0xfe, 0x00]).unwrap();
        let manifest = folder(&dir).manifest();
        assert_eq!(
            manifest.personas,
            PersonasResult::problem(Problem::TooLarge)
        );
        assert_eq!(manifest.design.problem, Some(Problem::Invalid));
    }

    #[test]
    fn serves_a_page_and_only_a_page() {
        let dir = TempDir::new().unwrap();
        write(&dir, "pages/index.html", "<main>hello</main>");
        write(&dir, "secret.html", "no");
        write(&dir, "pages/notes.txt", "no");
        let folder = folder(&dir);
        assert_eq!(
            folder.page("pages/index.html").unwrap(),
            "<main>hello</main>"
        );
        for path in [
            "pages/missing.html",
            "pages/notes.txt",
            "secret.html",
            "pages/../secret.html",
            "../secret.html",
            "/etc/passwd",
            "pages//index.html",
            "pages/index.html/",
            "personas.json",
            "",
        ] {
            assert_eq!(folder.page(path), Err(PageError::NotFound), "{path:?}");
        }
    }

    #[test]
    fn a_directory_named_like_a_page_is_not_a_page() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("pages/trick.html")).unwrap();
        let folder = folder(&dir);
        assert_eq!(folder.page("pages/trick.html"), Err(PageError::NotFound));
        assert!(folder.manifest().pages.is_empty());
    }

    #[test]
    fn refuses_a_page_that_is_too_large_or_not_text() {
        let dir = TempDir::new().unwrap();
        write(
            &dir,
            "pages/big.html",
            &"x".repeat(MAX_PAGE_BYTES as usize + 1),
        );
        fs::write(dir.path().join("pages/binary.html"), [0xff, 0xfe, 0x00]).unwrap();
        let folder = folder(&dir);
        assert_eq!(folder.page("pages/big.html"), Err(PageError::TooLarge));
        assert_eq!(folder.page("pages/binary.html"), Err(PageError::NotText));
    }

    #[test]
    fn the_version_follows_what_the_preview_reads_and_nothing_else() {
        let dir = TempDir::new().unwrap();
        write(&dir, "pages/index.html", "one");
        let folder = folder(&dir);
        let first = folder.version();
        assert_eq!(folder.version(), first);
        assert_eq!(folder.manifest().version, first);

        write(&dir, "unrelated/readme.md", "not exposed");
        write(&dir, "pages/notes.txt", "not a page");
        assert_eq!(
            folder.version(),
            first,
            "files the preview does not read change nothing"
        );

        write(&dir, "pages/index.html", "one, longer");
        let edited = folder.version();
        assert_ne!(edited, first);
        write(&dir, "pages/two.html", "x");
        let added = folder.version();
        assert_ne!(added, edited);
        write(&dir, "DESIGN.md", "# d");
        assert_ne!(folder.version(), added);
    }

    #[cfg(unix)]
    mod links {
        use super::*;
        use std::os::unix::fs::symlink;

        #[test]
        fn a_link_hides_a_file_whether_it_is_the_page_or_a_folder_on_the_way() {
            let dir = TempDir::new().unwrap();
            let outside = TempDir::new().unwrap();
            write(&outside, "stolen.html", "outside");
            write(
                &outside,
                "personas.json",
                r#"{"personas":[{"id":"x","name":"X"}]}"#,
            );
            write(&dir, "pages/index.html", "inside");
            symlink(
                outside.path().join("stolen.html"),
                dir.path().join("pages/link.html"),
            )
            .unwrap();
            symlink(outside.path(), dir.path().join("pages/dir")).unwrap();
            symlink(
                outside.path().join("personas.json"),
                dir.path().join("personas.json"),
            )
            .unwrap();

            let folder = folder(&dir);
            let manifest = folder.manifest();
            assert_eq!(manifest.pages, ["pages/index.html"]);
            assert_eq!(manifest.personas, PersonasResult::none());
            assert_eq!(folder.page("pages/link.html"), Err(PageError::NotFound));
            assert_eq!(
                folder.page("pages/dir/stolen.html"),
                Err(PageError::NotFound)
            );
        }

        #[test]
        fn a_pages_folder_that_is_a_link_exposes_nothing() {
            let dir = TempDir::new().unwrap();
            let outside = TempDir::new().unwrap();
            write(&outside, "index.html", "outside");
            symlink(outside.path(), dir.path().join("pages")).unwrap();
            let folder = folder(&dir);
            assert!(folder.manifest().pages.is_empty());
            assert_eq!(folder.page("pages/index.html"), Err(PageError::NotFound));
        }
    }
}
