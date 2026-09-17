//! Every fixture plugin parses; detection reports what each one is built to show.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use svnpush_core::Coded;
use svnpush_core::detect::{self, DetectError, DetectOptions};
use svnpush_core::version::{VersionLocation, VersionSourceKind};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/plugins").join(name)
}

fn options(slug: &str) -> (String, Vec<VersionLocation>) {
    (format!("https://plugins.svn.wordpress.org/{slug}"), Vec::new())
}

fn detect_fixture(name: &str) -> Result<detect::PluginFacts, DetectError> {
    let (url, locations) = options(name);
    detect::detect(
        &fixture(name),
        DetectOptions { svn_url: &url, main_file: None, version_locations: &locations },
    )
}

#[test]
fn minimal() {
    let facts = detect_fixture("minimal").unwrap();
    assert_eq!(facts.name, "Minimal");
    assert_eq!(facts.slug, "minimal");
    assert_eq!(facts.main_file, "minimal.php");
    assert_eq!(facts.header.text_domain.as_deref(), Some("minimal"));
    assert!(facts.folder_matches_slug);
    assert!(!facts.has_distignore);
    let readme = facts.readme.unwrap();
    assert_eq!(readme.changelog[0].version.as_deref(), Some("1.0.0"));
    assert!(facts.versions.iter().all(|v| v.value == "1.0.0"));
    assert_eq!(facts.versions.len(), 3);
}

#[test]
fn composer_based_with_a_custom_version_constant() {
    let url = "https://plugins.svn.wordpress.org/composer-based";
    let locations = vec![VersionLocation {
        path: "composer-based.php".into(),
        pattern: r"define\(\s*'COMPOSER_BASED_VERSION',\s*'([^']+)'".into(),
    }];
    let facts = detect::detect(
        &fixture("composer-based"),
        DetectOptions { svn_url: url, main_file: None, version_locations: &locations },
    )
    .unwrap();
    assert!(facts.has_distignore);
    let custom: Vec<_> =
        facts.versions.iter().filter(|v| v.kind == VersionSourceKind::Custom).collect();
    assert_eq!(custom.len(), 1);
    assert_eq!(custom[0].value, "1.4.2");
}

#[test]
fn block_with_build() {
    let facts = detect_fixture("block-with-build").unwrap();
    assert_eq!(facts.header.version.as_deref(), Some("0.3.0"));
    assert!(facts.has_distignore);
}

#[test]
fn no_readme() {
    let facts = detect_fixture("no-readme").unwrap();
    assert!(facts.readme.is_none());
    assert_eq!(facts.versions.len(), 1);
    assert_eq!(facts.versions[0].kind, VersionSourceKind::Header);
}

#[test]
fn two_main_files_needs_a_choice() {
    let err = detect_fixture("two-main-files").unwrap_err();
    assert_eq!(err.code(), "DETECT_MULTIPLE_MAIN_FILES");
    match &err {
        DetectError::MultipleMainFiles { candidates } => {
            assert_eq!(candidates, &["demo.php", "two-main-files.php"]);
        }
        other => panic!("unexpected {other}"),
    }
    assert_eq!(err.fix().as_deref(), Some("Choose the main file in project settings."));

    let (url, locations) = options("two-main-files");
    let facts = detect::detect(
        &fixture("two-main-files"),
        DetectOptions {
            svn_url: &url,
            main_file: Some("two-main-files.php"),
            version_locations: &locations,
        },
    )
    .unwrap();
    assert_eq!(facts.name, "Two Main Files");
}

#[test]
fn a_chosen_file_without_a_header_is_rejected() {
    let (url, locations) = options("minimal");
    let err = detect::detect(
        &fixture("minimal"),
        DetectOptions {
            svn_url: &url,
            main_file: Some("readme.txt"),
            version_locations: &locations,
        },
    )
    .unwrap_err();
    assert_eq!(err.code(), "DETECT_NOT_A_MAIN_FILE");
}

#[test]
fn stable_tag_trunk() {
    let facts = detect_fixture("stable-tag-trunk").unwrap();
    let stable = facts.versions.iter().find(|v| v.kind == VersionSourceKind::StableTag).unwrap();
    assert_eq!(stable.value, "trunk");
}

#[test]
fn pre_release() {
    let facts = detect_fixture("pre-release").unwrap();
    assert!(facts.versions.iter().all(|v| v.value == "2.0.0-beta1"));
    let parsed = svnpush_core::version::Version::parse("2.0.0-beta1").unwrap();
    assert!(parsed.is_prerelease());
}

#[test]
fn real_world_authdock() {
    let url = "https://plugins.svn.wordpress.org/authdock";
    let facts = detect::detect(
        &fixture("real-world"),
        DetectOptions { svn_url: url, main_file: None, version_locations: &[] },
    )
    .unwrap();
    assert_eq!(facts.slug, "authdock");
    assert_eq!(facts.main_file, "authdock.php");
    assert!(!facts.folder_matches_slug);
    assert!(facts.name.starts_with("AuthDock"));
    assert_eq!(facts.header.text_domain.as_deref(), Some("authdock"));
    assert_eq!(facts.header.requires_php.as_deref(), Some("7.4"));

    let readme = facts.readme.unwrap();
    for required in svnpush_core::readme::REQUIRED_HEADERS {
        assert!(readme.header(required).is_some(), "{required}");
    }
    assert!(readme.short_description.chars().count() <= 150);
    assert!(readme.has_section("External services"));
    assert_eq!(readme.changelog[0].version.as_deref(), Some("2.2.2"));
    assert!(readme.changelog.len() >= 2);
    assert!(!readme.upgrade_notice.is_empty());
    let versions: Vec<&str> = facts.versions.iter().map(|v| v.value.as_str()).collect();
    assert_eq!(versions, ["2.2.2", "2.2.2", "2.2.2"]);
}

#[test]
fn invalid_svn_url_is_reported() {
    let err = detect::detect(
        &fixture("minimal"),
        DetectOptions { svn_url: "https://example.com/", main_file: None, version_locations: &[] },
    )
    .unwrap_err();
    assert_eq!(err.code(), "DETECT_INVALID_SVN_URL");
}
