//! Three-way merge with Git-style conflict markers (diff3 layout) for a file changed on both sides.
//!
//! Editors such as VS Code open this layout in a merge editor (Accept Current / Incoming / Both).
//! The labels are specific on purpose: a document that merely mentions `=======` (a Markdown
//! heading underline) or a generic `<<<<<<<` is never mistaken for an unresolved conflict.
use similar::{Algorithm, DiffTag, capture_diff_slices};
use std::ops::Range;

const LOCAL: &str = "<<<<<<< local";
const BASE: &str = "||||||| base";
const SEPARATOR: &str = "=======";
const REMOTE: &str = ">>>>>>> remote";

/// The outcome of merging a file that changed on both sides.
#[derive(Debug, PartialEq, Eq)]
pub enum Merge {
    /// Every change was combined; no conflicting region is left.
    Clean(Vec<u8>),
    /// Non-overlapping changes are combined; each overlapping region is wrapped in markers.
    Conflicted(Vec<u8>),
}

/// A run of base lines `base` replaced by `side` lines of one side.
#[derive(Debug)]
struct Hunk {
    base: Range<usize>,
    side: Range<usize>,
}

fn hunks(base: &[&str], side: &[&str]) -> Vec<Hunk> {
    let mut out: Vec<Hunk> = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, base, side) {
        let (tag, old, new) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            continue;
        }
        match out.last_mut() {
            // A delete followed by an insert is one replacement.
            Some(last) if last.base.end == old.start && last.side.end == new.start => {
                last.base.end = old.end;
                last.side.end = new.end;
            }
            _ => out.push(Hunk {
                base: old,
                side: new,
            }),
        }
    }
    out
}

/// The text of `base[start..end]` as one side has it, with that side's hunks applied.
fn side_text(base: &[&str], side: &[&str], hunks: &[Hunk], start: usize, end: usize) -> String {
    let mut text = String::new();
    let mut at = start;
    for hunk in hunks {
        text.push_str(&base[at..hunk.base.start].concat());
        text.push_str(&side[hunk.side.clone()].concat());
        at = hunk.base.end;
    }
    text.push_str(&base[at..end].concat());
    text
}

fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// Three-way merge by lines, like `git merge-file --diff3`: changes that touch different lines
/// are combined, and only regions both sides changed differently get conflict markers. Changes
/// that touch or share a line are one region, as in Git. Content that is not UTF-8 text cannot
/// be split into lines, so it falls back to one marker block around the three whole files.
pub fn merge(local: &[u8], base: &[u8], remote: &[u8]) -> Merge {
    let (Ok(local), Ok(base), Ok(remote)) = (
        std::str::from_utf8(local),
        std::str::from_utf8(base),
        std::str::from_utf8(remote),
    ) else {
        return Merge::Conflicted(whole_file(local, base, remote));
    };
    let (base, local, remote) = (lines(base), lines(local), lines(remote));
    let (mine, theirs) = (hunks(&base, &local), hunks(&base, &remote));

    let mut out = Vec::new();
    let mut conflicted = false;
    let (mut i, mut j, mut at) = (0, 0, 0);
    while i < mine.len() || j < theirs.len() {
        let start = match (mine.get(i), theirs.get(j)) {
            (Some(a), Some(b)) => a.base.start.min(b.base.start),
            (Some(h), None) | (None, Some(h)) => h.base.start,
            (None, None) => unreachable!("the loop condition guarantees a hunk"),
        };
        // Grow the region until no further hunk of either side touches it.
        let (i0, j0, mut end) = (i, j, start);
        loop {
            let before = (i, j);
            while mine.get(i).is_some_and(|h| h.base.start <= end) {
                end = end.max(mine[i].base.end);
                i += 1;
            }
            while theirs.get(j).is_some_and(|h| h.base.start <= end) {
                end = end.max(theirs[j].base.end);
                j += 1;
            }
            if (i, j) == before {
                break;
            }
        }
        out.extend(base[at..start].iter().flat_map(|l| l.bytes()));
        let ours = side_text(&base, &local, &mine[i0..i], start, end);
        let other = side_text(&base, &remote, &theirs[j0..j], start, end);
        let original: String = base[start..end].concat();
        if ours == other || other == original {
            out.extend_from_slice(ours.as_bytes());
        } else if ours == original {
            out.extend_from_slice(other.as_bytes());
        } else {
            conflicted = true;
            write_block(
                &mut out,
                ours.as_bytes(),
                original.as_bytes(),
                other.as_bytes(),
            );
        }
        at = end;
    }
    out.extend(base[at..].iter().flat_map(|l| l.bytes()));
    if conflicted {
        Merge::Conflicted(out)
    } else {
        Merge::Clean(out)
    }
}

fn whole_file(local: &[u8], base: &[u8], remote: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(local.len() + base.len() + remote.len() + 64);
    write_block(&mut out, local, base, remote);
    out
}

/// One marker block. A section without a trailing newline gets one, so each marker stays on its
/// own line; an absent baseline (both sides added the file) is an empty section.
fn write_block(out: &mut Vec<u8>, local: &[u8], base: &[u8], remote: &[u8]) {
    section(out, LOCAL, local);
    section(out, BASE, base);
    section(out, SEPARATOR, remote);
    out.extend_from_slice(REMOTE.as_bytes());
    out.push(b'\n');
}

fn section(out: &mut Vec<u8>, marker: &str, body: &[u8]) {
    out.extend_from_slice(marker.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(body);
    if !body.is_empty() && !body.ends_with(b"\n") {
        out.push(b'\n');
    }
}

/// True while any marker line written by [`render`] is still in the file.
pub fn has_markers(bytes: &[u8]) -> bool {
    bytes.split(|b| *b == b'\n').any(|line| {
        [LOCAL, BASE, REMOTE]
            .iter()
            .any(|marker| line.starts_with(marker.as_bytes()))
    })
}

/// Lines that are exactly `=======`.
fn separators(bytes: &[u8]) -> usize {
    bytes
        .split(|b| *b == b'\n')
        .filter(|line| line.strip_suffix(b"\r").unwrap_or(line) == SEPARATOR.as_bytes())
        .count()
}

/// True when the file has more `=======` lines than any of the `known` versions of it: a separator
/// left behind after the labelled marker lines were removed. An underline that the document
/// already had (a Markdown heading) does not count.
pub fn has_stray_separator(bytes: &[u8], known: &[&[u8]]) -> bool {
    separators(bytes) > known.iter().map(|k| separators(k)).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(local: &str, base: &str, remote: &str) -> Merge {
        merge(local.as_bytes(), base.as_bytes(), remote.as_bytes())
    }

    fn clean(local: &str, base: &str, remote: &str) -> String {
        match run(local, base, remote) {
            Merge::Clean(bytes) => String::from_utf8(bytes).unwrap(),
            other => panic!("expected a clean merge, got {other:?}"),
        }
    }

    fn conflicted(local: &str, base: &str, remote: &str) -> String {
        match run(local, base, remote) {
            Merge::Conflicted(bytes) => String::from_utf8(bytes).unwrap(),
            other => panic!("expected conflict markers, got {other:?}"),
        }
    }

    const BASE_DOC: &str = "one\ntwo\nthree\nfour\nfive\n";

    #[test]
    fn changes_on_different_lines_are_combined_without_markers() {
        let merged = clean(
            "ONE\ntwo\nthree\nfour\nfive\n",
            BASE_DOC,
            "one\ntwo\nthree\nfour\nFIVE\n",
        );
        assert_eq!(merged, "ONE\ntwo\nthree\nfour\nFIVE\n");
    }

    #[test]
    fn an_insert_and_a_delete_far_apart_are_both_kept() {
        let merged = clean(
            "one\nnew\ntwo\nthree\nfour\nfive\n",
            BASE_DOC,
            "one\ntwo\nthree\nfive\n",
        );
        assert_eq!(merged, "one\nnew\ntwo\nthree\nfive\n");
    }

    #[test]
    fn only_the_overlapping_region_gets_markers() {
        let out = conflicted(
            "ONE\ntwo\nthree\nLOCAL\nfive\n",
            BASE_DOC,
            "one\ntwo\nthree\nREMOTE\nFIVE\n",
        );
        assert_eq!(
            out,
            "ONE\ntwo\nthree\n<<<<<<< local\nLOCAL\nfive\n||||||| base\nfour\nfive\n=======\nREMOTE\nFIVE\n>>>>>>> remote\n"
        );
    }

    #[test]
    fn two_separate_conflicts_get_two_blocks_and_the_text_between_is_untouched() {
        let base = "a\nb\nc\nd\ne\n";
        let out = conflicted("A1\nb\nc\nd\nE1\n", base, "A2\nb\nc\nd\nE2\n");
        assert_eq!(
            out,
            "<<<<<<< local\nA1\n||||||| base\na\n=======\nA2\n>>>>>>> remote\nb\nc\nd\n<<<<<<< local\nE1\n||||||| base\ne\n=======\nE2\n>>>>>>> remote\n"
        );
    }

    #[test]
    fn the_same_change_on_both_sides_is_not_a_conflict() {
        let merged = clean(
            "one\nSAME\nthree\nfour\nfive\n",
            BASE_DOC,
            "one\nSAME\nthree\nfour\nFIVE\n",
        );
        assert_eq!(merged, "one\nSAME\nthree\nfour\nFIVE\n");
    }

    #[test]
    fn adjacent_changes_are_one_conflict_as_in_git() {
        let out = conflicted(
            "one\nTWO\nthree\nfour\nfive\n",
            BASE_DOC,
            "one\ntwo\nTHREE\nfour\nfive\n",
        );
        assert_eq!(
            out,
            "one\n<<<<<<< local\nTWO\nthree\n||||||| base\ntwo\nthree\n=======\ntwo\nTHREE\n>>>>>>> remote\nfour\nfive\n"
        );
    }

    #[test]
    fn a_delete_against_an_edit_of_the_same_line_conflicts_with_an_empty_side() {
        let out = conflicted(
            "one\nthree\nfour\nfive\n",
            BASE_DOC,
            "one\nTWO\nthree\nfour\nfive\n",
        );
        assert_eq!(
            out,
            "one\n<<<<<<< local\n||||||| base\ntwo\n=======\nTWO\n>>>>>>> remote\nthree\nfour\nfive\n"
        );
    }

    #[test]
    fn both_inserting_different_text_at_the_same_spot_conflicts() {
        let out = conflicted("a\nL\nb\n", "a\nb\n", "a\nR\nb\n");
        assert_eq!(
            out,
            "a\n<<<<<<< local\nL\n||||||| base\n=======\nR\n>>>>>>> remote\nb\n"
        );
    }

    #[test]
    fn a_missing_final_newline_is_terminated_only_inside_a_marker_block() {
        let out = conflicted("55555555555", "test", "test2");
        assert_eq!(
            out,
            "<<<<<<< local\n55555555555\n||||||| base\ntest\n=======\ntest2\n>>>>>>> remote\n"
        );
        // Outside a conflict the bytes are preserved exactly.
        assert_eq!(
            clean("a\nb\nc\nd\ne", "a\nb\nc\nd\ne\n", "A\nb\nc\nd\ne\n"),
            "A\nb\nc\nd\ne"
        );
    }

    #[test]
    fn an_absent_baseline_makes_the_whole_file_one_conflict() {
        let out = conflicted("mine\n", "", "theirs\n");
        assert_eq!(
            out,
            "<<<<<<< local\nmine\n||||||| base\n=======\ntheirs\n>>>>>>> remote\n"
        );
    }

    #[test]
    fn crlf_lines_are_compared_and_kept_as_they_are() {
        let merged = clean(
            "ONE\r\ntwo\r\nthree\r\n",
            "one\r\ntwo\r\nthree\r\n",
            "one\r\ntwo\r\nTHREE\r\n",
        );
        assert_eq!(merged, "ONE\r\ntwo\r\nTHREE\r\n");
    }

    #[test]
    fn content_that_is_not_utf8_becomes_one_whole_file_block() {
        let out = match merge(b"\xff\xfe", b"base\n", b"remote\n") {
            Merge::Conflicted(bytes) => bytes,
            other => panic!("expected a conflict, got {other:?}"),
        };
        assert!(out.starts_with(b"<<<<<<< local\n\xff\xfe\n||||||| base\nbase\n"));
        assert!(has_markers(&out));
    }

    #[test]
    fn a_clean_merge_has_no_markers_and_a_conflicted_one_always_does() {
        let merged = clean(
            "ONE\ntwo\nthree\nfour\nfive\n",
            BASE_DOC,
            "one\ntwo\nthree\nfour\nFIVE\n",
        );
        assert!(!has_markers(merged.as_bytes()));
        assert!(has_markers(conflicted("a", "b", "c").as_bytes()));
        assert!(!has_markers(b"# Title\n\nbody\n"));
        assert!(!has_markers(b""));
    }

    #[test]
    fn every_line_of_both_sides_survives_a_merge() {
        // No edit may disappear: each side's new line is in the result, clean or marked.
        for (local, remote) in [
            (
                "L1\ntwo\nthree\nfour\nfive\n",
                "one\ntwo\nthree\nfour\nR5\n",
            ),
            (
                "one\nL2\nthree\nfour\nfive\n",
                "one\nR2\nthree\nfour\nfive\n",
            ),
            ("one\ntwo\nL3\nfour\nfive\n", "one\ntwo\nthree\nR4\nfive\n"),
        ] {
            let out = match run(local, BASE_DOC, remote) {
                Merge::Clean(b) | Merge::Conflicted(b) => String::from_utf8(b).unwrap(),
            };
            for line in local.lines().chain(remote.lines()) {
                if !BASE_DOC.contains(line) {
                    assert!(out.contains(line), "{line:?} lost from {out:?}");
                }
            }
        }
    }

    #[test]
    fn a_separator_the_document_did_not_have_is_stray() {
        let known: [&[u8]; 2] = [b"a\nb\n", b"a\nB\n"];
        assert!(has_stray_separator(b"a\n=======\nB\n", &known));
        assert!(has_stray_separator(b"a\r\n=======\r\nB\r\n", &known));
        assert!(!has_stray_separator(b"a\nB\n", &known));
        assert!(!has_stray_separator(b"", &known));
        assert!(has_stray_separator(b"=======\n", &[]));
    }

    #[test]
    fn an_underline_the_document_already_had_is_not_stray() {
        let known: [&[u8]; 3] = [
            b"Title\n=======\nold\n",
            b"Title\n=======\nmine\n",
            b"Title\n=======\ntheirs\n",
        ];
        assert!(!has_stray_separator(b"Title\n=======\nmerged\n", &known));
        // A second one is a leftover.
        assert!(has_stray_separator(
            b"Title\n=======\nmine\n=======\ntheirs\n",
            &known
        ));
        // Lines that merely contain the text are not separators.
        assert!(!has_stray_separator(
            b"Title\n========\n= ======\n",
            &known[..0]
        ));
    }

    #[test]
    fn any_leftover_marker_line_keeps_the_conflict_open() {
        for leftover in [
            "<<<<<<< local\nmine\n",
            "mine\n||||||| base\n",
            "mine\n>>>>>>> remote\n",
            "mine\r\n>>>>>>> remote\r\n",
        ] {
            assert!(has_markers(leftover.as_bytes()), "{leftover:?}");
        }
    }

    #[test]
    fn ordinary_markdown_is_not_mistaken_for_a_conflict() {
        // Setext heading underline and generic or mid-line marker text must not block a resolution.
        for text in [
            "Title\n=======\n",
            "<<<<<<< HEAD\nfrom git\n",
            "see >>>>>>> remote in prose\n",
            "    <<<<<<< local (indented)\n",
        ] {
            assert!(!has_markers(text.as_bytes()), "{text:?}");
        }
    }
}
