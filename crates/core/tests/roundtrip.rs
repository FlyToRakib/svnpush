//! Property tests: version write-then-read round trips, and readme edits
//! that keep every byte outside the edited region.

use proptest::prelude::*;
use svnpush_core::detect::header;
use svnpush_core::readme;
use svnpush_core::version::Version;

fn version_strategy() -> impl Strategy<Value = String> {
    (
        0_u32..200,
        0_u32..200,
        proptest::option::of(0_u32..200),
        proptest::option::of("[a-z]{1,6}[0-9]{0,2}"),
    )
        .prop_map(|(major, minor, patch, pre)| {
            let mut v = format!("{major}.{minor}");
            if let Some(p) = patch {
                v = format!("{v}.{p}");
            }
            if let Some(p) = pre {
                v.push('-');
                v.push_str(&p);
            }
            v
        })
}

fn eol_strategy() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("\n"), Just("\r\n")]
}

fn plugin_file(version: &str, eol: &str) -> String {
    let version_line = format!(" * Version:     {version}");
    [
        "<?php",
        "/**",
        " * Plugin Name: Prop",
        &version_line,
        " */",
        "",
    ]
    .join(eol)
}

fn readme_file(version: &str, eol: &str, intro: &str) -> String {
    [
        "=== Prop ===",
        "Contributors: someone",
        &format!("Stable tag: {version}"),
        "",
        "Short.",
        "",
        "== Changelog ==",
        "",
        intro,
        "",
        &format!("= {version} ="),
        "* Existing.",
        "",
        "== Other ==",
        "Tail text.",
        "",
    ]
    .join(eol)
}

proptest! {
    #[test]
    fn every_generated_version_parses(v in version_strategy()) {
        let parsed = Version::parse(&v).unwrap();
        prop_assert_eq!(parsed.as_str(), v.as_str());
    }

    #[test]
    fn header_version_write_then_read(old in version_strategy(), new in version_strategy(), eol in eol_strategy()) {
        let text = plugin_file(&old, eol);
        let written = header::set(&text, "Version", &new).unwrap();
        let parsed = header::parse(&written);
        prop_assert_eq!(parsed.version.as_deref(), Some(new.as_str()));
        prop_assert_eq!(written.replacen(&new, &old, 1), text);
    }

    #[test]
    fn stable_tag_write_then_read(old in version_strategy(), new in version_strategy(), eol in eol_strategy()) {
        let text = readme_file(&old, eol, "Intro.");
        let written = readme::set_header(&text, "Stable tag", &new).unwrap();
        let parsed = readme::parse(&written);
        prop_assert_eq!(parsed.header("Stable tag").map(|h| h.value.as_str()), Some(new.as_str()));
    }

    #[test]
    fn changelog_insert_keeps_every_other_byte(
        old in "1\\.[0-9]\\.[0-9]",
        body in "[*] [A-Za-z ,.]{1,40}",
        intro in "[A-Za-z ,.]{0,30}",
        eol in eol_strategy(),
    ) {
        let text = readme_file(&old, eol, &intro);
        let new_version = "9.9.9";
        let written = readme::upsert_changelog_entry(&text, new_version, &body);
        let parsed = readme::parse(&written);
        prop_assert_eq!(parsed.changelog[0].version.as_deref(), Some(new_version));
        prop_assert_eq!(parsed.changelog[0].body.as_str(), body.trim());

        let inserted = format!("= {new_version} ={eol}{}{eol}{eol}", body.trim());
        prop_assert_eq!(written.replacen(&inserted, "", 1), text);
    }

    #[test]
    fn upgrade_notice_insert_keeps_the_rest_parseable(
        notice in "[A-Za-z ,.]{1,60}",
        eol in eol_strategy(),
    ) {
        let text = readme_file("1.0.0", eol, "Intro.");
        let written = readme::upsert_upgrade_notice(&text, "1.0.1", &notice);
        let parsed = readme::parse(&written);
        if notice.trim().is_empty() {
            prop_assert_eq!(written, text);
        } else {
            prop_assert_eq!(parsed.upgrade_notice[0].body.as_str(), notice.trim());
            prop_assert_eq!(parsed.changelog[0].version.as_deref(), Some("1.0.0"));
            prop_assert!(parsed.has_section("Other"));
        }
    }
}
