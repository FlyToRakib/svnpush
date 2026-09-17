//! The manual draft: sensible pre-filled values and validation of what the
//! developer approves.

use crate::detect::PluginFacts;
use crate::version::Version;

use super::changes::ChangeSet;
use super::model::{ErrorView, ReleaseDraft};

/// The version to pre-fill: the header's version when it is already newer
/// than the previous release, otherwise the next patch.
pub fn suggested_version(facts: &PluginFacts, previous: Option<&str>) -> String {
    let header = facts.header.version.as_deref().and_then(|v| Version::parse(v).ok());
    let previous = previous.and_then(|p| Version::parse(p).ok());
    match (header, previous) {
        (Some(h), Some(p)) if h > p => h.to_string(),
        (_, Some(p)) => p.next_patch().to_string(),
        (Some(h), None) => h.to_string(),
        (None, None) => "1.0.0".to_owned(),
    }
}

/// A draft pre-filled from the readme (an entry already written for this
/// version) or the commit subjects.
pub fn prefill(facts: &PluginFacts, version: &str, changes: &ChangeSet) -> ReleaseDraft {
    let readme = facts.readme.as_ref();
    let entry_for = |entries: &[crate::readme::ChangelogEntry]| {
        entries.iter().find(|e| e.version.as_deref() == Some(version)).map(|e| e.body.clone())
    };
    let changelog = readme.and_then(|r| entry_for(&r.changelog)).unwrap_or_else(|| {
        changes.commits.iter().map(|c| format!("* {c}")).collect::<Vec<_>>().join("\n")
    });
    let upgrade_notice = readme.and_then(|r| entry_for(&r.upgrade_notice)).unwrap_or_default();
    ReleaseDraft {
        version: version.to_owned(),
        reason: String::new(),
        changelog_markdown: changelog,
        upgrade_notice,
        summary: String::new(),
        provider: None,
        fell_back_from: None,
    }
}

/// Checks a draft before it is written. Version ordering is left to V03 so
/// the check table stays the single place that decides.
pub fn validate_draft(draft: &ReleaseDraft) -> Result<(), ErrorView> {
    if Version::parse(draft.version.trim()).is_err() {
        return Err(ErrorView::new(
            "DRAFT_INVALID_VERSION",
            format!("\"{}\" is not a valid version.", draft.version),
            Some(
                "Use x.y or x.y.z, optionally followed by a pre-release suffix such as -beta1."
                    .to_owned(),
            ),
        ));
    }
    if draft.version.trim() != draft.version {
        return Err(ErrorView::new(
            "DRAFT_INVALID_VERSION",
            "The version has leading or trailing spaces.",
            Some("Remove the spaces.".to_owned()),
        ));
    }
    if draft.changelog_markdown.trim().is_empty() {
        return Err(ErrorView::new(
            "DRAFT_EMPTY_CHANGELOG",
            "The changelog entry is empty.",
            Some("Describe what changed in this release.".to_owned()),
        ));
    }
    if draft.changelog_markdown.lines().any(|l| l.trim_start().starts_with("==")) {
        return Err(ErrorView::new(
            "DRAFT_HEADING_IN_CHANGELOG",
            "The changelog entry contains a readme section heading.",
            Some("Remove lines that start with ==.".to_owned()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::Header;
    use crate::readme;
    use crate::run::changes::ChangeSource;

    fn facts(header_version: &str, readme_text: &str) -> PluginFacts {
        PluginFacts {
            name: "P".into(),
            slug: "p".into(),
            main_file: "p.php".into(),
            header: Header { version: Some(header_version.into()), ..Header::default() },
            readme: Some(readme::parse(readme_text)),
            versions: Vec::new(),
            folder_matches_slug: true,
            has_distignore: false,
            has_project_config: false,
        }
    }

    fn changes(commits: &[&str]) -> ChangeSet {
        ChangeSet {
            source: ChangeSource::Git,
            base: Some("v1.0.0".into()),
            files: Vec::new(),
            commits: commits.iter().map(|c| (*c).to_owned()).collect(),
        }
    }

    #[test]
    fn suggests_the_bumped_header_or_the_next_patch() {
        assert_eq!(suggested_version(&facts("1.2.0", ""), Some("1.1.0")), "1.2.0");
        assert_eq!(suggested_version(&facts("1.1.0", ""), Some("1.1.0")), "1.1.1");
        assert_eq!(suggested_version(&facts("0.9", ""), None), "0.9");
        assert_eq!(suggested_version(&facts("bad", ""), None), "1.0.0");
    }

    #[test]
    fn prefills_from_an_existing_entry_or_commits() {
        let text = "=== P ===\n\n== Changelog ==\n\n= 1.2.0 =\n* Written already.\n\n== Upgrade Notice ==\n\n= 1.2.0 =\nUpdate.\n";
        let d = prefill(&facts("1.2.0", text), "1.2.0", &changes(&["Fix a"]));
        assert_eq!(d.changelog_markdown, "* Written already.");
        assert_eq!(d.upgrade_notice, "Update.");
        let d = prefill(&facts("1.2.0", text), "1.3.0", &changes(&["Fix a", "Add b"]));
        assert_eq!(d.changelog_markdown, "* Fix a\n* Add b");
        assert_eq!(d.upgrade_notice, "");
    }

    #[test]
    fn validation() {
        let mut d = prefill(&facts("1.0", ""), "1.0.1", &changes(&["x"]));
        assert!(validate_draft(&d).is_ok());
        d.version = "1.0.1 ".into();
        assert_eq!(validate_draft(&d).unwrap_err().code, "DRAFT_INVALID_VERSION");
        d.version = "one".into();
        assert_eq!(validate_draft(&d).unwrap_err().code, "DRAFT_INVALID_VERSION");
        d.version = "1.0.1".into();
        d.changelog_markdown = "  ".into();
        assert_eq!(validate_draft(&d).unwrap_err().code, "DRAFT_EMPTY_CHANGELOG");
        d.changelog_markdown = "* a\n== FAQ ==".into();
        assert_eq!(validate_draft(&d).unwrap_err().code, "DRAFT_HEADING_IN_CHANGELOG");
    }
}
