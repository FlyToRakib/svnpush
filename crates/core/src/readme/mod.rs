//! `readme.txt`: parse, validate and write the WordPress.org plugin readme.

mod parse;
mod write;

use serde::Serialize;
use ts_rs::TS;

use crate::error::Coded;

pub use parse::parse;
pub use write::{set_header, upsert_changelog_entry, upsert_upgrade_notice};

/// The file name WordPress.org reads.
pub const README_FILE: &str = "readme.txt";

/// The headers WordPress.org requires (check V05). `name` is the `=== Name ===` line.
pub const REQUIRED_HEADERS: [&str; 8] = [
    "Contributors",
    "Tags",
    "Requires at least",
    "Tested up to",
    "Requires PHP",
    "Stable tag",
    "License",
    "License URI",
];

/// One `Name: value` header line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ReadmeHeader {
    /// The header name as written.
    pub name: String,
    /// The trimmed value.
    pub value: String,
    /// 1-based line number.
    pub line: u32,
}

/// One `== Title ==` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Section {
    /// The section title as written.
    pub title: String,
    /// The section text, without the title line.
    pub body: String,
    /// 1-based line number of the title.
    pub line: u32,
}

/// One `= 1.2.3 =` entry under Changelog or Upgrade Notice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ChangelogEntry {
    /// The entry title as written, for example `1.2.3 - 2026-09-01`.
    pub title: String,
    /// The version the title names, when it names one.
    pub version: Option<String>,
    /// The entry text, trimmed.
    pub body: String,
    /// 1-based line number of the title.
    pub line: u32,
}

/// A parsed `readme.txt`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Readme {
    /// The plugin name from the `=== Name ===` line.
    pub name: Option<String>,
    /// The header lines, in file order.
    pub headers: Vec<ReadmeHeader>,
    /// The short description between the headers and the first section.
    pub short_description: String,
    /// Every `== Title ==` section, in file order.
    pub sections: Vec<Section>,
    /// Entries under `== Changelog ==`, newest first as written.
    pub changelog: Vec<ChangelogEntry>,
    /// Entries under `== Upgrade Notice ==`.
    pub upgrade_notice: Vec<ChangelogEntry>,
}

impl Readme {
    /// The value of header `name`, matched case-insensitively.
    pub fn header(&self, name: &str) -> Option<&ReadmeHeader> {
        self.headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
    }

    /// Whether a section titled `title` exists (case-insensitive).
    pub fn has_section(&self, title: &str) -> bool {
        self.sections
            .iter()
            .any(|s| s.title.eq_ignore_ascii_case(title))
    }
}

/// A readme edit that could not be made.
#[derive(Debug, thiserror::Error)]
pub enum ReadmeError {
    /// The header to change does not exist.
    #[error("readme.txt has no \"{name}:\" header")]
    HeaderMissing { name: String },
    /// A file involved in the edit could not be loaded or saved.
    #[error(transparent)]
    Edit(#[from] crate::edit::EditError),
}

impl Coded for ReadmeError {
    fn code(&self) -> &'static str {
        match self {
            Self::HeaderMissing { .. } => "README_HEADER_MISSING",
            Self::Edit(inner) => inner.code(),
        }
    }

    fn fix(&self) -> Option<String> {
        match self {
            Self::HeaderMissing { name } => Some(format!(
                "Add a \"{name}:\" line to the header block at the top of readme.txt."
            )),
            Self::Edit(inner) => inner.fix(),
        }
    }
}
