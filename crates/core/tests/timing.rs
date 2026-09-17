//! Plan §5.8: detection and verification finish in under two seconds for a
//! 5,000-file tree.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use svnpush_core::detect::{self, DetectOptions};
use svnpush_core::package::{self, Exclusions};
use svnpush_core::verify::{self, FileRef, VerifyInput, WorkingCopyState};

#[test]
fn detect_and_verify_5000_files_under_two_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("big-plugin");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("big-plugin.php"),
        "<?php\n/*\n * Plugin Name: Big\n * Version: 1.0.0\n * Text Domain: big-plugin\n */\nif ( ! defined( 'ABSPATH' ) ) { exit; }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("readme.txt"),
        "=== Big ===\nStable tag: 1.0.0\n\nShort.\n\n== Changelog ==\n\n= 1.0.0 =\n* One.\n",
    )
    .unwrap();
    for d in 0..50 {
        let sub = root.join(format!("includes/d{d}"));
        std::fs::create_dir_all(&sub).unwrap();
        for f in 0..100 {
            std::fs::write(sub.join(format!("f{f}.php")), "<?php // file\n").unwrap();
        }
    }

    let started = Instant::now();
    let facts = detect::detect(
        &root,
        DetectOptions {
            svn_url: "https://plugins.svn.wordpress.org/big-plugin",
            main_file: None,
            version_locations: &[],
        },
    )
    .unwrap();
    let listing = package::list(&root, &Exclusions::load(&root, &[]).unwrap()).unwrap();
    let rels: Vec<String> = listing.files.iter().map(|f| f.rel.clone()).collect();
    let gitignored = package::gitignored(&root, &rels);
    let files: Vec<FileRef<'_>> =
        listing.files.iter().map(|f| FileRef { rel: &f.rel, size: f.size }).collect();
    let readme_text = std::fs::read_to_string(root.join("readme.txt")).unwrap();
    let main_text = std::fs::read_to_string(root.join("big-plugin.php")).unwrap();
    let results = verify::run(&VerifyInput {
        facts: &facts,
        version: "1.0.1",
        previous: Some("1.0.0"),
        server_tags: &[],
        readme_text: Some(&readme_text),
        main_file_text: &main_text,
        files: &files,
        zip_entries: None,
        required_paths: &[],
        allow_phar: false,
        svn_version: Some("1.14.5"),
        working_copy: &WorkingCopyState::Clean,
        has_credentials: true,
        git_dirty: None,
        current_wordpress: None,
        assets: None,
        gitignored: &gitignored,
    });
    let elapsed = started.elapsed();

    assert_eq!(listing.files.len(), 5_002);
    assert_eq!(results.len(), 26);
    assert!(elapsed < Duration::from_secs(2), "took {elapsed:?} for 5,002 files");
}
