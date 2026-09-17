//! Every place a plugin declares its version, read and written together.

use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::detect::header;
use crate::edit::{self, EditSet};
use crate::readme::{self, README_FILE};

use super::VersionError;

/// Where a version value was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum VersionSourceKind {
    /// The `Version:` line of the main plugin file header.
    Header,
    /// The readme `Stable tag:` header.
    StableTag,
    /// The newest `= x.y.z =` entry under `== Changelog ==`.
    Changelog,
    /// A project-configured location (file plus regex).
    Custom,
}

/// One version value and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct VersionSource {
    /// Human label, for example `Plugin header` or `plugin.php (match 2)`.
    pub label: String,
    /// Path relative to the package root, with `/` separators.
    pub path: String,
    /// The value found; empty when the source exists but holds no version.
    pub value: String,
    /// What kind of source this is.
    pub kind: VersionSourceKind,
    /// 1-based line number, when known.
    pub line: Option<u32>,
}

/// An extra version location configured for a project: a file and a regex
/// with exactly one capture group around the version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VersionLocation {
    /// Path relative to the package root, with `/` separators.
    pub path: String,
    /// Regular expression with one capture group, for example
    /// `define\(\s*'MY_VERSION',\s*'([^']+)'`.
    pub pattern: String,
}

/// Compiles a location's pattern, requiring exactly one capture group.
pub fn validate_location(location: &VersionLocation) -> Result<Regex, VersionError> {
    let bad = |reason: String| VersionError::BadPattern {
        path: location.path.clone(),
        reason,
    };
    let regex = Regex::new(&location.pattern).map_err(|e| bad(e.to_string()))?;
    let groups = regex.captures_len() - 1;
    if groups != 1 {
        return Err(bad(format!("it has {groups} capture groups, expected 1")));
    }
    Ok(regex)
}

fn line_of(text: &str, offset: usize) -> u32 {
    crate::text::line_number(text[..offset].matches('\n').count())
}

/// Reads every version source. Missing values are reported as empty so
/// check V01 can name them; a missing readme contributes no sources.
pub fn read_sources(
    root: &Path,
    main_file: &str,
    locations: &[VersionLocation],
) -> Result<Vec<VersionSource>, VersionError> {
    let mut sources = Vec::new();

    let main = edit::read_text(root, main_file)?;
    let found = header::find(&main, "Version");
    sources.push(VersionSource {
        label: "Plugin header".to_owned(),
        path: main_file.to_owned(),
        value: found.as_ref().map(|m| m.value.clone()).unwrap_or_default(),
        kind: VersionSourceKind::Header,
        line: found.map(|m| crate::text::line_number(m.line_index)),
    });

    if root.join(README_FILE).is_file() {
        let parsed = readme::parse(&edit::read_text(root, README_FILE)?);
        let stable = parsed.header("Stable tag");
        sources.push(VersionSource {
            label: "Readme stable tag".to_owned(),
            path: README_FILE.to_owned(),
            value: stable.map(|h| h.value.clone()).unwrap_or_default(),
            kind: VersionSourceKind::StableTag,
            line: stable.map(|h| h.line),
        });
        let newest = parsed.changelog.first();
        sources.push(VersionSource {
            label: "Readme changelog".to_owned(),
            path: README_FILE.to_owned(),
            value: newest.and_then(|e| e.version.clone()).unwrap_or_default(),
            kind: VersionSourceKind::Changelog,
            line: newest.map(|e| e.line),
        });
    }

    for location in locations {
        let regex = validate_location(location)?;
        let content = edit::read_text(root, &location.path)?;
        let matches: Vec<_> = regex
            .captures_iter(&content)
            .filter_map(|c| c.get(1))
            .collect();
        if matches.is_empty() {
            sources.push(VersionSource {
                label: location.path.clone(),
                path: location.path.clone(),
                value: String::new(),
                kind: VersionSourceKind::Custom,
                line: None,
            });
        }
        let many = matches.len() > 1;
        for (index, m) in matches.iter().enumerate() {
            sources.push(VersionSource {
                label: if many {
                    format!("{} (match {})", location.path, index + 1)
                } else {
                    location.path.clone()
                },
                path: location.path.clone(),
                value: m.as_str().to_owned(),
                kind: VersionSourceKind::Custom,
                line: Some(line_of(&content, m.start())),
            });
        }
    }

    Ok(sources)
}

/// Plans writing `version` into the plugin header and every custom location.
///
/// The readme stable tag and changelog are written by the readme module in
/// the same [`EditSet`].
pub fn write_version(
    edits: &mut EditSet,
    main_file: &str,
    locations: &[VersionLocation],
    version: &str,
) -> Result<(), VersionError> {
    edits.modify(main_file, |text| {
        header::set(text, "Version", version).ok_or_else(|| VersionError::NotFound {
            path: main_file.to_owned(),
        })
    })?;

    for location in locations {
        let regex = validate_location(location)?;
        edits.modify(&location.path, |text| {
            let mut out = String::with_capacity(text.len());
            let mut last = 0;
            let mut replaced = false;
            for caps in regex.captures_iter(text) {
                if let Some(group) = caps.get(1) {
                    out.push_str(&text[last..group.start()]);
                    out.push_str(version);
                    last = group.end();
                    replaced = true;
                }
            }
            if !replaced {
                return Err(VersionError::NotFound {
                    path: location.path.clone(),
                });
            }
            out.push_str(&text[last..]);
            Ok(out)
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Coded;

    fn plugin(dir: &Path) {
        std::fs::write(
            dir.join("p.php"),
            "<?php\n/*\n * Plugin Name: P\n * Version: 1.0.0\n */\ndefine( 'P_VERSION', '1.0.0' );\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("readme.txt"),
            "=== P ===\nStable tag: 1.0.0\n\nShort.\n\n== Changelog ==\n\n= 1.0.0 =\n* First.\n",
        )
        .unwrap();
        std::fs::write(dir.join("package.json"), "{\n  \"version\": \"0.9.0\"\n}\n").unwrap();
    }

    fn locations() -> Vec<VersionLocation> {
        vec![
            VersionLocation {
                path: "p.php".into(),
                pattern: r"define\(\s*'P_VERSION',\s*'([^']+)'".into(),
            },
            VersionLocation {
                path: "package.json".into(),
                pattern: r#""version":\s*"([^"]+)""#.into(),
            },
        ]
    }

    #[test]
    fn reads_all_sources_with_lines() {
        let dir = tempfile::tempdir().unwrap();
        plugin(dir.path());
        let sources = read_sources(dir.path(), "p.php", &locations()).unwrap();
        let values: Vec<(&str, VersionSourceKind, Option<u32>)> = sources
            .iter()
            .map(|s| (s.value.as_str(), s.kind, s.line))
            .collect();
        assert_eq!(
            values,
            [
                ("1.0.0", VersionSourceKind::Header, Some(4)),
                ("1.0.0", VersionSourceKind::StableTag, Some(2)),
                ("1.0.0", VersionSourceKind::Changelog, Some(8)),
                ("1.0.0", VersionSourceKind::Custom, Some(6)),
                ("0.9.0", VersionSourceKind::Custom, Some(2)),
            ]
        );
    }

    #[test]
    fn writes_header_and_custom_locations_in_one_edit_per_file() {
        let dir = tempfile::tempdir().unwrap();
        plugin(dir.path());
        let mut edits = EditSet::new(dir.path());
        write_version(&mut edits, "p.php", &locations(), "1.1.0").unwrap();
        let changed = edits.changed();
        assert_eq!(changed.len(), 2);
        let main = changed.iter().find(|c| c.path == "p.php").unwrap();
        assert!(main.after.contains("Version: 1.1.0"));
        assert!(main.after.contains("'P_VERSION', '1.1.0'"));
    }

    #[test]
    fn rejects_patterns_without_exactly_one_group() {
        let loc = VersionLocation {
            path: "x".into(),
            pattern: "(a)(b)".into(),
        };
        assert_eq!(
            validate_location(&loc).unwrap_err().code(),
            "VERSION_BAD_PATTERN"
        );
        let loc = VersionLocation {
            path: "x".into(),
            pattern: "([".into(),
        };
        assert!(validate_location(&loc).is_err());
    }

    #[test]
    fn a_location_that_matches_nothing_reads_empty_and_fails_to_write() {
        let dir = tempfile::tempdir().unwrap();
        plugin(dir.path());
        let loc = vec![VersionLocation {
            path: "package.json".into(),
            pattern: r#""release":\s*"([^"]+)""#.into(),
        }];
        let sources = read_sources(dir.path(), "p.php", &loc).unwrap();
        assert_eq!(sources.last().unwrap().value, "");
        let mut edits = EditSet::new(dir.path());
        let err = write_version(&mut edits, "p.php", &loc, "2.0").unwrap_err();
        assert_eq!(err.code(), "VERSION_NOT_FOUND");
    }
}
