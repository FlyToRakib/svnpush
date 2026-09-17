//! Byte-preserving edits to `readme.txt`.
//!
//! Every function returns new content in which only the edited region
//! differs; line endings of inserted text follow the file's dominant style.

use crate::text;

use super::ReadmeError;
use super::parse::Layout;

/// Replaces the value of header `name`.
pub fn set_header(content: &str, name: &str, value: &str) -> Result<String, ReadmeError> {
    let layout = Layout::scan(content);
    let span = layout
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.clone())
        .ok_or_else(|| ReadmeError::HeaderMissing { name: name.to_owned() })?;
    let mut out = String::with_capacity(content.len() + value.len());
    out.push_str(&content[..span.start]);
    if span.is_empty() && !content[..span.start].ends_with(' ') {
        out.push(' ');
    }
    out.push_str(value);
    out.push_str(&content[span.end..]);
    Ok(out)
}

/// Adds or replaces the `= version =` entry at the top of `== Changelog ==`.
///
/// When an entry for `version` already exists its body is replaced and its
/// title kept. Otherwise the entry is inserted before the first entry, after
/// any introductory text. A missing section is appended at the end.
pub fn upsert_changelog_entry(content: &str, version: &str, body: &str) -> String {
    upsert_entry(content, "Changelog", version, body, None)
}

/// Adds or replaces the `= version =` entry under `== Upgrade Notice ==`.
///
/// An empty notice changes nothing. A missing section is created directly
/// after `== Changelog ==` (or at the end when there is no changelog).
pub fn upsert_upgrade_notice(content: &str, version: &str, notice: &str) -> String {
    if notice.trim().is_empty() {
        return content.to_owned();
    }
    upsert_entry(content, "Upgrade Notice", version, notice, Some("Changelog"))
}

fn upsert_entry(
    content: &str,
    section_title: &str,
    version: &str,
    body: &str,
    create_after: Option<&str>,
) -> String {
    let eol = text::dominant_eol(content);
    let body = text::with_eol(body.trim(), eol);
    let block = format!("= {version} ={eol}{body}{eol}");
    let layout = Layout::scan(content);

    let Some(section) = layout.section(section_title) else {
        let heading = format!("== {section_title} =={eol}{eol}{block}");
        let after = create_after.and_then(|t| layout.section(t));
        return match after {
            Some(previous) if previous.end_line < layout.lines.len() => {
                let at = layout.lines[previous.end_line].start;
                splice(content, at..at, &format!("{heading}{eol}"))
            }
            _ => append_block(content, &heading, eol),
        };
    };

    let entries = layout.entries(section);

    if let Some(existing) = entries.iter().find(|e| e.version.as_deref() == Some(version)) {
        let title = &layout.lines[existing.title_line];
        let body_start = existing.title_line + 1;
        let body_end = layout.content_end(body_start..existing.end_line);
        if body_end > body_start {
            let range = layout.lines[body_start].start..layout.lines[body_end - 1].content_end;
            return splice(content, range, &body);
        }
        let insert = if title.end == title.content_end {
            format!("{eol}{body}")
        } else {
            format!("{body}{eol}")
        };
        return splice(content, title.end..title.end, &insert);
    }

    if let Some(first) = entries.first() {
        let at = layout.lines[first.title_line].start;
        return splice(content, at..at, &format!("{block}{eol}"));
    }

    let last = layout.content_end(section.title_line..section.end_line);
    let line = &layout.lines[last - 1];
    let mut insert = String::new();
    if line.end == line.content_end {
        insert.push_str(eol);
    }
    insert.push_str(eol);
    insert.push_str(&block);
    if section.end_line < layout.lines.len() && last == section.end_line {
        insert.push_str(eol);
    }
    splice(content, line.end..line.end, &insert)
}

fn append_block(content: &str, block: &str, eol: &str) -> String {
    let mut out = content.to_owned();
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push_str(eol);
        }
        out.push_str(eol);
    }
    out.push_str(block);
    out
}

fn splice(content: &str, range: std::ops::Range<usize>, insert: &str) -> String {
    let mut out = String::with_capacity(content.len() + insert.len());
    out.push_str(&content[..range.start]);
    out.push_str(insert);
    out.push_str(&content[range.end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readme::parse;

    const SAMPLE: &str = "=== My Plugin ===
Stable tag: 1.1.0
License: GPLv2

Short.

== Changelog ==

Intro text.

= 1.1.0 =
* Old.

== Upgrade Notice ==

= 1.1.0 =
Old notice.
";

    #[test]
    fn sets_a_header_value() {
        let out = set_header(SAMPLE, "stable tag", "1.2.0").unwrap();
        assert_eq!(out, SAMPLE.replace("Stable tag: 1.1.0", "Stable tag: 1.2.0"));
    }

    #[test]
    fn missing_header_is_an_error() {
        let err = set_header(SAMPLE, "Tested up to", "6.6").unwrap_err();
        assert!(matches!(err, ReadmeError::HeaderMissing { .. }));
    }

    #[test]
    fn inserts_new_entry_before_the_first_and_after_the_intro() {
        let out = upsert_changelog_entry(SAMPLE, "1.2.0", "* New.\n");
        let expected =
            SAMPLE.replace("= 1.1.0 =\n* Old.", "= 1.2.0 =\n* New.\n\n= 1.1.0 =\n* Old.");
        assert_eq!(out, expected);
        let readme = parse(&out);
        assert_eq!(readme.changelog[0].version.as_deref(), Some("1.2.0"));
        assert_eq!(readme.changelog[0].body, "* New.");
    }

    #[test]
    fn replaces_the_body_of_an_existing_entry() {
        let out = upsert_changelog_entry(SAMPLE, "1.1.0", "* Rewritten.");
        assert_eq!(out, SAMPLE.replace("* Old.", "* Rewritten."));
    }

    #[test]
    fn uses_crlf_when_the_file_does() {
        let crlf = SAMPLE.replace('\n', "\r\n");
        let out = upsert_changelog_entry(&crlf, "1.2.0", "* A.\n* B.");
        assert!(out.contains("= 1.2.0 =\r\n* A.\r\n* B.\r\n\r\n= 1.1.0 ="));
        assert!(!out.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn creates_a_missing_changelog_at_the_end() {
        let text = "=== P ===\nStable tag: 1.0\n\nShort.\n";
        let out = upsert_changelog_entry(text, "1.0", "* First.");
        assert_eq!(out, format!("{text}\n== Changelog ==\n\n= 1.0 =\n* First.\n"));
    }

    #[test]
    fn fills_an_empty_changelog_section() {
        let text = "=== P ===\n\n== Changelog ==\n\n== FAQ ==\nQ\n";
        let out = upsert_changelog_entry(text, "1.0", "* First.");
        assert_eq!(out, "=== P ===\n\n== Changelog ==\n\n= 1.0 =\n* First.\n\n== FAQ ==\nQ\n");
        assert_eq!(parse(&out).changelog[0].body, "* First.");
    }

    #[test]
    fn upgrade_notice_inserts_and_skips_empty() {
        assert_eq!(upsert_upgrade_notice(SAMPLE, "1.2.0", "  "), SAMPLE);
        let out = upsert_upgrade_notice(SAMPLE, "1.2.0", "Update now.");
        assert_eq!(parse(&out).upgrade_notice[0].body, "Update now.");
    }

    #[test]
    fn upgrade_notice_section_is_created_after_changelog() {
        let text = "=== P ===\n\n== Changelog ==\n\n= 1.0 =\n* A.\n\n== FAQ ==\nQ\n";
        let out = upsert_upgrade_notice(text, "1.0", "Please update.");
        assert_eq!(
            out,
            "=== P ===\n\n== Changelog ==\n\n= 1.0 =\n* A.\n\n== Upgrade Notice ==\n\n= 1.0 =\nPlease update.\n\n== FAQ ==\nQ\n"
        );
    }
}
