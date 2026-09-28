//! Reading `readme.txt` into a [`Readme`] and into a byte-offset layout for editing.

use std::ops::Range;

use crate::text::{self, BOM, Line};
use crate::version;

use super::{ChangelogEntry, Readme, ReadmeHeader, Section};

/// The headers WordPress.org reads, lower-cased, with their aliases.
const KNOWN_HEADERS: [&str; 11] = [
    "tested",
    "tested up to",
    "requires",
    "requires at least",
    "requires php",
    "tags",
    "contributors",
    "donate link",
    "stable tag",
    "license",
    "license uri",
];

fn trimmed(content: &str) -> &str {
    content.trim_start_matches(BOM).trim()
}

/// A section heading as `class-parser.php` sees it: a line starting with
/// `==`, or with `##` but not `###`. Closing marks are optional.
pub(super) fn section_title(content: &str) -> Option<&str> {
    let t = trimmed(content);
    let markdown = t.starts_with("##") && t.len() > 2 && !t[2..].starts_with('#');
    (t.starts_with("==") || markdown).then(|| t.trim_matches(['#', '=', ' ', '\t']))
}

/// An entry heading inside a section: any other line starting with `=` or `#`.
pub(super) fn entry_title(content: &str) -> Option<&str> {
    let t = trimmed(content);
    let mark = t.chars().next().filter(|c| matches!(c, '=' | '#'))?;
    let title = t.trim_matches([mark, ' ', '\t']);
    (section_title(content).is_none() && !title.is_empty()).then_some(title)
}

/// The key a header name is matched by: lower-cased, with WordPress.org's
/// short forms `Tested` and `Requires` mapped to the full names.
pub(super) fn header_key(name: &str) -> String {
    let key = name.trim_matches([' ', '\t', '*', '-']).to_ascii_lowercase();
    match key.as_str() {
        "tested" => "tested up to".to_owned(),
        "requires" => "requires at least".to_owned(),
        _ => key,
    }
}

/// The key a section title is matched by, with WordPress.org's aliases.
pub(super) fn section_key(title: &str) -> String {
    let key = title.trim().to_ascii_lowercase().replace(' ', "_");
    match key.as_str() {
        "frequently_asked_questions" => "faq".to_owned(),
        "change_log" => "changelog".to_owned(),
        "screenshot" => "screenshots".to_owned(),
        _ => key,
    }
}

/// A `Name: value` line, read like `parse_possible_header`: any line with a
/// colon that does not start with `#` or `=`. The name is trimmed of ` \t*-`
/// and the value also of `<>`, so `**Stable tag:** 1.0` reads `1.0`.
fn header_line(line: &Line<'_>) -> Option<(String, Range<usize>)> {
    let content = line.content.trim_start_matches(BOM);
    let bom_len = line.content.len() - content.len();
    if content.starts_with(['#', '=']) {
        return None;
    }
    let colon = content.find(':')?;
    let name = content[..colon].trim_matches([' ', '\t', '*', '-']);
    let after = &content[colon + 1..];
    let marks = |c: char| c.is_whitespace() || matches!(c, '*' | '-' | '<' | '>');
    let value = after.trim_matches(marks);
    let value_start = if value.is_empty() {
        line.start + bom_len + colon + 1 + after.len()
    } else {
        line.start + bom_len + colon + 1 + (after.len() - after.trim_start_matches(marks).len())
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
    /// Whether the name line was the `=== Plugin Name ===` placeholder.
    pub name_placeholder: bool,
    pub headers: Vec<HeaderSpan>,
    pub description_lines: Range<usize>,
    pub sections: Vec<SectionSpan<'a>>,
}

impl<'a> Layout<'a> {
    pub fn scan(text: &'a str) -> Self {
        let lines = text::lines(text);
        let count = lines.len();
        let is_blank = |i: usize| trimmed(lines[i].content).is_empty();
        let known = |line: &Line<'_>| {
            header_line(line)
                .is_some_and(|(n, _)| KNOWN_HEADERS.contains(&n.to_ascii_lowercase().as_str()))
        };
        let mut i = 0;

        // The first non-blank line is the name, unless it is already a header.
        while i < count && is_blank(i) {
            i += 1;
        }
        let name_of =
            |i: usize| trimmed(lines[i].content).trim_matches(['#', '=', ' ', '\t', '\0', '\x0B']);
        let mut name = None;
        let mut name_placeholder = false;
        if i < count && !known(&lines[i]) {
            name = Some(name_of(i));
            i += 1;
            // A Markdown `====` underline below the name.
            if i < count && trimmed(lines[i].content).trim_matches(['=', '-']).is_empty() {
                i += 1;
            }
            // `=== Plugin Name ===` with the real name on the next line. As in
            // class-parser.php, a line of 50 bytes or more, or a known header,
            // is not a name, and the readme then has none. Either way the
            // official validator reports the placeholder as an error.
            if name.is_some_and(|n| n.eq_ignore_ascii_case("plugin name")) {
                name_placeholder = true;
                while i < count && is_blank(i) {
                    i += 1;
                }
                name = None;
                if i < count && lines[i].content.len() < 50 && !known(&lines[i]) {
                    name = Some(name_of(i));
                    i += 1;
                }
            }
        }

        // Blank lines before the block are skipped. Inside it, empty lines are
        // skipped; an unknown header after one, or any other line (a line of
        // spaces too, as class-parser.php tests `empty( $line )`), starts the
        // short description.
        while i < count && is_blank(i) {
            i += 1;
        }
        let mut headers = Vec::new();
        let mut after_blank = false;
        while i < count {
            if lines[i].content.trim_start_matches(BOM).is_empty() {
                after_blank = true;
                i += 1;
                continue;
            }
            let Some((header_name, value)) = header_line(&lines[i]) else {
                break;
            };
            if after_blank && !known(&lines[i]) {
                break;
            }
            if !header_name.is_empty() {
                headers.push(HeaderSpan { name: header_name, value, line: i });
            }
            after_blank = false;
            i += 1;
        }

        let description_start = i;
        let any_heading =
            |i: usize| ["==", "##"].iter().any(|m| trimmed(lines[i].content).starts_with(m));
        while i < count && !any_heading(i) {
            i += 1;
        }
        let description_lines = description_start..i;

        let mut sections: Vec<SectionSpan<'a>> = Vec::new();
        while i < count {
            if let Some(title) = section_title(lines[i].content) {
                if let Some(previous) = sections.last_mut() {
                    previous.end_line = i;
                }
                sections.push(SectionSpan { title, title_line: i, end_line: count });
            }
            i += 1;
        }

        Self { lines, name, name_placeholder, headers, description_lines, sections }
    }

    pub fn section(&self, title: &str) -> Option<&SectionSpan<'a>> {
        self.sections.iter().find(|s| section_key(s.title) == section_key(title))
    }

    /// The entries of `section`. In the Changelog, a heading that names no
    /// version after an entry (`#### Added`, `= Fixed =`) is part of that
    /// entry; WordPress.org shows the section as one text. In the Upgrade
    /// Notice every heading starts an entry, as WordPress.org splits it.
    pub fn entries(&self, section: &SectionSpan<'a>) -> Vec<EntrySpan<'a>> {
        let changelog = section_key(section.title) == "changelog";
        let mut entries: Vec<EntrySpan<'a>> = Vec::new();
        for index in section.title_line + 1..section.end_line {
            if let Some(title) = entry_title(self.lines[index].content) {
                let version = version::extract_leading(title);
                if changelog && version.is_none() && !entries.is_empty() {
                    continue;
                }
                if let Some(previous) = entries.last_mut() {
                    previous.end_line = index;
                }
                entries.push(EntrySpan {
                    title,
                    version,
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
        name_placeholder: layout.name_placeholder,
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
    fn headings_follow_class_parser() {
        assert_eq!(section_title("=== A ==="), Some("A"));
        assert_eq!(section_title("== B"), Some("B"));
        assert_eq!(section_title("## C ##"), Some("C"));
        assert_eq!(section_title("### 1.0"), None);
        assert_eq!(section_title("= 1.0 ="), None);
        assert_eq!(entry_title(" = 1.0 = "), Some("1.0"));
        assert_eq!(entry_title("### 1.0"), Some("1.0"));
        assert_eq!(entry_title("# 1.0"), Some("1.0"));
        assert_eq!(entry_title("== B =="), None);
        assert_eq!(entry_title("a = b"), None);
    }

    #[test]
    fn blank_lines_inside_the_header_block_are_skipped() {
        let text = "=== P ===\nContributors: a\n\nStable tag: 1.0\n\nDonate: no\nShort.\n";
        let readme = parse(text);
        assert_eq!(readme.header("Stable tag").unwrap().value, "1.0");
        assert!(readme.header("Donate").is_none());
        assert_eq!(readme.short_description, "Donate: no Short.");
    }

    #[test]
    fn the_header_block_ends_where_class_parser_ends_it() {
        // Blank lines before the first header do not count as a gap, so an
        // unknown first header is skipped rather than ending the block.
        let text = "=== P ===\n\nPlugin URI: https://example.org\nStable tag: 1.0\n\nShort.\n";
        let readme = parse(text);
        assert_eq!(readme.header("Stable tag").unwrap().value, "1.0");
        assert_eq!(readme.short_description, "Short.");
        // A line of spaces is not empty: it ends the block.
        let text = "=== P ===\nContributors: a\n  \nStable tag: 1.0\n\nShort.\n";
        let readme = parse(text);
        assert!(readme.header("Stable tag").is_none());
        assert_eq!(readme.short_description, "Stable tag: 1.0 Short.");
    }

    #[test]
    fn reads_markdown_style_readmes() {
        let text = "# My Plugin\n\n* Contributors: a\n- Tested: 6.6\n\nShort.\n\n## Changelog\n\n### 1.1\n* B.\n\n=== FAQ ===\n\n= Q =\nA.\n";
        let readme = parse(text);
        assert_eq!(readme.name.as_deref(), Some("My Plugin"));
        assert_eq!(readme.header("contributors").unwrap().value, "a");
        assert_eq!(readme.header("Tested up to").unwrap().value, "6.6");
        assert_eq!(readme.short_description, "Short.");
        let titles: Vec<&str> = readme.sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Changelog", "FAQ"]);
        assert_eq!(readme.changelog[0].version.as_deref(), Some("1.1"));
        assert_eq!(readme.changelog[0].body, "* B.");
    }

    #[test]
    fn the_name_line_needs_no_closing_marks_and_may_be_missing() {
        assert_eq!(parse("== My Plugin\nStable tag: 1.0\n").name.as_deref(), Some("My Plugin"));
        let headless = parse("Stable tag: 1.0\n\nShort.\n");
        assert_eq!(headless.name, None);
        assert_eq!(headless.header("Stable tag").unwrap().value, "1.0");
        let underlined = parse("My Plugin\n=========\nStable tag: 1.0\n");
        assert_eq!(underlined.name.as_deref(), Some("My Plugin"));
        assert_eq!(underlined.header("Stable tag").unwrap().line, 3);
    }

    #[test]
    fn a_placeholder_name_line_takes_the_name_from_the_next_line() {
        let readme = parse("=== Plugin Name ===\n\nHello Release\nStable tag: 1.0\n\nShort.\n");
        assert_eq!(readme.name.as_deref(), Some("Hello Release"));
        assert_eq!(readme.header("Stable tag").unwrap().value, "1.0");
        assert_eq!(readme.short_description, "Short.");
        // The next line is a header, or too long to be a name: no name.
        let header_next = parse("=== Plugin Name ===\nStable tag: 1.0\n\nShort.\n");
        assert_eq!(header_next.name, None);
        assert_eq!(header_next.header("Stable tag").unwrap().value, "1.0");
        let long_next = parse(&format!("=== Plugin Name ===\n{}\n", "x".repeat(50)));
        assert_eq!(long_next.name, None);
        assert!(long_next.name_placeholder);
        // The name's marks are trimmed as on the first line, and a header
        // written as a Markdown list item is still a header.
        let marked = parse("=== Plugin Name ===\n== Hello ==\nStable tag: 1.0\n");
        assert_eq!(marked.name.as_deref(), Some("Hello"));
        let listed = parse("=== Plugin Name ===\n* Stable tag: 1.0\n\nShort.\n");
        assert_eq!(listed.name, None);
        assert_eq!(listed.header("Stable tag").unwrap().value, "1.0");
    }

    #[test]
    fn the_last_of_repeated_headers_wins() {
        let readme = parse("=== P ===\nStable tag: 1.0\nStable Tag: 1.1\n");
        assert_eq!(readme.header("stable tag").unwrap().value, "1.1");
    }
}
