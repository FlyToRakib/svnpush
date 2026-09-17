//! Reading `readme.txt` into a [`Readme`] and into a byte-offset layout for editing.

use std::ops::Range;

use crate::text::{self, BOM, Line};
use crate::version;

use super::{ChangelogEntry, Readme, ReadmeHeader, Section};

/// A `=`-delimited heading: level 3 is the name line, 2 a section, 1 an entry.
pub(super) fn heading(content: &str) -> Option<(usize, &str)> {
    let trimmed = content.trim_start_matches(BOM).trim();
    let leading = trimmed.len() - trimmed.trim_start_matches('=').len();
    let trailing = trimmed.len() - trimmed.trim_end_matches('=').len();
    if leading == 0 || trailing == 0 || leading + trailing >= trimmed.len() {
        return None;
    }
    let title = trimmed.trim_matches('=').trim();
    (!title.is_empty()).then_some((leading, title))
}

/// A `Name: value` header line, where the name is letters and spaces.
fn header_line(line: &Line<'_>) -> Option<(String, Range<usize>)> {
    let content = line.content.trim_start_matches(BOM);
    let bom_len = line.content.len() - content.len();
    let colon = content.find(':')?;
    let name = content[..colon].trim();
    let valid_name = !name.is_empty()
        && name.len() <= 40
        && name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphabetic() || c == ' ');
    if !valid_name {
        return None;
    }
    let after = &content[colon + 1..];
    let value = after.trim();
    let value_start = if value.is_empty() {
        line.start + bom_len + colon + 1 + after.len()
    } else {
        line.start + bom_len + colon + 1 + (after.len() - after.trim_start().len())
    };
    Some((name.to_owned(), value_start..value_start + value.len()))
}

/// A header line's position.
pub(super) struct HeaderSpan {
    pub name: String,
    pub value: Range<usize>,
    pub line: usize,
}

/// A section's position: `title_line` holds the title, the body runs to `end_line` (exclusive).
pub(super) struct SectionSpan<'a> {
    pub title: &'a str,
    pub title_line: usize,
    pub end_line: usize,
}

/// An entry's position inside a section.
pub(super) struct EntrySpan<'a> {
    pub title: &'a str,
    pub version: Option<String>,
    pub title_line: usize,
    pub end_line: usize,
}

/// Byte-level structure of a readme.
pub(super) struct Layout<'a> {
    pub lines: Vec<Line<'a>>,
    pub name: Option<&'a str>,
    pub headers: Vec<HeaderSpan>,
    pub description_lines: Range<usize>,
    pub sections: Vec<SectionSpan<'a>>,
}

impl<'a> Layout<'a> {
    pub fn scan(text: &'a str) -> Self {
        let lines = text::lines(text);
        let count = lines.len();
        let is_blank = |i: usize| lines[i].content.trim_start_matches(BOM).trim().is_empty();
        let mut i = 0;

        while i < count && is_blank(i) {
            i += 1;
        }
        let mut name = None;
        if i < count
            && let Some((level, title)) = heading(lines[i].content)
            && level >= 3
        {
            name = Some(title);
            i += 1;
        }
        while i < count && is_blank(i) {
            i += 1;
        }

        let mut headers = Vec::new();
        while i < count && heading(lines[i].content).is_none() {
            let Some((header_name, value)) = header_line(&lines[i]) else {
                break;
            };
            headers.push(HeaderSpan { name: header_name, value, line: i });
            i += 1;
        }

        let description_start = i;
        while i < count && !matches!(heading(lines[i].content), Some((2, _))) {
            i += 1;
        }
        let description_lines = description_start..i;

        let mut sections: Vec<SectionSpan<'a>> = Vec::new();
        while i < count {
            if let Some((2, title)) = heading(lines[i].content) {
                if let Some(previous) = sections.last_mut() {
                    previous.end_line = i;
                }
                sections.push(SectionSpan { title, title_line: i, end_line: count });
            }
            i += 1;
        }

        Self { lines, name, headers, description_lines, sections }
    }

    pub fn section(&self, title: &str) -> Option<&SectionSpan<'a>> {
        self.sections.iter().find(|s| s.title.eq_ignore_ascii_case(title))
    }

    pub fn entries(&self, section: &SectionSpan<'a>) -> Vec<EntrySpan<'a>> {
        let mut entries: Vec<EntrySpan<'a>> = Vec::new();
        for index in section.title_line + 1..section.end_line {
            if let Some((1, title)) = heading(self.lines[index].content) {
                if let Some(previous) = entries.last_mut() {
                    previous.end_line = index;
                }
                entries.push(EntrySpan {
                    title,
                    version: version::extract_leading(title),
                    title_line: index,
                    end_line: section.end_line,
                });
            }
        }
        entries
    }

    /// The text of lines `range`, joined with `\n` and trimmed of blank edges.
    pub fn text_of(&self, range: Range<usize>) -> String {
        let joined: Vec<&str> = self.lines[range].iter().map(|l| l.content).collect();
        joined.join("\n").trim().to_owned()
    }

    /// Index just past the last non-blank line in `range`, or `range.start`.
    pub fn content_end(&self, range: Range<usize>) -> usize {
        let start = range.start;
        range.rev().find(|&i| !self.lines[i].content.trim().is_empty()).map_or(start, |i| i + 1)
    }
}

/// Parses `readme.txt` content. Never fails: missing parts are empty.
pub fn parse(text: &str) -> Readme {
    let layout = Layout::scan(text);
    let headers = layout
        .headers
        .iter()
        .map(|h| ReadmeHeader {
            name: h.name.clone(),
            value: text[h.value.clone()].to_owned(),
            line: text::line_number(h.line),
        })
        .collect();

    let description: Vec<&str> = layout.lines[layout.description_lines.clone()]
        .iter()
        .map(|l| l.content.trim())
        .filter(|l| !l.is_empty())
        .collect();

    let sections = layout
        .sections
        .iter()
        .map(|s| Section {
            title: s.title.to_owned(),
            body: layout.text_of(s.title_line + 1..s.end_line),
            line: text::line_number(s.title_line),
        })
        .collect();

    let entries_of = |title: &str| -> Vec<ChangelogEntry> {
        layout.section(title).map_or_else(Vec::new, |section| {
            layout
                .entries(section)
                .into_iter()
                .map(|e| ChangelogEntry {
                    title: e.title.to_owned(),
                    version: e.version,
                    body: layout.text_of(e.title_line + 1..e.end_line),
                    line: text::line_number(e.title_line),
                })
                .collect()
        })
    };

    Readme {
        name: layout.name.map(str::to_owned),
        headers,
        short_description: description.join(" "),
        sections,
        changelog: entries_of("Changelog"),
        upgrade_notice: entries_of("Upgrade Notice"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "=== My Plugin ===
Contributors: alice, bob
Tags: release, svn
Requires at least: 6.0
Tested up to: 6.6
Requires PHP: 7.4
Stable tag: 1.2.0
License: GPLv2 or later
License URI: https://www.gnu.org/licenses/gpl-2.0.html

Ships plugins.

== Description ==

Long text.

== Changelog ==

Older entries live elsewhere.

= 1.2.0 - 2026-09-01 =
* Added a thing.

= 1.1.0 =
* Fixed a thing.

== Upgrade Notice ==

= 1.2.0 =
Recommended.
";

    #[test]
    fn parses_name_headers_and_description() {
        let readme = parse(SAMPLE);
        assert_eq!(readme.name.as_deref(), Some("My Plugin"));
        assert_eq!(readme.headers.len(), 8);
        assert_eq!(readme.header("stable TAG").unwrap().value, "1.2.0");
        assert_eq!(readme.header("Tags").unwrap().line, 3);
        assert_eq!(readme.short_description, "Ships plugins.");
    }

    #[test]
    fn parses_sections_and_entries() {
        let readme = parse(SAMPLE);
        let titles: Vec<&str> = readme.sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Description", "Changelog", "Upgrade Notice"]);
        assert_eq!(readme.changelog.len(), 2);
        assert_eq!(readme.changelog[0].version.as_deref(), Some("1.2.0"));
        assert_eq!(readme.changelog[0].title, "1.2.0 - 2026-09-01");
        assert_eq!(readme.changelog[0].body, "* Added a thing.");
        assert_eq!(readme.upgrade_notice[0].body, "Recommended.");
    }

    #[test]
    fn handles_crlf_and_bom() {
        let text = format!("\u{feff}{}", SAMPLE.replace('\n', "\r\n"));
        let readme = parse(&text);
        assert_eq!(readme.name.as_deref(), Some("My Plugin"));
        assert_eq!(readme.header("Stable tag").unwrap().value, "1.2.0");
        assert_eq!(readme.changelog[1].body, "* Fixed a thing.");
    }

    #[test]
    fn empty_input_is_an_empty_readme() {
        assert_eq!(parse(""), Readme::default());
    }

    #[test]
    fn heading_levels() {
        assert_eq!(heading("=== A ==="), Some((3, "A")));
        assert_eq!(heading("== B =="), Some((2, "B")));
        assert_eq!(heading(" = 1.0 = "), Some((1, "1.0")));
        assert_eq!(heading("===="), None);
        assert_eq!(heading("a = b"), None);
    }
}
