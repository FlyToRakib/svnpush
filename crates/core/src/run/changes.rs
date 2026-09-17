//! Step 2.1: what changed since the previous release.
//!
//! With git and a tag, the working tree is compared with the last tag (so
//! uncommitted edits count, because they are what will ship). Otherwise the
//! package root is compared file by file with the trunk working copy.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::detect::git::{Git, GitFacts};
use crate::package::{self, Exclusions, Listing, PackageError};
use crate::tools::ProcessError;

/// How the change set was computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ChangeSource {
    /// `git diff <last tag>` against the working tree.
    Git,
    /// The package root compared with the trunk working copy.
    Trunk,
    /// Trunk is empty: everything is new.
    FirstRelease,
    /// The package root does not exist yet (a pre-build hook creates it).
    Unknown,
}

/// A changed path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ChangeKind {
    /// New.
    Added,
    /// Changed.
    Modified,
    /// Removed.
    Deleted,
}

/// One changed path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChangedFile {
    /// Path, relative to the project folder (git) or package root (trunk).
    pub path: String,
    /// What happened to it.
    pub kind: ChangeKind,
}

/// Everything that changed since the previous release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChangeSet {
    /// How it was computed.
    pub source: ChangeSource,
    /// The git tag compared against, for `Git`.
    pub base: Option<String>,
    /// Changed paths, sorted.
    pub files: Vec<ChangedFile>,
    /// Commit subjects since the base, newest first.
    pub commits: Vec<String>,
}

impl ChangeSet {
    /// Whether there is provably nothing to release.
    pub fn is_empty(&self) -> bool {
        self.source != ChangeSource::Unknown && self.files.is_empty()
    }
}

fn kind_from_status(status: &str) -> Option<ChangeKind> {
    match status.chars().next()? {
        'A' | 'C' => Some(ChangeKind::Added),
        'D' => Some(ChangeKind::Deleted),
        'M' | 'T' | 'R' => Some(ChangeKind::Modified),
        _ => None,
    }
}

/// Parses `git diff --name-status` output.
pub fn parse_name_status(output: &str) -> Vec<ChangedFile> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let status = fields.next()?;
            let kind = kind_from_status(status)?;
            let path = fields.next_back()?.to_owned();
            Some(ChangedFile { path, kind })
        })
        .collect()
}

/// The change set from git, or `None` when there is no tag to compare with.
pub async fn from_git(git: &Git<'_>, facts: &GitFacts) -> Result<Option<ChangeSet>, ProcessError> {
    let Some(tag) = &facts.last_tag else { return Ok(None) };
    // `--relative` keeps paths relative to the plugin folder when it sits inside a larger repository.
    let Some(output) =
        git.output(&["diff", "--relative", "--name-status", "-M", tag, "--", "."]).await?
    else {
        return Ok(None);
    };
    let mut files = parse_name_status(&output);
    if let Some(untracked) =
        git.output(&["ls-files", "--others", "--exclude-standard", "--", "."]).await?
    {
        files.extend(
            untracked
                .lines()
                .filter(|l| !l.is_empty())
                .map(|path| ChangedFile { path: path.to_owned(), kind: ChangeKind::Added }),
        );
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    Ok(Some(ChangeSet {
        source: ChangeSource::Git,
        base: Some(tag.clone()),
        files,
        commits: facts.commits.clone(),
    }))
}

/// The change set from comparing the package root with `trunk_dir`.
pub fn from_trunk(
    package_root: &Path,
    trunk_dir: &Path,
    commits: Vec<String>,
) -> Result<ChangeSet, PackageError> {
    if !package_root.is_dir() {
        return Ok(ChangeSet {
            source: ChangeSource::Unknown,
            base: None,
            files: Vec::new(),
            commits,
        });
    }
    let listing: Listing = package::list(package_root, &Exclusions::load(package_root, &[])?)?;
    let trunk: BTreeMap<String, String> = if trunk_dir.is_dir() {
        package::list(trunk_dir, &Exclusions::hard_only(trunk_dir)?)?
            .files
            .into_iter()
            .map(|f| Ok((f.rel, package::hash_file(Path::new(&f.abs))?)))
            .collect::<Result<_, PackageError>>()?
    } else {
        BTreeMap::new()
    };

    let source = if trunk.is_empty() { ChangeSource::FirstRelease } else { ChangeSource::Trunk };
    let mut files = Vec::new();
    for file in &listing.files {
        match trunk.get(&file.rel) {
            None => files.push(ChangedFile { path: file.rel.clone(), kind: ChangeKind::Added }),
            Some(hash) if *hash != package::hash_file(Path::new(&file.abs))? => {
                files.push(ChangedFile { path: file.rel.clone(), kind: ChangeKind::Modified });
            }
            Some(_) => {}
        }
    }
    for rel in trunk.keys() {
        if !listing.files.iter().any(|f| &f.rel == rel) {
            files.push(ChangedFile { path: rel.clone(), kind: ChangeKind::Deleted });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(ChangeSet { source, base: None, files, commits })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_status_including_renames() {
        let out = "M\tplugin.php\nA\tinc/new.php\nD\told.php\nR100\tfrom.php\tto.php\n";
        let files = parse_name_status(out);
        assert_eq!(files.len(), 4);
        assert_eq!(files[3], ChangedFile { path: "to.php".into(), kind: ChangeKind::Modified });
        assert_eq!(files[2].kind, ChangeKind::Deleted);
    }

    #[test]
    fn trunk_comparison_finds_all_three_kinds() {
        let src = tempfile::tempdir().unwrap();
        let trunk = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("same.php"), "same").unwrap();
        std::fs::write(src.path().join("changed.php"), "new").unwrap();
        std::fs::write(src.path().join("added.php"), "a").unwrap();
        std::fs::write(trunk.path().join("same.php"), "same").unwrap();
        std::fs::write(trunk.path().join("changed.php"), "old").unwrap();
        std::fs::write(trunk.path().join("gone.php"), "g").unwrap();
        let set = from_trunk(src.path(), trunk.path(), Vec::new()).unwrap();
        assert_eq!(set.source, ChangeSource::Trunk);
        let summary: Vec<(&str, ChangeKind)> =
            set.files.iter().map(|f| (f.path.as_str(), f.kind)).collect();
        assert_eq!(
            summary,
            [
                ("added.php", ChangeKind::Added),
                ("changed.php", ChangeKind::Modified),
                ("gone.php", ChangeKind::Deleted)
            ]
        );
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(["-c", "user.email=test@example.org", "-c", "user.name=Test"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(status.status.success(), "git {args:?}");
    }

    #[tokio::test]
    async fn a_plugin_inside_a_larger_repository_gets_folder_relative_paths() {
        let repo = tempfile::tempdir().unwrap();
        let plugin = repo.path().join("plugins/demo");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(plugin.join("demo.php"), "<?php\n").unwrap();
        git(repo.path(), &["init", "-q"]);
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-q", "-m", "First"]);
        git(repo.path(), &["tag", "v1.0.0"]);
        std::fs::write(plugin.join("demo.php"), "<?php\n// changed\n").unwrap();
        std::fs::write(plugin.join("new.php"), "<?php\n").unwrap();

        let bin =
            crate::tools::discover_git(None).await.path.map(std::path::PathBuf::from).unwrap();
        let cancel = tokio_util::sync::CancellationToken::new();
        let runner = Git::new(&bin, &plugin, &crate::report::NullReporter, &cancel);
        let facts = runner.facts().await.unwrap().unwrap();
        assert_eq!(facts.dirty, ["demo.php", "new.php"]);
        let set = from_git(&runner, &facts).await.unwrap().unwrap();
        let paths: Vec<(&str, ChangeKind)> =
            set.files.iter().map(|f| (f.path.as_str(), f.kind)).collect();
        assert_eq!(paths, [("demo.php", ChangeKind::Modified), ("new.php", ChangeKind::Added)]);

        let filter = crate::run::material::Filter::new(&[]);
        let material =
            crate::run::material::from_git(&runner, &plugin, &set, &filter).await.unwrap();
        let diffed: Vec<&str> = material.diffs.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(diffed, ["demo.php", "new.php"]);
    }

    #[test]
    fn identical_trees_have_nothing_to_release() {
        let src = tempfile::tempdir().unwrap();
        let trunk = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("a.php"), "a").unwrap();
        std::fs::write(trunk.path().join("a.php"), "a").unwrap();
        assert!(from_trunk(src.path(), trunk.path(), Vec::new()).unwrap().is_empty());
        let missing = from_trunk(&src.path().join("dist"), trunk.path(), Vec::new()).unwrap();
        assert_eq!(missing.source, ChangeSource::Unknown);
        assert!(!missing.is_empty());
    }
}
