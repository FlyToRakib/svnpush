//! The plugin header block, read the way WordPress's `get_file_data` reads it.
//!
//! WordPress scans the first 8 KiB of the file for lines of the form
//! `Name: value`, optionally prefixed by `<?php`, spaces, tabs, `/`, `*`, `#`
//! or `@`, matching the header name case-insensitively. A trailing `*/` or
//! `?>` and everything after it is dropped from the value.

use regex::Regex;
use serde::Serialize;
use ts_rs::TS;

use crate::text;

/// How much of the file WordPress reads when looking for headers.
const HEADER_SCAN_BYTES: usize = 8 * 1024;

/// The header fields SVNpush reads from the main plugin file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Header {
    /// `Plugin Name:`
    pub name: Option<String>,
    /// `Version:`
    pub version: Option<String>,
    /// `Text Domain:`
    pub text_domain: Option<String>,
    /// `Requires at least:`
    pub requires_at_least: Option<String>,
    /// `Requires PHP:`
    pub requires_php: Option<String>,
    /// `License:`
    pub license: Option<String>,
}

/// One header field located in the source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderMatch {
    /// The cleaned value.
    pub value: String,
    /// Byte range of the value inside the text.
    pub span: std::ops::Range<usize>,
    /// 0-based line index.
    pub line_index: usize,
}

/// The portion of `text` WordPress scans, cut on a character boundary.
fn scan_window(text: &str) -> &str {
    if text.len() <= HEADER_SCAN_BYTES {
        return text;
    }
    let mut end = HEADER_SCAN_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn header_regex(name: &str) -> Option<Regex> {
    Regex::new(&format!(
        r"(?i)^(?:[ \t]*<\?php)?[ \t/*#@]*{}:(.*)$",
        regex::escape(name)
    ))
    .ok()
}

fn value_end_regex() -> Option<Regex> {
    Regex::new(r"\s*(?:\*/|\?>)").ok()
}

/// Finds the first occurrence of header `name` in `text`.
pub fn find(text: &str, name: &str) -> Option<HeaderMatch> {
    let pattern = header_regex(name)?;
    let cut = value_end_regex()?;
    let window = scan_window(text);
    for (index, line) in text::lines(window).iter().enumerate() {
        let Some(caps) = pattern.captures(line.content) else {
            continue;
        };
        let raw = caps.get(1)?;
        let raw_text = raw.as_str();
        let kept = cut
            .find(raw_text)
            .map_or(raw_text, |m| &raw_text[..m.start()]);
        let trimmed = kept.trim();
        if trimmed.is_empty() {
            return Some(HeaderMatch {
                value: String::new(),
                span: line.start + raw.end()..line.start + raw.end(),
                line_index: index,
            });
        }
        let leading = kept.len() - kept.trim_start().len();
        let start = line.start + raw.start() + leading;
        return Some(HeaderMatch {
            value: trimmed.to_owned(),
            span: start..start + trimmed.len(),
            line_index: index,
        });
    }
    None
}

/// Whether `text` contains a `Plugin Name:` header, the mark of a main plugin file.
pub fn is_main_plugin_file(text: &str) -> bool {
    find(text, "Plugin Name").is_some_and(|m| !m.value.is_empty())
}

/// Parses the header fields SVNpush needs.
pub fn parse(text: &str) -> Header {
    let get = |name: &str| find(text, name).map(|m| m.value).filter(|v| !v.is_empty());
    Header {
        name: get("Plugin Name"),
        version: get("Version"),
        text_domain: get("Text Domain"),
        requires_at_least: get("Requires at least"),
        requires_php: get("Requires PHP"),
        license: get("License"),
    }
}

/// Replaces the value of header `name`, keeping every other byte. `None` when absent.
pub fn set(text: &str, name: &str, value: &str) -> Option<String> {
    let found = find(text, name)?;
    let mut out = String::with_capacity(text.len() + value.len());
    out.push_str(&text[..found.span.start]);
    if found.span.is_empty() {
        out.push(' ');
    }
    out.push_str(value);
    out.push_str(&text[found.span.end..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "<?php\n/**\n * Plugin Name:       My Plugin\n * Version:           1.2.3\n * Text Domain:       my-plugin\n * Requires at least: 6.0\n * Requires PHP:      7.4\n * License:           GPL-2.0-or-later\n */\n";

    #[test]
    fn parses_the_standard_block() {
        let header = parse(SAMPLE);
        assert_eq!(header.name.as_deref(), Some("My Plugin"));
        assert_eq!(header.version.as_deref(), Some("1.2.3"));
        assert_eq!(header.text_domain.as_deref(), Some("my-plugin"));
        assert_eq!(header.requires_at_least.as_deref(), Some("6.0"));
        assert_eq!(header.requires_php.as_deref(), Some("7.4"));
        assert_eq!(header.license.as_deref(), Some("GPL-2.0-or-later"));
    }

    #[test]
    fn matches_case_insensitively_and_strips_comment_close() {
        let text = "<?php /* plugin name: Tiny */\n// version: 0.1 ?>\n";
        let header = parse(text);
        assert_eq!(header.name.as_deref(), Some("Tiny"));
        assert_eq!(header.version.as_deref(), Some("0.1"));
    }

    #[test]
    fn ignores_headers_beyond_8_kib() {
        let text = format!("<?php\n{}\n * Plugin Name: Late\n", "x".repeat(9000));
        assert!(!is_main_plugin_file(&text));
    }

    #[test]
    fn set_replaces_only_the_value() {
        let updated = set(SAMPLE, "Version", "1.3.0").unwrap();
        assert_eq!(updated, SAMPLE.replace("1.2.3", "1.3.0"));
    }

    #[test]
    fn set_keeps_crlf_line_endings() {
        let text = SAMPLE.replace('\n', "\r\n");
        let updated = set(&text, "Version", "2.0").unwrap();
        assert_eq!(updated, text.replace("1.2.3", "2.0"));
    }

    #[test]
    fn set_on_missing_header_is_none() {
        assert!(set("<?php\n", "Version", "1.0").is_none());
    }

    #[test]
    fn set_fills_an_empty_value() {
        let updated = set("<?php\n * Version:\n", "Version", "1.0").unwrap();
        assert_eq!(parse(&updated).version.as_deref(), Some("1.0"));
    }
}
