//! The one template behind every human-readable message, errors included. `--help` is Clap's and stays separate.
//!
//! ```text
//! ✔ Title                        icon by tone: ✔ done · ! needs attention · ℹ info · ✖ error
//!
//!   label     value              facts: dim labels, aligned
//!
//!   Section (2)                  a list of items; each item has a glyph, a label, and a dim detail
//!     + first.md   detail
//!     ~ second.md
//!
//!   ! A note.                    a note in the tone's color
//!
//!   → The next step              hint: what to do next
//!
//!   dim meta line                error code, status, and so on
//! ```
//!
//! Blocks are separated by one blank line and indented by two spaces, so a command shows only the blocks it has.
//! Colors are added only on a terminal and never when `NO_COLOR` is set.
use clap::builder::styling::{AnsiColor, Effects, Style};
use std::io::IsTerminal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Warn,
    Info,
    Fail,
}

impl Tone {
    fn glyph(self) -> &'static str {
        match self {
            Tone::Ok => "✔",
            Tone::Warn => "!",
            Tone::Info => "ℹ",
            Tone::Fail => "✖",
        }
    }

    fn style(self) -> Style {
        let color = match self {
            Tone::Ok => AnsiColor::Green,
            Tone::Warn => AnsiColor::Yellow,
            Tone::Info => AnsiColor::Cyan,
            Tone::Fail => AnsiColor::Red,
        };
        color.on_default().effects(Effects::BOLD)
    }
}

/// Whether ANSI colors are wanted for one output stream.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    color: bool,
}

fn wants_color(is_terminal: bool) -> bool {
    is_terminal && std::env::var_os("NO_COLOR").is_none()
}

impl Theme {
    pub fn new(color: bool) -> Self {
        Theme { color }
    }

    pub fn stdout() -> Self {
        Theme::new(wants_color(std::io::stdout().is_terminal()))
    }

    pub fn stderr() -> Self {
        Theme::new(wants_color(std::io::stderr().is_terminal()))
    }

    fn paint(&self, style: Style, text: &str) -> String {
        if self.color {
            format!("{style}{text}{style:#}")
        } else {
            text.to_string()
        }
    }

    fn tone(&self, tone: Tone, text: &str) -> String {
        self.paint(tone.style(), text)
    }

    fn dim(&self, text: &str) -> String {
        self.paint(AnsiColor::BrightBlack.on_default(), text)
    }

    fn bold(&self, text: &str) -> String {
        self.paint(Style::new().effects(Effects::BOLD), text)
    }

    /// `commands` in backticks keep their backticks and get the same green as in `--help`.
    fn spans(&self, text: &str) -> String {
        let literal = AnsiColor::Green.on_default().effects(Effects::BOLD);
        text.split('`')
            .enumerate()
            .map(|(i, part)| {
                if i % 2 == 1 {
                    format!("`{}`", self.paint(literal, part))
                } else {
                    part.to_string()
                }
            })
            .collect()
    }

    /// One line of a unified diff: headers bold, hunks cyan, additions green, removals red.
    pub fn diff(&self, diff: &str) -> String {
        diff.lines()
            .map(|line| {
                let style = if line.starts_with("---") || line.starts_with("+++") {
                    Style::new().effects(Effects::BOLD)
                } else if line.starts_with("@@") {
                    AnsiColor::Cyan.on_default()
                } else if line.starts_with('+') {
                    AnsiColor::Green.on_default()
                } else if line.starts_with('-') {
                    AnsiColor::Red.on_default()
                } else {
                    return line.to_string();
                };
                self.paint(style, line)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// One row of a section: `<glyph> <label>  <detail>`.
pub struct Item {
    tone: Tone,
    glyph: &'static str,
    label: String,
    detail: Option<String>,
}

impl Item {
    pub fn new(tone: Tone, glyph: &'static str, label: impl Into<String>) -> Self {
        Item {
            tone,
            glyph,
            label: label.into(),
            detail: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

fn width<'a>(texts: impl Iterator<Item = &'a str>) -> usize {
    texts.map(|t| t.chars().count()).max().unwrap_or(0)
}

fn pad(text: &str, width: usize) -> String {
    format!(
        "{text}{}",
        " ".repeat(width.saturating_sub(text.chars().count()))
    )
}

/// Builds one message. Every block starts after a blank line; [`Screen::finish`] returns the text without a trailing newline.
pub struct Screen<'a> {
    theme: &'a Theme,
    out: String,
}

impl<'a> Screen<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Screen {
            theme,
            out: String::new(),
        }
    }

    fn gap(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    /// The first line: an icon and what happened.
    pub fn title(&mut self, tone: Tone, text: &str) -> &mut Self {
        self.gap();
        let line = format!(
            "{} {}\n",
            self.theme.tone(tone, tone.glyph()),
            self.theme.tone(tone, text)
        );
        self.out.push_str(&line);
        self
    }

    /// A title without an icon, for views that show data (a list, a status).
    pub fn heading(&mut self, text: &str) -> &mut Self {
        self.gap();
        let line = format!("{}\n", self.theme.bold(text));
        self.out.push_str(&line);
        self
    }

    /// Free text, one line per line of `text`. Lines keep their own extra indentation.
    pub fn text(&mut self, text: &str) -> &mut Self {
        self.gap();
        for line in text.lines() {
            let line = format!("  {}\n", self.theme.spans(line));
            self.out.push_str(&line);
        }
        self
    }

    /// `label  value` rows with the labels aligned.
    pub fn facts(&mut self, rows: &[(&str, String)]) -> &mut Self {
        if rows.is_empty() {
            return self;
        }
        self.gap();
        let w = width(rows.iter().map(|(label, _)| *label));
        for (label, value) in rows {
            let line = format!(
                "  {}  {}\n",
                self.theme.dim(&pad(label, w)),
                self.theme.spans(value)
            );
            self.out.push_str(&line);
        }
        self
    }

    /// A headed list; nothing is written for an empty list.
    pub fn section(&mut self, heading: &str, items: Vec<Item>) -> &mut Self {
        if items.is_empty() {
            return self;
        }
        self.gap();
        let line = format!(
            "  {}\n",
            self.theme.bold(&format!("{heading} ({})", items.len()))
        );
        self.out.push_str(&line);
        let w = width(items.iter().map(|i| i.label.as_str()));
        for item in &items {
            let glyph = self.theme.tone(item.tone, item.glyph);
            let line = match &item.detail {
                Some(detail) => format!(
                    "    {glyph} {}  {}\n",
                    pad(&item.label, w),
                    self.theme.dim(detail)
                ),
                None => format!("    {glyph} {}\n", item.label),
            };
            self.out.push_str(&line);
        }
        self
    }

    /// Columns under a dim header row; the last column is not padded.
    pub fn table(&mut self, headers: &[&str], rows: &[Vec<String>]) -> &mut Self {
        self.gap();
        let widths: Vec<usize> = (0..headers.len())
            .map(|c| width(std::iter::once(headers[c]).chain(rows.iter().map(|r| r[c].as_str()))))
            .collect();
        let line = |cells: Vec<&str>, dim: bool| {
            let last = cells.len() - 1;
            let joined = cells
                .iter()
                .enumerate()
                .map(|(i, cell)| {
                    if i == last {
                        cell.to_string()
                    } else {
                        pad(cell, widths[i])
                    }
                })
                .collect::<Vec<_>>()
                .join("  ");
            format!("  {}\n", if dim { self.theme.dim(&joined) } else { joined })
        };
        let header = line(headers.to_vec(), true);
        self.out.push_str(&header);
        for row in rows {
            let row = line(row.iter().map(String::as_str).collect(), false);
            self.out.push_str(&row);
        }
        self
    }

    /// A one-line remark in the tone's color.
    pub fn note(&mut self, tone: Tone, text: &str) -> &mut Self {
        self.gap();
        let line = format!(
            "  {} {}\n",
            self.theme.tone(tone, tone.glyph()),
            self.theme.spans(text)
        );
        self.out.push_str(&line);
        self
    }

    /// What to do next.
    pub fn hint(&mut self, text: &str) -> &mut Self {
        self.gap();
        let line = format!(
            "  {} {}\n",
            self.theme.tone(Tone::Warn, "→"),
            self.theme.spans(text)
        );
        self.out.push_str(&line);
        self
    }

    /// A dim last line for technical details.
    pub fn meta(&mut self, text: &str) -> &mut Self {
        self.gap();
        let line = format!("  {}\n", self.theme.dim(text));
        self.out.push_str(&line);
        self
    }

    pub fn finish(&self) -> String {
        self.out.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_are_separated_by_one_blank_line_and_indented() {
        let theme = Theme::new(false);
        let mut s = Screen::new(&theme);
        s.title(Tone::Ok, "Done")
            .facts(&[("a", "1".into()), ("long", "2".into())])
            .section(
                "Files",
                vec![Item::new(Tone::Ok, "+", "x.md").detail("new")],
            )
            .note(Tone::Warn, "careful")
            .hint("Run `speq pull`")
            .meta("code 1");
        assert_eq!(
            s.finish(),
            "✔ Done\n\n  a     1\n  long  2\n\n  Files (1)\n    + x.md  new\n\n  ! careful\n\n  → Run `speq pull`\n\n  code 1"
        );
    }

    #[test]
    fn empty_sections_and_facts_write_nothing() {
        let theme = Theme::new(false);
        let mut s = Screen::new(&theme);
        s.title(Tone::Info, "Nothing")
            .facts(&[])
            .section("Files", vec![]);
        assert_eq!(s.finish(), "ℹ Nothing");
    }

    #[test]
    fn section_details_line_up_and_a_row_without_one_stays_bare() {
        let theme = Theme::new(false);
        let mut s = Screen::new(&theme);
        s.section(
            "Files",
            vec![
                Item::new(Tone::Info, "~", "a.md").detail("changed"),
                Item::new(Tone::Info, "~", "long-name.md").detail("changed"),
                Item::new(Tone::Info, "~", "bare.md"),
            ],
        );
        assert_eq!(
            s.finish(),
            "  Files (3)\n    ~ a.md          changed\n    ~ long-name.md  changed\n    ~ bare.md"
        );
    }

    #[test]
    fn table_pads_every_column_but_the_last() {
        let theme = Theme::new(false);
        let mut s = Screen::new(&theme);
        s.table(
            &["NAME", "ROLE"],
            &[vec!["a".into(), "x".into()], vec!["bbb".into(), "y".into()]],
        );
        assert_eq!(s.finish(), "  NAME  ROLE\n  a     x\n  bbb   y");
    }

    #[test]
    fn plain_theme_has_no_escape_codes_and_colored_theme_has() {
        let build = |color| {
            let theme = Theme::new(color);
            let mut s = Screen::new(&theme);
            s.title(Tone::Fail, "Bad").hint("Run `speq login`");
            s.finish()
        };
        assert!(!build(false).contains('\x1b'));
        let colored = build(true);
        assert!(colored.contains("\x1b[1m\x1b[31m✖"), "{colored}");
        assert!(
            colored.contains("`\x1b[1m\x1b[32mspeq login\x1b[0m`"),
            "{colored}"
        );
    }

    #[test]
    fn diff_lines_are_colored_by_kind_only_when_color_is_on() {
        let diff = "--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n same";
        assert_eq!(Theme::new(false).diff(diff), diff);
        let colored = Theme::new(true).diff(diff);
        assert!(colored.contains("\x1b[31m-old"), "{colored}");
        assert!(colored.contains("\x1b[32m+new"), "{colored}");
        assert!(colored.contains("\x1b[36m@@"), "{colored}");
        assert!(colored.ends_with("\n same"), "{colored}");
    }
}
