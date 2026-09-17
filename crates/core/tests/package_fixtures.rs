//! Fixture plugins produce spec-correct packages and zips.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use svnpush_core::package::{self, ExclusionSource, Exclusions};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/plugins").join(name)
}

fn rels(listing: &package::Listing) -> Vec<&str> {
    listing.files.iter().map(|f| f.rel.as_str()).collect()
}

fn build(name: &str, version: &str, builds: &Path) -> package::Package {
    let root = fixture(name);
    let exclusions = Exclusions::load(&root, &[]).unwrap();
    let listing = package::list(&root, &exclusions).unwrap();
    package::build(&listing, builds, name, version).unwrap()
}

#[test]
fn minimal_uses_the_default_list() {
    let root = fixture("minimal");
    let listing = package::list(&root, &Exclusions::load(&root, &[]).unwrap()).unwrap();
    assert_eq!(listing.exclusion_source, ExclusionSource::Defaults);
    assert_eq!(rels(&listing), ["minimal.php", "readme.txt"]);
}

#[test]
fn composer_based_ships_vendor_but_not_composer_json() {
    let root = fixture("composer-based");
    let listing = package::list(&root, &Exclusions::load(&root, &[]).unwrap()).unwrap();
    assert_eq!(listing.exclusion_source, ExclusionSource::Distignore);
    assert_eq!(
        rels(&listing),
        [
            "composer-based.php",
            "readme.txt",
            "vendor/autoload.php",
            "vendor/composer/autoload_real.php",
        ]
    );
}

#[test]
fn block_with_build_ships_build_not_src() {
    let root = fixture("block-with-build");
    let listing = package::list(&root, &Exclusions::load(&root, &[]).unwrap()).unwrap();
    assert_eq!(
        rels(&listing),
        ["block-with-build.php", "build/block.json", "build/index.js", "readme.txt",]
    );
}

#[test]
fn zip_is_deterministic_and_rooted_at_the_slug() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let a = build("block-with-build", "0.3.0", first.path());
    let b = build("block-with-build", "0.3.0", second.path());
    assert_eq!(a.sha256, b.sha256);
    assert_eq!(a.files, b.files);
    assert_eq!(a.total_size, a.files.iter().map(|f| f.size).sum::<u64>());

    let zip_path = Path::new(&a.zip_path);
    assert!(zip_path.ends_with("block-with-build/0.3.0/block-with-build-0.3.0.zip"));
    let entries = package::zip_entry_names(zip_path).unwrap();
    assert_eq!(
        entries,
        [
            "block-with-build/",
            "block-with-build/block-with-build.php",
            "block-with-build/build/",
            "block-with-build/build/block.json",
            "block-with-build/build/index.js",
            "block-with-build/readme.txt",
        ]
    );

    let checksum = std::fs::read_to_string(zip_path.with_extension("zip.sha256")).unwrap();
    assert_eq!(checksum, format!("{}  block-with-build-0.3.0.zip\n", a.sha256));
    assert_eq!(package::sha256_file(zip_path).unwrap(), a.sha256);

    let staged = Path::new(&a.root);
    assert!(staged.ends_with("block-with-build/0.3.0/block-with-build"));
    for file in &a.files {
        assert_eq!(package::hash_file(&staged.join(&file.rel)).unwrap(), file.hash);
    }
}

#[test]
fn prune_keeps_the_newest_three_builds() {
    let builds = tempfile::tempdir().unwrap();
    for version in ["1.0.0", "1.0.1", "1.0.2", "1.0.3"] {
        build("minimal", version, builds.path());
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    package::prune_builds(builds.path(), "minimal").unwrap();
    let mut left: Vec<String> = std::fs::read_dir(builds.path().join("minimal"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["1.0.1", "1.0.2", "1.0.3"]);
}

// Windows itself refuses to create such a name, so this runs where it can exist.
#[cfg(unix)]
#[test]
fn invalid_windows_names_are_reported_with_the_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("plugin.php"), "<?php").unwrap();
    std::fs::create_dir_all(dir.path().join("docs")).unwrap();
    std::fs::write(dir.path().join("docs/a:b.txt"), "x").unwrap();
    let err = package::list(dir.path(), &Exclusions::load(dir.path(), &[]).unwrap()).unwrap_err();
    assert!(
        matches!(err, package::PackageError::InvalidName { ref path, .. } if path == "docs/a:b.txt")
    );
}
