//! The WordPress.org readme validator's rules, run locally so a readme can be
//! checked instantly and without uploading it. The rules and limits follow
//! the official validator (`class-validator.php`) and parser (`class-parser.php`)
//! in the WordPress/wordpress.org repository. Checks that need WordPress.org's own data
//! (trademarked names, whether contributor usernames exist, how popular a
//! tag is) are left to the official page, which the app links to.

use serde::Serialize;
use ts_rs::TS;

use super::{Readme, parse};

/// The official validator's page.
pub const OFFICIAL_VALIDATOR_URL: &str =
    "https://wordpress.org/plugins/developers/readme-validator/";

/// Longest short description WordPress.org shows, in characters.
const SHORT_DESCRIPTION_CHARS: usize = 150;
/// Most tags WordPress.org keeps.
const MAX_TAGS: usize = 5;
/// Tags WordPress.org ignores.
const IGNORED_TAGS: [&str; 2] = ["plugin", "wordpress"];
/// Words per section before WordPress.org truncates it.
const SECTION_WORDS: usize = 2500;
/// Words for the Changelog and FAQ sections.
const LONG_SECTION_WORDS: usize = 5000;

/// Licences WordPress.org rejects.
const INCOMPATIBLE_LICENSES: [&str; 9] = [
    "4 clause bsd",
    "apache 1",
    "cc by-nc",
    "noncommercial",
    "cc by-nd",
    "noderivative",
    "eupl",
    "osl",
    "proprietary",
];
/// Licence names WordPress.org recognises as GPL-compatible.
const COMPATIBLE_LICENSES: [&str; 9] =
    ["gpl", "mit", "isc", "apache 2", "bsd", "mpl", "public domain", "cc0", "zlib"];

/// How serious an issue is, as WordPress.org grades it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[ts(export)]
pub enum IssueLevel {
    /// WordPress.org rejects the readme.
    Error,
    /// Something is ignored or cut off on the plugin page.
    Warning,
    /// A suggestion.
    Note,
}

/// One validator finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ReadmeIssue {
    /// How serious it is.
    pub level: IssueLevel,
    /// The official validator's key, for example `stable_tag_invalid`.
    pub code: String,
    /// What to change.
    pub message: String,
}

/// Every finding for one readme, most serious first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ReadmeReport {
    /// The findings.
    pub issues: Vec<ReadmeIssue>,
    /// The official page, for the checks that need WordPress.org's data.
    pub official_url: String,
}

impl ReadmeReport {
    /// Findings at `level`.
    pub fn at(&self, level: IssueLevel) -> impl Iterator<Item = &ReadmeIssue> {
        self.issues.iter().filter(move |i| i.level == level)
    }
}

fn wp_version_ok(value: &str) -> bool {
    // !^\d+\.\d(\.\d+)?$!
    let parts: Vec<&str> = value.split('.').collect();
    matches!(parts.len(), 2 | 3)
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        && parts[1].len() == 1
}

fn php_version_ok(value: &str) -> bool {
    // !^\d+(\.\d+){1,2}$!
    let parts: Vec<&str> = value.split('.').collect();
    matches!(parts.len(), 2 | 3)
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// WordPress.org ignores a WordPress version above the current release plus 0.1.
fn major_minor(value: &str) -> Option<(u64, u64)> {
    let mut parts = value.trim().split('.').map(|p| p.parse::<u64>().ok());
    Some((parts.next()??, parts.next()??))
}

/// WordPress.org ignores a WordPress version above the next release (current
/// branch plus 0.1). Compared by major.minor, the precision readmes use.
fn beyond_current(value: &str, current: Option<&str>) -> bool {
    let (Some(value), Some((major, minor))) = (major_minor(value), current.and_then(major_minor))
    else {
        return false;
    };
    value > (major, minor + 1)
}

fn section_key(title: &str) -> String {
    let key = title.trim().to_ascii_lowercase().replace(' ', "_");
    match key.as_str() {
        "frequently_asked_questions" => "faq".to_owned(),
        "change_log" => "changelog".to_owned(),
        "screenshot" => "screenshots".to_owned(),
        _ => key,
    }
}

fn section_name(key: &str) -> &str {
    match key {
        "description" => "Description",
        "installation" => "Installation",
        "faq" => "Frequently Asked Questions",
        "screenshots" => "Screenshots",
        "changelog" => "Changelog",
        "upgrade_notice" => "Upgrade Notice",
        _ => key,
    }
}

struct Findings(Vec<ReadmeIssue>);

impl Findings {
    fn add(&mut self, level: IssueLevel, code: &str, message: impl Into<String>) {
        self.0.push(ReadmeIssue { level, code: code.to_owned(), message: message.into() });
    }
}

fn check_name(readme: &Readme, out: &mut Findings) {
    let name = readme.name.as_deref().map(str::trim).unwrap_or_default();
    let looks_like_header = name.split_once(':').is_some_and(|(key, _)| {
        readme.header(key.trim()).is_some() || super::REQUIRED_HEADERS.contains(&key.trim())
    });
    if name.is_empty() || name.eq_ignore_ascii_case("plugin name") || looks_like_header {
        out.add(
            IssueLevel::Error,
            "invalid_plugin_name_header",
            "We cannot find a plugin name in your readme. Plugin names look like: === Plugin Name ===. Change Plugin Name to the actual name of your plugin.",
        );
    }
}

fn check_license(readme: &Readme, out: &mut Findings) {
    let Some(license) =
        readme.header("License").map(|h| h.value.to_ascii_lowercase()).filter(|l| !l.is_empty())
    else {
        out.add(IssueLevel::Warning, "license_missing", "The License field is missing. A GPLv2 or later compatible license should be specified.");
        return;
    };
    if INCOMPATIBLE_LICENSES.iter().any(|bad| license.contains(bad)) {
        out.add(IssueLevel::Error, "invalid_license", "The License field appears to be invalid. A GPLv2 or later compatible license should be specified.");
    } else if !COMPATIBLE_LICENSES.iter().any(|good| license.contains(good)) {
        out.add(IssueLevel::Note, "unknown_license", "The License field could not be validated. A GPLv2 or later compatible license should be specified. The specified license may be compatible.");
    }
}

fn check_versions(readme: &Readme, current: Option<&str>, out: &mut Findings) {
    match readme.header("Requires at least").map(|h| h.value.as_str()).filter(|v| !v.is_empty()) {
        Some(v) if !wp_version_ok(v) || beyond_current(v, current) => out.add(
            IssueLevel::Warning,
            "requires_header_ignored",
            "The Requires at least field was ignored. It should only contain a valid WordPress version such as 6.5.",
        ),
        Some(_) => {}
        None => out.add(IssueLevel::Note, "requires_header_missing", "The Requires at least field is missing. It should be defined here, or in your main plugin file."),
    }
    match readme.header("Tested up to").map(|h| h.value.as_str()).filter(|v| !v.is_empty()) {
        Some(v) if !wp_version_ok(v) || beyond_current(v, current) => out.add(
            IssueLevel::Warning,
            "tested_header_ignored",
            "The Tested up to field was ignored. It should only contain a valid WordPress version such as 6.8, no higher than the next release.",
        ),
        Some(_) => {}
        None => out.add(IssueLevel::Warning, "tested_header_missing", "The Tested up to field is missing."),
    }
    match readme.header("Requires PHP").map(|h| h.value.as_str()).filter(|v| !v.is_empty()) {
        Some(v) if !php_version_ok(v) => out.add(
            IssueLevel::Warning,
            "requires_php_header_ignored",
            "The Requires PHP field was ignored. It should only contain a PHP version such as 7.4.",
        ),
        Some(_) => {}
        None => out.add(IssueLevel::Note, "requires_php_header_missing", "The Requires PHP field is missing. It should be defined here, or in your main plugin file."),
    }
    let stable =
        readme.header("Stable tag").map(|h| h.value.to_ascii_lowercase()).unwrap_or_default();
    if stable.is_empty() || stable.contains("trunk") {
        out.add(IssueLevel::Warning, "stable_tag_invalid", "The Stable tag field is missing or invalid. Do not use trunk as the stable tag, so every release can be rolled back.");
    }
}

fn check_tags(readme: &Readme, out: &mut Findings) {
    let tags: Vec<String> = readme
        .header("Tags")
        .map(|h| {
            h.value.split(',').map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()).collect()
        })
        .unwrap_or_default();
    let ignored: Vec<&str> = tags
        .iter()
        .map(String::as_str)
        .filter(|t| IGNORED_TAGS.contains(&t.to_ascii_lowercase().as_str()))
        .collect();
    if !ignored.is_empty() {
        out.add(
            IssueLevel::Warning,
            "ignored_tags",
            format!(
                "One or more tags were ignored. These tags are not permitted: {}.",
                ignored.join(", ")
            ),
        );
    }
    let kept: Vec<&String> =
        tags.iter().filter(|t| !IGNORED_TAGS.contains(&t.to_ascii_lowercase().as_str())).collect();
    if kept.len() > MAX_TAGS {
        let extra: Vec<&str> = kept[MAX_TAGS..].iter().map(|t| t.as_str()).collect();
        out.add(
            IssueLevel::Warning,
            "too_many_tags",
            format!(
                "One or more tags were ignored: {}. Limit your plugin to 5 tags.",
                extra.join(", ")
            ),
        );
    }
}

fn check_text(readme: &Readme, out: &mut Findings) {
    let short = readme.short_description.trim();
    if short.is_empty() {
        out.add(IssueLevel::Note, "no_short_description_present", "The Short Description is missing. WordPress.org will use the start of your Description instead.");
    } else if short.chars().count() > SHORT_DESCRIPTION_CHARS {
        out.add(IssueLevel::Warning, "trimmed_short_description", format!("The Short Description is too long and will be cut off. A maximum of {SHORT_DESCRIPTION_CHARS} characters is supported."));
    }
    for section in &readme.sections {
        let key = section_key(&section.title);
        let limit = if matches!(key.as_str(), "changelog" | "faq") {
            LONG_SECTION_WORDS
        } else {
            SECTION_WORDS
        };
        if section.body.split_whitespace().count() > limit {
            out.add(
                IssueLevel::Warning,
                &format!("trimmed_section_{key}"),
                format!("The {} section is too long and will be cut off. A maximum of {limit} words is supported.", section_name(&key)),
            );
        }
    }
}

fn check_optional(readme: &Readme, out: &mut Findings) {
    if readme.header("Contributors").is_none_or(|h| h.value.trim().is_empty()) {
        out.add(IssueLevel::Note, "contributors_missing", "The Contributors field is missing.");
    }
    let has = |key: &str| {
        readme.sections.iter().any(|s| section_key(&s.title) == key && !s.body.trim().is_empty())
    };
    for (key, code, heading) in [
        ("faq", "faq_missing", "== Frequently Asked Questions =="),
        ("changelog", "changelog_missing", "== Changelog =="),
        ("upgrade_notice", "upgrade_notice_missing", "== Upgrade Notice =="),
        ("screenshots", "screenshots_missing", "== Screenshots =="),
    ] {
        if !has(key) {
            out.add(IssueLevel::Note, code, format!("No {heading} section was found."));
        }
    }
    if readme.header("Donate link").is_none_or(|h| h.value.trim().is_empty()) {
        out.add(IssueLevel::Note, "donate_link_missing", "No donate link was found.");
    }
}

/// Validates `text` as WordPress.org would. `current_wordpress` (when known)
/// lets version headers above the next release be flagged.
pub fn validate(text: &str, current_wordpress: Option<&str>) -> ReadmeReport {
    let readme = parse(text);
    let mut out = Findings(Vec::new());
    check_name(&readme, &mut out);
    check_license(&readme, &mut out);
    check_versions(&readme, current_wordpress, &mut out);
    check_tags(&readme, &mut out);
    check_text(&readme, &mut out);
    check_optional(&readme, &mut out);
    let mut issues = out.0;
    issues.sort_by_key(|i| i.level);
    ReadmeReport { issues, official_url: OFFICIAL_VALIDATOR_URL.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "=== Hello Release ===
Contributors: hellorelease
Donate link: https://example.org/donate
Tags: admin, footer
Requires at least: 6.0
Tested up to: 6.8
Requires PHP: 7.4
Stable tag: 1.0.0
License: GPLv2 or later
License URI: https://www.gnu.org/licenses/gpl-2.0.html

A short thank-you note in the admin footer.

== Description ==

Hello Release replaces the admin footer text.

== Frequently Asked Questions ==

= Does it work? =

Yes.

== Screenshots ==

1. The footer.

== Changelog ==

= 1.0.0 =
* First release.

== Upgrade Notice ==

= 1.0.0 =
First release.
";

    fn codes(report: &ReadmeReport) -> Vec<&str> {
        report.issues.iter().map(|i| i.code.as_str()).collect()
    }

    #[test]
    fn a_complete_readme_has_no_findings() {
        assert!(validate(GOOD, Some("6.8.2")).issues.is_empty(), "{:?}", validate(GOOD, None));
    }

    #[test]
    fn errors_come_first_and_block_nothing_else() {
        let text = GOOD
            .replace("=== Hello Release ===", "=== Plugin Name ===")
            .replace("GPLv2 or later", "Proprietary");
        let report = validate(&text, None);
        assert_eq!(codes(&report)[..2], ["invalid_plugin_name_header", "invalid_license"]);
        assert_eq!(report.at(IssueLevel::Error).count(), 2);
    }

    #[test]
    fn header_values_follow_the_official_patterns() {
        let text = GOOD
            .replace("Tested up to: 6.8", "Tested up to: 6.8 (latest)")
            .replace("Requires PHP: 7.4", "Requires PHP: 7")
            .replace("Requires at least: 6.0", "Requires at least: 6.10")
            .replace("Stable tag: 1.0.0", "Stable tag: trunk");
        let found = codes(&validate(&text, None)).join(",");
        for code in [
            "tested_header_ignored",
            "requires_php_header_ignored",
            "requires_header_ignored",
            "stable_tag_invalid",
        ] {
            assert!(found.contains(code), "{code} in {found}");
        }
        let ahead = GOOD.replace("Tested up to: 6.8", "Tested up to: 7.1");
        assert!(codes(&validate(&ahead, Some("6.8.2"))).contains(&"tested_header_ignored"));
        assert!(codes(&validate(&GOOD.replace("6.8", "6.9"), Some("6.8.2"))).is_empty());
    }

    #[test]
    fn tags_short_description_and_sections_are_limited() {
        let text = GOOD
            .replace("Tags: admin, footer", "Tags: plugin, a, b, c, d, e, f")
            .replace("A short thank-you note in the admin footer.", &"x".repeat(151));
        let report = validate(&text, None);
        let found = codes(&report);
        assert!(found.contains(&"ignored_tags") && found.contains(&"too_many_tags"));
        assert!(found.contains(&"trimmed_short_description"));
        let long =
            GOOD.replace("Hello Release replaces the admin footer text.", &"word ".repeat(2501));
        assert!(codes(&validate(&long, None)).contains(&"trimmed_section_description"));
    }

    #[test]
    fn optional_parts_are_notes() {
        let bare = "=== Tiny ===\nStable tag: 1.0\nLicense: MIT\nTested up to: 6.8\n\nTiny.\n\n== Description ==\n\nTiny.\n";
        let report = validate(bare, None);
        assert_eq!(report.at(IssueLevel::Error).count(), 0);
        let notes: Vec<&str> = report.at(IssueLevel::Note).map(|i| i.code.as_str()).collect();
        for code in [
            "contributors_missing",
            "faq_missing",
            "changelog_missing",
            "upgrade_notice_missing",
            "screenshots_missing",
            "donate_link_missing",
            "requires_header_missing",
            "requires_php_header_missing",
        ] {
            assert!(notes.contains(&code), "{code}");
        }
        assert!(
            codes(&validate(&GOOD.replace("GPLv2 or later", "WTFPL"), None))
                .contains(&"unknown_license")
        );
    }
}

#[cfg(test)]
mod real_world {
    use super::*;

    #[test]
    fn a_published_plugin_readme_has_no_validator_errors() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/plugins/real-world/readme.txt");
        let text = std::fs::read_to_string(path).unwrap();
        let report = validate(&text, None);
        let serious: Vec<&ReadmeIssue> =
            report.issues.iter().filter(|i| i.level != IssueLevel::Note).collect();
        assert!(serious.is_empty(), "{serious:?}");
    }
}
