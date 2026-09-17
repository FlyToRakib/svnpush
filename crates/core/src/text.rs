//! Byte-preserving text helpers shared by the header and readme editors.

/// The UTF-8 byte order mark.
pub const BOM: char = '\u{feff}';

/// One line of a text with its byte offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line<'a> {
    /// Byte offset of the first character of the line.
    pub start: usize,
    /// Byte offset just past the line content, before the line ending.
    pub content_end: usize,
    /// Byte offset just past the line ending (equals `content_end` on the last line).
    pub end: usize,
    /// The line content without its line ending.
    pub content: &'a str,
}

/// Splits `text` into lines, keeping exact offsets so edits can splice bytes.
pub fn lines(text: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    while start < text.len() {
        let newline = bytes[start..].iter().position(|&b| b == b'\n');
        let (content_end, end) = match newline {
            Some(offset) => {
                let nl = start + offset;
                let content_end = if nl > start && bytes[nl - 1] == b'\r' { nl - 1 } else { nl };
                (content_end, nl + 1)
            }
            None => (text.len(), text.len()),
        };
        out.push(Line { start, content_end, end, content: &text[start..content_end] });
        start = end;
    }
    out
}

/// The line ending used most in `text`: `"\r\n"` when CRLF outnumbers LF.
pub fn dominant_eol(text: &str) -> &'static str {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    if crlf > lf { "\r\n" } else { "\n" }
}

/// Rewrites every line ending in `text` to `eol`.
pub fn with_eol(text: &str, eol: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', eol)
}

/// Line-ending facts about a text, used by warning W09.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EolReport {
    /// Number of CRLF line endings.
    pub crlf: usize,
    /// Number of bare LF line endings.
    pub lf: usize,
    /// Whether the text starts with a UTF-8 byte order mark.
    pub has_bom: bool,
}

impl EolReport {
    /// Whether both CRLF and LF line endings appear.
    pub fn is_mixed(&self) -> bool {
        self.crlf > 0 && self.lf > 0
    }
}

/// Counts line endings and detects a byte order mark.
pub fn eol_report(text: &str) -> EolReport {
    let crlf = text.matches("\r\n").count();
    EolReport { crlf, lf: text.matches('\n').count() - crlf, has_bom: text.starts_with(BOM) }
}

/// Converts a 0-based line index into the 1-based number shown to people.
pub fn line_number(index: usize) -> u32 {
    u32::try_from(index + 1).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_keep_offsets_for_mixed_endings() {
        let text = "a\r\nbb\nccc";
        let ls = lines(text);
        assert_eq!(ls.len(), 3);
        assert_eq!(ls[0].content, "a");
        assert_eq!(&text[ls[0].start..ls[0].end], "a\r\n");
        assert_eq!(ls[1].content, "bb");
        assert_eq!(ls[2].content, "ccc");
        assert_eq!(ls[2].end, text.len());
    }

    #[test]
    fn dominant_eol_prefers_the_majority() {
        assert_eq!(dominant_eol("a\r\nb\r\nc\n"), "\r\n");
        assert_eq!(dominant_eol("a\nb\n"), "\n");
        assert_eq!(dominant_eol("single"), "\n");
    }

    #[test]
    fn eol_report_detects_mixed_and_bom() {
        let report = eol_report("\u{feff}a\r\nb\n");
        assert!(report.is_mixed());
        assert!(report.has_bom);
        assert!(!eol_report("a\nb\n").is_mixed());
    }

    #[test]
    fn with_eol_normalises() {
        assert_eq!(with_eol("a\nb\r\nc", "\r\n"), "a\r\nb\r\nc");
    }
}
