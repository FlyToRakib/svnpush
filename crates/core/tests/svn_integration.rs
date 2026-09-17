//! The release protocol against a local `file://` repository (plan §14.4).
//!
//! Run with `cargo test -p svnpush-core --features svn-integration`.
#![cfg(feature = "svn-integration")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use svnpush_core::edit::EditSet;
use svnpush_core::package::{self, Exclusions};
use svnpush_core::report::NullReporter;
use svnpush_core::secret::Secret;
use svnpush_core::svn::{self, Credentials, Delta, StatusItem, Svn, TagVerification};
use svnpush_core::tools;
use svnpush_core::verify::WorkingCopyState;
use svnpush_core::{readme, version};
use tokio_util::sync::CancellationToken;

struct Env {
    _dir: tempfile::TempDir,
    root: PathBuf,
    url: String,
    svn_bin: PathBuf,
}

fn run(program: &str, args: &[&str]) {
    let status = Command::new(program).args(args).status().unwrap();
    assert!(status.success(), "{program} {args:?}");
}

fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.starts_with('/') { format!("file://{text}") } else { format!("file:///{text}") }
}

fn env(slug: &str) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let repo = root.join("repo");
    run("svnadmin", &["create", repo.to_str().unwrap()]);
    let base = file_url(&repo);
    let url = format!("{base}/{slug}");
    run(
        "svn",
        &[
            "mkdir",
            "--parents",
            "-m",
            "Seed",
            &format!("{url}/trunk"),
            &format!("{url}/tags"),
            &format!("{url}/assets"),
            &format!("{url}/branches"),
        ],
    );
    let svn_bin = tools::find_on_path("svn").expect("svn on PATH");
    Env { _dir: dir, root, url, svn_bin }
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in walkdir::WalkDir::new(from).min_depth(1) {
        let entry = entry.unwrap();
        let target = to.join(entry.path().strip_prefix(from).unwrap());
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
        } else {
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn fixture_copy(env: &Env, name: &str) -> PathBuf {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/plugins").join(name);
    let dest = env.root.join("src").join(name);
    copy_tree(&src, &dest);
    dest
}

fn credentials() -> Credentials {
    Credentials { username: "tester".into(), password: Secret::new("not-used-by-file-urls") }
}

fn client(env: &Env) -> Svn<'static> {
    Svn::new(&env.svn_bin, &NullReporter, CancellationToken::new())
}

/// Steps 5 and 6 of the protocol: build, then mirror trunk and assets.
async fn preview(env: &Env, plugin: &Path, slug: &str, version: &str) -> (Delta, Delta) {
    let exclusions = Exclusions::load(plugin, &[]).unwrap();
    let listing = package::list(plugin, &exclusions).unwrap();
    let built = package::build(&listing, &env.root.join("builds"), slug, version).unwrap();
    let svn = client(env);
    let wc = env.root.join("wc").join(slug);
    svn.ensure_working_copy(&env.url, &wc, Some(&credentials())).await.unwrap();
    let trunk = svn
        .mirror(&svn::source_files(Path::new(&built.root), &built.files), &wc.join("trunk"))
        .await
        .unwrap();
    let assets_dir = plugin.join(".wordpress-org");
    let assets = if assets_dir.is_dir() {
        svn.mirror(&svn::folder_files(&assets_dir).unwrap(), &wc.join("assets")).await.unwrap()
    } else {
        Delta::default()
    };
    (trunk, assets)
}

/// Step 7: commit, tag, verify.
async fn publish(env: &Env, slug: &str, version: &str, main_file: &str) -> (u64, u64) {
    let svn = client(env);
    let wc = env.root.join("wc").join(slug);
    let creds = credentials();
    let trunk_rev = svn.commit(&wc, &format!("Release {version}"), &creds).await.unwrap().unwrap();
    let tag_rev = svn
        .tag(&env.url, version, Some(trunk_rev), &format!("Tag {version}"), &creds)
        .await
        .unwrap();
    let verified =
        svn.verify_tag(&env.url, version, main_file, Some(&creds), Duration::ZERO).await.unwrap();
    assert_eq!(verified, TagVerification::Verified);
    (trunk_rev, tag_rev)
}

fn bump(plugin: &Path, main_file: &str, next: &str) {
    let mut edits = EditSet::new(plugin);
    version::write_version(&mut edits, main_file, &[], next).unwrap();
    edits
        .modify::<readme::ReadmeError>(readme::README_FILE, |text| {
            let text = readme::set_header(text, "Stable tag", next)?;
            Ok(readme::upsert_changelog_entry(&text, next, "* Second release."))
        })
        .unwrap();
    edits.write().unwrap();
}

fn propget(path: &Path) -> String {
    let out = Command::new("svn").args(["propget", "svn:mime-type"]).arg(path).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[tokio::test]
async fn first_and_second_release() {
    let env = env("minimal");
    let plugin = fixture_copy(&env, "minimal");
    std::fs::create_dir_all(plugin.join("assets/img")).unwrap();
    std::fs::write(plugin.join("assets/img/logo.png"), [0x89, b'P', b'N', b'G', 0, 1, 2]).unwrap();
    std::fs::create_dir_all(plugin.join(".wordpress-org")).unwrap();
    std::fs::write(plugin.join(".wordpress-org/icon-128x128.png"), [0x89, b'P', b'N', b'G', 9])
        .unwrap();

    let (trunk, assets) = preview(&env, &plugin, "minimal", "1.0.0").await;
    assert_eq!(trunk.added, ["assets/img/logo.png", "minimal.php", "readme.txt"]);
    assert!(trunk.modified.is_empty() && trunk.deleted.is_empty());
    assert_eq!(assets.added, ["icon-128x128.png"]);
    let wc = env.root.join("wc/minimal");
    assert_eq!(propget(&wc.join("trunk/assets/img/logo.png")), "image/png");
    assert_eq!(propget(&wc.join("trunk/readme.txt")), "");

    let (trunk_rev, tag_rev) = publish(&env, "minimal", "1.0.0", "minimal.php").await;
    assert!(tag_rev > trunk_rev);
    let svn = client(&env);
    assert_eq!(svn.list_tags(&env.url, None).await.unwrap(), ["1.0.0"]);
    assert_eq!(svn.working_copy_state(&wc, None).await.unwrap(), WorkingCopyState::Clean);

    bump(&plugin, "minimal.php", "1.0.1");
    std::fs::remove_file(plugin.join("assets/img/logo.png")).unwrap();
    std::fs::create_dir_all(plugin.join("includes")).unwrap();
    std::fs::write(plugin.join("includes/new.php"), "<?php\n").unwrap();

    let (trunk, assets) = preview(&env, &plugin, "minimal", "1.0.1").await;
    assert_eq!(trunk.added, ["includes/new.php"]);
    assert_eq!(trunk.modified, ["minimal.php", "readme.txt"]);
    assert_eq!(trunk.deleted, ["assets/", "assets/img/logo.png"]);
    assert!(assets.is_empty());

    publish(&env, "minimal", "1.0.1", "minimal.php").await;
    let listed = svn.list(&format!("{}/trunk", env.url), None).await.unwrap();
    assert_eq!(listed, ["includes/", "minimal.php", "readme.txt"]);
    let tagged = svn.cat(&format!("{}/tags/1.0.1/minimal.php", env.url), None).await.unwrap();
    assert!(tagged.contains("Version:           1.0.1"));
    assert_eq!(svn.list_tags(&env.url, None).await.unwrap(), ["1.0.0", "1.0.1"]);
}

#[tokio::test]
async fn an_existing_tag_is_found_before_anything_is_written() {
    let env = env("minimal");
    let plugin = fixture_copy(&env, "minimal");
    preview(&env, &plugin, "minimal", "1.0.0").await;
    publish(&env, "minimal", "1.0.0", "minimal.php").await;

    let tags = client(&env).list_tags(&env.url, None).await.unwrap();
    assert!(tags.contains(&"1.0.0".to_owned()));
    let newest = version::newest(tags.iter().map(String::as_str)).unwrap();
    assert_eq!(newest.as_str(), "1.0.0");

    let err =
        client(&env).tag(&env.url, "1.0.0", None, "Tag again", &credentials()).await.unwrap_err();
    assert!(matches!(err, svn::SvnError::TagExists { ref version } if version == "1.0.0"), "{err}");
    let nested = client(&env).list(&format!("{}/tags/1.0.0/", env.url), None).await.unwrap();
    assert!(!nested.contains(&"trunk/".to_owned()));
}

#[tokio::test]
async fn trunk_committed_then_resume_creates_only_the_tag() {
    let env = env("minimal");
    let plugin = fixture_copy(&env, "minimal");
    preview(&env, &plugin, "minimal", "1.0.0").await;
    let svn = client(&env);
    let wc = env.root.join("wc/minimal");
    let trunk_rev = svn.commit(&wc, "Release 1.0.0", &credentials()).await.unwrap().unwrap();
    assert!(svn.list_tags(&env.url, None).await.unwrap().is_empty());

    // Resume: retry only the copy, pinned to the recorded trunk revision.
    let tag_rev =
        svn.tag(&env.url, "1.0.0", Some(trunk_rev), "Tag 1.0.0", &credentials()).await.unwrap();
    assert_eq!(tag_rev, trunk_rev + 1);
    let verified =
        svn.verify_tag(&env.url, "1.0.0", "minimal.php", None, Duration::ZERO).await.unwrap();
    assert_eq!(verified, TagVerification::Verified);
}

#[tokio::test]
async fn dry_run_revert_leaves_the_working_copy_clean() {
    let env = env("minimal");
    let plugin = fixture_copy(&env, "minimal");
    let (trunk, _) = preview(&env, &plugin, "minimal", "1.0.0").await;
    assert_eq!(trunk.added.len(), 2);

    let svn = client(&env);
    let wc = env.root.join("wc/minimal");
    assert!(svn.status(&wc).await.unwrap().iter().any(|e| e.item == StatusItem::Added));
    svn.revert(&wc).await.unwrap();
    assert!(svn.status(&wc).await.unwrap().is_empty());
    assert!(!wc.join("trunk/minimal.php").exists());
    assert!(svn.commit(&wc, "Nothing", &credentials()).await.unwrap().is_none());
}

#[tokio::test]
async fn out_of_date_and_reset() {
    let env = env("minimal");
    let plugin = fixture_copy(&env, "minimal");
    preview(&env, &plugin, "minimal", "1.0.0").await;
    publish(&env, "minimal", "1.0.0", "minimal.php").await;

    // Someone else commits to trunk.
    let other = env.root.join("other");
    run("svn", &["checkout", &format!("{}/trunk", env.url), other.to_str().unwrap(), "-q"]);
    std::fs::write(other.join("extra.php"), "<?php\n").unwrap();
    run("svn", &["add", other.join("extra.php").to_str().unwrap(), "-q"]);
    run("svn", &["commit", other.to_str().unwrap(), "-m", "Elsewhere", "-q"]);

    let svn = client(&env);
    let wc = env.root.join("wc/minimal");
    let state = svn.working_copy_state(&wc, None).await.unwrap();
    assert!(
        matches!(state, WorkingCopyState::OutOfDate(ref paths) if !paths.is_empty()),
        "{state:?}"
    );

    std::fs::remove_dir_all(wc.join(".svn")).unwrap();
    svn.ensure_working_copy(&env.url, &wc, None).await.unwrap();
    assert!(wc.join("trunk/extra.php").is_file());
    assert_eq!(svn.working_copy_state(&wc, None).await.unwrap(), WorkingCopyState::Clean);
}

#[tokio::test]
async fn case_only_rename_is_delete_plus_add() {
    let env = env("minimal");
    let plugin = fixture_copy(&env, "minimal");
    std::fs::write(plugin.join("Helper.php"), "<?php\n").unwrap();
    preview(&env, &plugin, "minimal", "1.0.0").await;
    publish(&env, "minimal", "1.0.0", "minimal.php").await;

    std::fs::rename(plugin.join("Helper.php"), plugin.join("helper-tmp.php")).unwrap();
    std::fs::rename(plugin.join("helper-tmp.php"), plugin.join("helper.php")).unwrap();
    bump(&plugin, "minimal.php", "1.0.1");
    let (trunk, _) = preview(&env, &plugin, "minimal", "1.0.1").await;
    assert_eq!(trunk.added, ["helper.php"]);
    assert_eq!(trunk.deleted, ["Helper.php"]);
    publish(&env, "minimal", "1.0.1", "minimal.php").await;
    let listed = client(&env).list(&format!("{}/trunk", env.url), None).await.unwrap();
    assert_eq!(listed, ["helper.php", "minimal.php", "readme.txt"]);
}

#[tokio::test]
async fn missing_repository_is_not_found() {
    let env = env("minimal");
    let err = client(&env).list_tags(&format!("{}-missing", env.url), None).await.unwrap_err();
    assert!(matches!(err, svn::SvnError::NotFound { .. }), "{err}");
}
