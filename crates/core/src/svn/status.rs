//! `svn status --xml`, parsed.

use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

use super::{SvnError, slash};

/// The state of one path, from the `item` attribute of `wc-status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum StatusItem {
    /// Scheduled for addition.
    Added,
    /// Content or properties changed.
    Modified,
    /// Scheduled for deletion.
    Deleted,
    /// Deleted and re-added.
    Replaced,
    /// In conflict.
    Conflicted,
    /// Not under version control.
    Unversioned,
    /// Versioned but missing from disk.
    Missing,
    /// Unchanged, or anything else `svn` reports.
    Normal,
}

/// One row of `svn status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct StatusEntry {
    /// Path relative to the working copy root, with `/` separators.
    pub path: String,
    /// Local state.
    pub item: StatusItem,
    /// Whether only properties changed.
    pub props_only: bool,
    /// Whether a tree conflict is recorded.
    pub tree_conflict: bool,
    /// Whether the server has a newer version (only with `-u`).
    pub out_of_date: bool,
}

fn item(value: &str) -> StatusItem {
    match value {
        "added" => StatusItem::Added,
        "modified" => StatusItem::Modified,
        "deleted" => StatusItem::Deleted,
        "replaced" => StatusItem::Replaced,
        "conflicted" => StatusItem::Conflicted,
        "unversioned" => StatusItem::Unversioned,
        "missing" => StatusItem::Missing,
        _ => StatusItem::Normal,
    }
}

/// Parses `svn status --xml` output. Paths become relative to `wc_root`.
pub fn parse(xml: &str, wc_root: &Path) -> Result<Vec<StatusEntry>, SvnError> {
    let doc =
        roxmltree::Document::parse(xml).map_err(|e| SvnError::Parse { detail: e.to_string() })?;
    let root = slash(wc_root);
    let root = root.trim_end_matches('/');
    let mut entries = Vec::new();
    for node in doc.descendants().filter(|n| n.has_tag_name("entry")) {
        let Some(raw) = node.attribute("path") else { continue };
        let full = raw.replace('\\', "/");
        let path = full
            .strip_prefix(root)
            .map_or(full.as_str(), |rest| rest.trim_start_matches('/'))
            .to_owned();
        let wc = node.children().find(|c| c.has_tag_name("wc-status"));
        let repos = node.children().find(|c| c.has_tag_name("repos-status"));
        let local = wc.and_then(|w| w.attribute("item")).map_or(StatusItem::Normal, item);
        let props = wc.and_then(|w| w.attribute("props")).unwrap_or("none");
        let props_only = local == StatusItem::Normal && props == "modified";
        entries.push(StatusEntry {
            path,
            item: if props_only { StatusItem::Modified } else { local },
            props_only,
            tree_conflict: wc.and_then(|w| w.attribute("tree-conflicted")) == Some("true"),
            out_of_date: repos.is_some_and(|r| {
                r.attribute("item").is_some_and(|i| i != "none")
                    || r.attribute("props").is_some_and(|p| p != "none")
            }),
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<status>
<target path="C:\wc\trunk">
<entry path="C:\wc\trunk\a.php">
<wc-status item="modified" props="none" revision="3"><commit revision="3"/></wc-status>
</entry>
<entry path="C:\wc\trunk\img\b.png">
<wc-status item="added" props="modified" revision="-1"></wc-status>
</entry>
<entry path="C:\wc\trunk\c.php">
<wc-status item="normal" props="modified" revision="3"></wc-status>
<repos-status item="modified" props="none"/>
</entry>
<entry path="C:\wc\trunk\d.php">
<wc-status item="conflicted" props="none" tree-conflicted="true" revision="3"></wc-status>
</entry>
<entry path="C:\wc\trunk\new.txt">
<wc-status item="unversioned" props="none"></wc-status>
</entry>
</target>
</status>"#;

    #[test]
    fn parses_items_props_and_remote_state() {
        let entries = parse(XML, Path::new(r"C:\wc")).unwrap();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].path, "trunk/a.php");
        assert_eq!(entries[0].item, StatusItem::Modified);
        assert_eq!(entries[1].path, "trunk/img/b.png");
        assert_eq!(entries[1].item, StatusItem::Added);
        assert!(!entries[1].props_only);
        assert!(entries[2].props_only);
        assert!(entries[2].out_of_date);
        assert!(entries[3].tree_conflict);
        assert_eq!(entries[3].item, StatusItem::Conflicted);
        assert_eq!(entries[4].item, StatusItem::Unversioned);
    }

    #[test]
    fn bad_xml_is_a_parse_error() {
        assert!(matches!(parse("<status", Path::new("/wc")), Err(SvnError::Parse { .. })));
    }
}
