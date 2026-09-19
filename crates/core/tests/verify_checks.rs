//! Every check has a passing and a failing case.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use svnpush_core::detect::{Header, PluginFacts};
use svnpush_core::readme::{self, Readme};
use svnpush_core::verify::{
    self, CheckResult, CheckStatus, FileRef, VerifyInput, WorkingCopyState,
};
use svnpush_core::version::{VersionSource, VersionSourceKind};

const README: &str = "=== Demo ===
Contributors: someone
Tags: release, svn
Requires at least: 6.0
Tested up to: 6.8
Requires PHP: 7.4
Stable tag: 1.2.0
License: GPLv2 or later
License URI: https://www.gnu.org/licenses/gpl-2.0.html

Short and sweet.

== Screenshots ==

1. The screen.
2. Another screen.

== Changelog ==

= 1.2.0 =
* New.

= 1.1.0 =
* Old.
";

const MAIN: &str = "<?php\n/**\n * Plugin Name: Demo\n * Version: 1.2.0\n * Text Domain: demo\n * Requires at least: 6.0\n * Requires PHP: 7.4\n */\nif ( ! defined( 'ABSPATH' ) ) {\n\texit;\n}\n";

struct Case {
    facts: PluginFacts,
    version: String,
    previous: Option<String>,
    tags: Vec<String>,
    readme_text: Option<String>,
    main_text: String,
    files: Vec<(String, u64)>,
    zip: Option<Vec<String>>,
    required: Vec<String>,
    allow_phar: bool,
    svn: Option<String>,
    wc: WorkingCopyState,
    credentials: Option<bool>,
    dirty: Option<Vec<String>>,
    wordpress: Option<String>,
    assets: Option<Vec<String>>,
    gitignored: Vec<String>,
}

fn source(label: &str, path: &str, kind: VersionSourceKind) -> VersionSource {
    VersionSource {
        label: label.into(),
        path: path.into(),
        value: "1.2.0".into(),
        kind,
        line: Some(1),
    }
}

impl Case {
    fn passing() -> Self {
        let readme = readme::parse(README);
        Self {
            facts: PluginFacts {
                name: "Demo".into(),
                slug: "demo".into(),
                main_file: "demo.php".into(),
                header: Header {
                    name: Some("Demo".into()),
                    version: Some("1.2.0".into()),
                    text_domain: Some("demo".into()),
                    requires_at_least: Some("6.0".into()),
                    requires_php: Some("7.4".into()),
                    license: None,
                },
                readme: Some(readme),
                versions: vec![
                    source("Plugin header", "demo.php", VersionSourceKind::Header),
                    source("Readme stable tag", "readme.txt", VersionSourceKind::StableTag),
                    source("Readme changelog", "readme.txt", VersionSourceKind::Changelog),
                ],
                folder_matches_slug: true,
                has_distignore: false,
                has_project_config: false,
            },
            version: "1.2.0".into(),
            previous: Some("1.1.0".into()),
            tags: vec!["1.0.0".into(), "1.1.0".into()],
            readme_text: Some(README.into()),
            main_text: MAIN.into(),
            files: vec![
                ("demo.php".into(), 400),
                ("includes/class-demo.php".into(), 2_000),
                ("readme.txt".into(), 600),
            ],
            zip: Some(vec![
                "demo/".into(),
                "demo/demo.php".into(),
                "demo/includes/".into(),
                "demo/includes/class-demo.php".into(),
                "demo/readme.txt".into(),
            ]),
            required: vec!["includes".into()],
            allow_phar: false,
            svn: Some("1.14.5 (r1922182)".into()),
            wc: WorkingCopyState::Clean,
            credentials: Some(true),
            dirty: Some(Vec::new()),
            wordpress: Some("6.8.2".into()),
            assets: Some(vec![
                "banner-772x250.png".into(),
                "screenshot-1.png".into(),
                "screenshot-2.jpg".into(),
            ]),
            gitignored: Vec::new(),
        }
    }

    fn results(&self) -> Vec<CheckResult> {
        let files: Vec<FileRef<'_>> =
            self.files.iter().map(|(rel, size)| FileRef { rel, size: *size }).collect();
        let input = VerifyInput {
            facts: &self.facts,
            version: &self.version,
            previous: self.previous.as_deref(),
            server_tags: &self.tags,
            readme_text: self.readme_text.as_deref(),
            main_file_text: &self.main_text,
            files: &files,
            zip_entries: self.zip.as_deref(),
            required_paths: &self.required,
            allow_phar: self.allow_phar,
            svn_version: self.svn.as_deref(),
            working_copy: &self.wc,
            has_credentials: self.credentials,
            git_dirty: self.dirty.as_deref(),
            current_wordpress: self.wordpress.as_deref(),
            assets: self.assets.as_deref(),
            gitignored: &self.gitignored,
        };
        verify::run(&input)
    }

    fn status(&self, id: &str) -> CheckResult {
        self.results().into_iter().find(|r| r.id == id).unwrap_or_else(|| panic!("no check {id}"))
    }

    fn readme(&mut self, text: &str) {
        self.readme_text = Some(text.to_owned());
        self.facts.readme = Some(readme::parse(text));
    }
}

fn assert_fails(case: &Case, id: &str) -> CheckResult {
    let result = case.status(id);
    assert_eq!(result.status, CheckStatus::Fail, "{id}: {}", result.message);
    assert!(result.fix.is_some(), "{id} has no fix");
    result
}

#[test]
fn the_baseline_passes_every_check() {
    let results = Case::passing().results();
    assert_eq!(results.len(), 28);
    for r in &results {
        assert_eq!(r.status, CheckStatus::Pass, "{} {}: {}", r.id, r.title, r.message);
    }
    assert!(!verify::is_blocked(&results));
    let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids[0], "V01");
    assert_eq!(ids[15], "V16");
    assert_eq!(ids[16], "V17");
    assert_eq!(ids[27], "W11");
}

#[test]
fn v01_fails_on_a_mismatched_source() {
    let mut case = Case::passing();
    case.facts.versions[1].value = "1.1.0".into();
    let r = assert_fails(&case, "V01");
    assert_eq!(r.paths, ["readme.txt"]);
    assert!(r.message.contains("Readme stable tag: 1.1.0"));
    assert!(verify::is_blocked(&case.results()));
}

#[test]
fn v02_fails_on_an_invalid_version() {
    let mut case = Case::passing();
    case.version = "1.2.0.1".into();
    assert_fails(&case, "V02");
}

#[test]
fn v03_fails_when_not_newer() {
    let mut case = Case::passing();
    case.version = "1.1".into();
    case.tags = vec!["1.0.0".into()];
    let r = assert_fails(&case, "V03");
    assert!(r.fix.unwrap().contains("1.1.1"));
}

#[test]
fn v03_fails_when_the_tag_exists_even_if_written_differently() {
    let mut case = Case::passing();
    case.tags.push("1.2".into());
    let r = assert_fails(&case, "V03");
    assert_eq!(r.paths, ["tags/1.2"]);
}

#[test]
fn v03_passes_on_a_first_release() {
    let mut case = Case::passing();
    case.previous = None;
    case.tags.clear();
    assert_eq!(case.status("V03").status, CheckStatus::Pass);
}

#[test]
fn v04_fails_when_the_newest_entry_is_another_version() {
    let mut case = Case::passing();
    case.readme(&README.replace("= 1.2.0 =\n* New.\n\n", ""));
    assert_fails(&case, "V04");
}

#[test]
fn v05_fails_on_missing_headers() {
    let mut case = Case::passing();
    case.readme(&README.replace("Tested up to: 6.8\n", "").replace("=== Demo ===\n", ""));
    let r = assert_fails(&case, "V05");
    assert!(r.message.contains("Tested up to"));
    assert!(r.message.contains("=== Plugin Name ==="));
}

#[test]
fn v05_fails_without_a_readme() {
    let mut case = Case::passing();
    case.facts.readme = None;
    case.readme_text = None;
    assert_fails(&case, "V05");
}

#[test]
fn v06_fails_on_trunk() {
    let mut case = Case::passing();
    case.readme(&README.replace("Stable tag: 1.2.0", "Stable tag: trunk"));
    let r = assert_fails(&case, "V06");
    assert!(r.message.contains("trunk"));
}

#[test]
fn v07_fails_on_a_long_short_description() {
    let mut case = Case::passing();
    case.readme(&README.replace("Short and sweet.", &"word ".repeat(40)));
    let r = assert_fails(&case, "V07");
    assert!(r.message.contains("199 characters"));
}

#[test]
fn v08_fails_on_another_text_domain() {
    let mut case = Case::passing();
    case.facts.header.text_domain = Some("demo-plugin".into());
    assert_fails(&case, "V08");
}

#[test]
fn v09_fails_without_an_abspath_guard() {
    let mut case = Case::passing();
    case.main_text = "<?php\n/* Plugin Name: Demo */\nadd_action( 'init', 'demo' );\n".into();
    assert_fails(&case, "V09");
    case.main_text = "<?php\ndefined(\"WPINC\") || die;\n".into();
    assert_eq!(case.status("V09").status, CheckStatus::Pass);
}

#[test]
fn v10_fails_on_a_missing_required_path() {
    let mut case = Case::passing();
    case.files.retain(|(rel, _)| rel != "readme.txt");
    case.required.push("assets/app.js".into());
    let r = assert_fails(&case, "V10");
    assert_eq!(r.paths, ["readme.txt", "assets/app.js"]);
}

#[test]
fn v11_fails_on_a_forbidden_path() {
    let mut case = Case::passing();
    case.files.push(("sub/.DS_Store".into(), 10));
    case.files.push(("old.tar.gz".into(), 10));
    let r = assert_fails(&case, "V11");
    assert_eq!(r.paths.len(), 2);
}

#[test]
fn v11_fails_on_files_that_hold_secrets() {
    for secret in [
        ".env",
        "config/.env.production",
        "certs/site.PEM",
        "deploy.key",
        "wp-config.php",
        "keys/id_rsa",
    ] {
        let mut case = Case::passing();
        case.files.push((secret.into(), 10));
        let r = assert_fails(&case, "V11");
        assert_eq!(r.paths, [secret]);
    }
    let mut case = Case::passing();
    case.files.push(("includes/class-keyring.php".into(), 10));
    case.files.push(("assets/environment.js".into(), 10));
    assert_eq!(case.status("V11").status, CheckStatus::Pass);
}

#[test]
fn v12_fails_on_executables_and_phar_unless_allowed() {
    let mut case = Case::passing();
    case.files.push(("bin/tool.phar".into(), 10));
    assert_fails(&case, "V12");
    case.allow_phar = true;
    assert_eq!(case.status("V12").status, CheckStatus::Pass);
    case.files.push(("lib/native.DLL".into(), 10));
    assert_fails(&case, "V12");
}

#[test]
fn v13_fails_on_entries_outside_the_slug_and_skips_before_build() {
    let mut case = Case::passing();
    case.zip = Some(vec!["demo/demo.php".into(), "other\\readme.txt".into()]);
    let r = assert_fails(&case, "V13");
    assert_eq!(r.paths, ["other\\readme.txt"]);
    case.zip = None;
    assert_eq!(case.status("V13").status, CheckStatus::Skip);
}

#[test]
fn v14_fails_when_svn_is_missing_or_old() {
    let mut case = Case::passing();
    case.svn = None;
    assert_fails(&case, "V14");
    case.svn = Some("1.9.7".into());
    assert_fails(&case, "V14");
    case.svn = Some("1.10.0".into());
    assert_eq!(case.status("V14").status, CheckStatus::Pass);
}

#[test]
fn v15_fails_on_conflicts_and_skips_before_checkout() {
    let mut case = Case::passing();
    case.wc = WorkingCopyState::Conflicts(vec!["trunk/demo.php".into()]);
    assert_fails(&case, "V15");
    case.wc = WorkingCopyState::OutOfDate(vec!["trunk/readme.txt".into()]);
    assert_fails(&case, "V15");
    case.wc = WorkingCopyState::NotCreated;
    assert_eq!(case.status("V15").status, CheckStatus::Skip);
}

#[test]
fn v16_fails_without_credentials_and_skips_in_a_dry_run() {
    let mut case = Case::passing();
    case.credentials = Some(false);
    assert_fails(&case, "V16");
    case.credentials = None;
    assert_eq!(case.status("V16").status, CheckStatus::Skip);
}

#[test]
fn w01_warns_on_a_dirty_tree_and_skips_without_git() {
    let mut case = Case::passing();
    case.dirty = Some(vec!["demo.php".into()]);
    assert_fails(&case, "W01");
    assert!(!verify::is_blocked(&case.results()));
    case.dirty = None;
    assert_eq!(case.status("W01").status, CheckStatus::Skip);
}

#[test]
fn w02_warns_when_tested_up_to_is_old() {
    let mut case = Case::passing();
    case.wordpress = Some("6.9".into());
    assert_fails(&case, "W02");
    case.wordpress = None;
    assert_eq!(case.status("W02").status, CheckStatus::Skip);
}

#[test]
fn w03_warns_on_too_many_tags() {
    let mut case = Case::passing();
    case.readme(&README.replace("Tags: release, svn", "Tags: a, b, c, d, e, f"));
    assert_fails(&case, "W03");
}

#[test]
fn w04_warns_on_mismatched_screenshots() {
    let mut case = Case::passing();
    case.assets = Some(vec!["screenshot-1.png".into(), "screenshot-3.png".into()]);
    let r = assert_fails(&case, "W04");
    assert_eq!(r.paths, ["screenshot-2", "screenshot-3"]);
}

#[test]
fn w05_warns_on_large_files() {
    let mut case = Case::passing();
    case.files.push(("assets/video.mp4".into(), 3 * 1024 * 1024));
    assert_fails(&case, "W05");
}

#[test]
fn w06_warns_on_pre_release() {
    let mut case = Case::passing();
    case.version = "1.3.0-beta1".into();
    assert_fails(&case, "W06");
}

#[test]
fn w07_warns_when_requirements_differ() {
    let mut case = Case::passing();
    case.facts.header.requires_php = Some("8.0".into());
    assert_fails(&case, "W07");
}

#[test]
fn w08_warns_on_git_ignored_files() {
    let mut case = Case::passing();
    case.gitignored = vec!["build/index.js".into()];
    assert_fails(&case, "W08");
}

#[test]
fn w09_warns_on_mixed_endings_or_bom() {
    let mut case = Case::passing();
    case.readme_text = Some(README.replacen('\n', "\r\n", 3));
    assert_fails(&case, "W09");
    case.readme_text = Some(format!("\u{feff}{README}"));
    assert_fails(&case, "W09");
}

#[test]
fn w10_warns_on_a_large_vendor_folder() {
    let mut case = Case::passing();
    case.files.push(("vendor/a.php".into(), 3 * 1024 * 1024));
    case.files.push(("vendor/b.php".into(), 3 * 1024 * 1024));
    assert_fails(&case, "W10");
}

#[test]
fn package_checks_are_v10_to_v13() {
    let case = Case::passing();
    let files: Vec<FileRef<'_>> =
        case.files.iter().map(|(rel, size)| FileRef { rel, size: *size }).collect();
    let input = VerifyInput {
        facts: &case.facts,
        version: &case.version,
        previous: None,
        server_tags: &[],
        readme_text: None,
        main_file_text: "",
        files: &files,
        zip_entries: case.zip.as_deref(),
        required_paths: &case.required,
        allow_phar: false,
        svn_version: None,
        working_copy: &WorkingCopyState::NotCreated,
        has_credentials: Some(false),
        git_dirty: None,
        current_wordpress: None,
        assets: None,
        gitignored: &[],
    };
    let ids: Vec<String> = verify::package_checks(&input).into_iter().map(|r| r.id).collect();
    assert_eq!(ids, ["V10", "V11", "V12", "V13"]);
}

#[test]
fn readme_default_is_usable_in_checks() {
    let mut case = Case::passing();
    case.facts.readme = Some(Readme::default());
    assert_fails(&case, "V04");
    assert_fails(&case, "V05");
}

#[test]
fn v17_and_w11_report_the_readme_validator() {
    let mut case = Case::passing();
    case.readme(&README.replace("License: GPLv2 or later", "License: Proprietary"));
    assert!(assert_fails(&case, "V17").message.contains("License field appears to be invalid"));
    case.readme(&README.replace("Stable tag: 1.2.0", "Stable tag: trunk"));
    let w = case.status("W11");
    assert!(w.status == CheckStatus::Fail && w.message.contains("Stable tag"), "{}", w.message);
}
