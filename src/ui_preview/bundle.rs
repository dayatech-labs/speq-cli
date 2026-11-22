//! The files of the preview viewer (the hosted app's prototype view, built by speq-app's `bun run build:local`), as the
//! binary embeds them: one pack, every file already gzipped, so the server hands them out as they are with
//! `Content-Encoding: gzip`. The writer is speq-app's `scripts/pack-bundle.mjs`; change both together.
//!
//! Layout, little-endian: `"SPQB"`, count `u32`, then per file: path length `u16`, path (UTF-8), length `u32`, gzip bytes.
use crate::error::{Error, Result};
use std::collections::BTreeMap;

/// The viewer as built into this binary. Rebuilt with `bun run build:local ../speq-cli/assets/ui-preview.bin`.
static EMBEDDED: &[u8] = include_bytes!("../../assets/ui-preview.bin");

const MAGIC: &[u8; 4] = b"SPQB";
/// The viewer is a few hundred files at most; a pack that claims more is not one of ours.
const MAX_FILES: usize = 4096;
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];
/// The page every address of the viewer that is not a file or an API call is answered with.
pub const SHELL: &str = "local.html";

/// Files by their path in the build folder (`local.html`, `assets/x.js`), each as the gzip bytes to send.
#[derive(Debug, Clone, Default)]
pub struct Bundle {
    files: BTreeMap<String, Vec<u8>>,
}

fn damaged(why: &str) -> Error {
    Error::Other(format!("the embedded preview viewer is damaged: {why}"))
}

/// A path as the viewer's own build writes it: relative, with plain segments, so a lookup by it can never leave the pack.
fn is_plain_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && !segment.contains('\\')
                && !segment.chars().any(char::is_control)
        })
}

impl Bundle {
    /// The viewer built into this binary.
    pub fn embedded() -> Result<Bundle> {
        Bundle::parse(EMBEDDED)
    }

    /// A pack in the layout above. Anything that is not exactly that layout is refused whole.
    pub fn parse(bytes: &[u8]) -> Result<Bundle> {
        let mut reader = Reader { bytes, at: 0 };
        if reader.take(4)? != MAGIC {
            return Err(damaged("wrong header"));
        }
        let count = usize::try_from(u32::from_le_bytes(reader.array()?)).unwrap_or(usize::MAX);
        if count > MAX_FILES {
            return Err(damaged("too many files"));
        }
        let mut files = BTreeMap::new();
        for _ in 0..count {
            let name_length = usize::from(u16::from_le_bytes(reader.array()?));
            let path = std::str::from_utf8(reader.take(name_length)?)
                .map_err(|_| damaged("a path is not text"))?
                .to_string();
            if !is_plain_path(&path) {
                return Err(damaged("a path is not a plain relative path"));
            }
            let length = usize::try_from(u32::from_le_bytes(reader.array()?)).unwrap_or(usize::MAX);
            let body = reader.take(length)?;
            if !body.starts_with(&GZIP_MAGIC) {
                return Err(damaged("a file is not gzip"));
            }
            if files.insert(path, body.to_vec()).is_some() {
                return Err(damaged("a path is there twice"));
            }
        }
        if reader.at != bytes.len() {
            return Err(damaged("bytes after the last file"));
        }
        Ok(Bundle { files })
    }

    /// The gzip bytes of the file at `path`, when the pack has it.
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len());
        let end = end.ok_or_else(|| damaged("it ends early"))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("take gave N bytes"))
    }
}

/// A pack in the layout above, for tests that need a viewer of their own.
#[cfg(test)]
pub(crate) fn pack_for_tests(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend((files.len() as u32).to_le_bytes());
    for (path, body) in files {
        out.extend((path.len() as u16).to_le_bytes());
        out.extend(path.as_bytes());
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(*body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GZ: &[u8] = &[0x1f, 0x8b, 8, 0];

    #[test]
    fn reads_what_the_writer_lays_out() {
        let bundle =
            Bundle::parse(&pack_for_tests(&[("local.html", GZ), ("assets/a.js", GZ)])).unwrap();
        assert_eq!(bundle.get("local.html"), Some(GZ));
        assert_eq!(bundle.get("assets/a.js"), Some(GZ));
        assert_eq!(bundle.get("assets"), None);
        assert_eq!(
            bundle.paths().collect::<Vec<_>>(),
            ["assets/a.js", "local.html"]
        );
    }

    #[test]
    fn refuses_anything_that_is_not_exactly_the_layout() {
        let good = pack_for_tests(&[("local.html", GZ)]);
        assert!(Bundle::parse(&good).is_ok());

        let mut wrong_magic = good.clone();
        wrong_magic[0] = b'X';
        let mut trailing = good.clone();
        trailing.push(0);
        let truncated = &good[..good.len() - 1];
        let mut too_many = good.clone();
        too_many[4..8].copy_from_slice(&u32::MAX.to_le_bytes());

        for (what, bytes) in [
            ("wrong header", wrong_magic),
            ("trailing bytes", trailing),
            ("truncated", truncated.to_vec()),
            ("count too large", too_many),
            ("empty", Vec::new()),
            ("not gzip", pack_for_tests(&[("local.html", b"<html>")])),
            ("duplicate", pack_for_tests(&[("a", GZ), ("a", GZ)])),
        ] {
            assert!(Bundle::parse(&bytes).is_err(), "{what}");
        }
    }

    #[test]
    fn refuses_a_path_that_could_leave_the_pack() {
        for path in [
            "", "/a", "a/", "a//b", "../a", "a/../b", "./a", "a\\b", "a\nb",
        ] {
            assert!(
                Bundle::parse(&pack_for_tests(&[(path, GZ)])).is_err(),
                "{path:?} should be refused"
            );
        }
    }

    #[test]
    fn the_embedded_viewer_parses_and_holds_its_shell_and_scripts() {
        let bundle = Bundle::embedded().expect("the embedded pack parses");
        let shell = bundle.get(SHELL).expect("the shell is in the pack");
        assert!(shell.starts_with(&GZIP_MAGIC));
        for path in ["theme-init.js", "favicon.svg"] {
            assert!(bundle.get(path).is_some(), "{path} is in the pack");
        }
        assert!(
            bundle
                .paths()
                .any(|p| p.starts_with("assets/") && p.ends_with(".js")),
            "the pack holds the viewer's scripts"
        );
    }
}
