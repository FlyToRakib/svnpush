//! Step 2.1 for the AI: the diff stat and the per-file diffs a prompt is
//! written from, prioritised and filtered (plan §5.2, §9.8).

use std::collections::BTreeMap;
use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::ai::prompts::{DIFF_BUDGET_CHARS, FILE_DIFF_BUDGET_CHARS, truncate_chars};
use crate::detect::git::Git;
use crate::edit;
use crate::readme::README_FILE;
use crate::text::plural;
use crate::tools::ProcessError;
use crate::verify;

use super::changes::{ChangeKind, ChangeSet, ChangeSource};
use super::steps::unified_diff;

/// At most this many files are summarised one by one when the diff is too large.
pub const MAX_SUMMARISED_FILES: usize = 25;
/// Larger files are listed in the stat but not read or diffed: a generated or
/// bundled file that size says nothing a release note needs.
pub const MAX_DIFF_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// What the AI sees of the changes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Material {
    /// One line per changed file.
    pub stat: String,
    /// Per-file diffs, most important first.
    pub diffs: Vec<(String, String)>,
    /// Changed files kept out of the prompt.
    pub withheld: Vec<String>,
}

impl Material {
    /// Characters of diff in total.
    pub fn diff_chars(&self) -> usize {
        self.diffs.iter().map(|(_, d)| d.chars().count()).sum()
    }

    /// Whether the diff must be summarised per file first.
    pub fn needs_summaries(&self) -> bool {
        self.diff_chars() > DIFF_BUDGET_CHARS
    }

    /// Every diff joined, for a prompt that fits the budget.
    pub fn joined(&self) -> String {
        if self.diffs.is_empty() {
            return "No text diff is available for these files.".to_owned();
        }
        self.diffs.iter().map(|(_, d)| d.as_str()).collect::<Vec<_>>().join("\n")
    }
}

fn has_extension(path: &str, extensions: &[&str]) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|ext| extensions.iter().any(|wanted| ext.eq_ignore_ascii_case(wanted)))
}

/// Readme and PHP first, then scripts and styles, then everything else.
pub fn priority(path: &str) -> u8 {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.eq_ignore_ascii_case(README_FILE) || has_extension(path, &["php"]) {
        0
    } else if has_extension(path, &["js", "jsx", "ts", "tsx", "css", "scss"]) {
        1
    } else {
        2
    }
}

/// The project's AI exclude patterns.
pub struct Filter {
    rules: Gitignore,
}

impl Filter {
    /// Builds the matcher; invalid patterns are ignored.
    pub fn new(patterns: &[String]) -> Self {
        let mut builder = GitignoreBuilder::new("/");
        for pattern in patterns {
            // A malformed pattern excludes nothing rather than failing the draft.
            let _ = builder.add_line(None, pattern);
        }
        Self { rules: builder.build().unwrap_or_else(|_| Gitignore::empty()) }
    }

    /// Whether `path` stays out of the prompt. Files that look like secrets
    /// (the list check V11 blocks) never reach one, whatever the settings say.
    pub fn excludes(&self, path: &str) -> bool {
        verify::looks_like_secret(path)
            || self.rules.matched_path_or_any_parents(Path::new("/").join(path), false).is_ignore()
    }
}

/// Splits `git diff` output into one diff per file.
pub fn split_git_diff(output: &str) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let mut current: Option<(String, String)> = None;
    for line in output.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some((path, diff)) = current.take() {
                files.insert(path, diff);
            }
            let path = rest.trim_end().rsplit_once(" b/").map_or("", |(_, b)| b).to_owned();
            current = Some((path, String::new()));
        }
        if let Some((_, diff)) = current.as_mut() {
            diff.push_str(line);
        }
    }
    if let Some((path, diff)) = current {
        files.insert(path, diff);
    }
    files
}

/// The path a rename or copy diff came from, read from its header.
fn renamed_from(diff: &str) -> Option<&str> {
    diff.lines().take_while(|line| !line.starts_with("--- ") && !line.starts_with("@@")).find_map(
        |line| line.strip_prefix("rename from ").or_else(|| line.strip_prefix("copy from ")),
    )
}

/// A text file, unless it is binary or too large to diff.
fn text_of(root: &Path, rel: &str) -> Option<String> {
    let size = std::fs::metadata(root.join(rel)).ok()?.len();
    if size > MAX_DIFF_FILE_BYTES {
        return None;
    }
    edit::read_text(root, rel).ok().filter(|t| !t.contains('\0'))
}

fn stat_line(kind: ChangeKind, path: &str) -> String {
    let mark = match kind {
        ChangeKind::Added => "added",
        ChangeKind::Modified => "modified",
        ChangeKind::Deleted => "deleted",
    };
    format!("{mark}: {path}")
}

/// Sorts by priority, filters, and cuts each diff to the per-file budget.
fn finish(changes: &ChangeSet, filter: &Filter, mut diffs: BTreeMap<String, String>) -> Material {
    let mut files: Vec<_> = changes.files.iter().collect();
    files.sort_by_key(|f| (priority(&f.path), f.path.clone()));
    let mut material = Material::default();
    let mut stat = Vec::new();
    for file in files {
        if filter.excludes(&file.path) {
            material.withheld.push(file.path.clone());
            continue;
        }
        stat.push(stat_line(file.kind, &file.path));
        if let Some(diff) = diffs.remove(&file.path).filter(|d| !d.trim().is_empty()) {
            material.diffs.push((file.path.clone(), truncate_chars(&diff, FILE_DIFF_BUDGET_CHARS)));
        }
    }
    if !material.withheld.is_empty() {
        stat.push(format!(
            "({} withheld from the AI by the exclude patterns)",
            plural(material.withheld.len(), "file", "files")
        ));
    }
    material.stat = stat.join("\n");
    material
}

/// The material from git: one `git diff` against the base tag, plus the
/// content of untracked new files.
pub async fn from_git(
    git: &Git<'_>,
    project_root: &Path,
    changes: &ChangeSet,
    filter: &Filter,
) -> Result<Material, ProcessError> {
    let base = changes.base.clone().unwrap_or_default();
    // Without core.quotePath=false git writes a non-ASCII name as "b/caf\303\251.php",
    // which never matches the change set's path, and that file's diff is lost.
    let output = git
        .output(&[
            "-c",
            "core.quotePath=false",
            "diff",
            "--relative",
            "-M",
            "--no-color",
            &base,
            "--",
            ".",
        ])
        .await?
        .unwrap_or_default();
    let mut diffs = split_git_diff(&output);
    // A rename's diff holds the old file's lines. When the old file is
    // withheld (a quoted name cannot be checked), send only the new file's text.
    for (path, diff) in &mut diffs {
        if renamed_from(diff).is_some_and(|from| from.starts_with('"') || filter.excludes(from)) {
            *diff = text_of(project_root, path)
                .map(|text| unified_diff(path, "", &text))
                .unwrap_or_default();
        }
    }
    for file in changes.files.iter().filter(|f| f.kind == ChangeKind::Added) {
        if !diffs.contains_key(&file.path)
            && !filter.excludes(&file.path)
            && let Some(text) = text_of(project_root, &file.path)
        {
            diffs.insert(file.path.clone(), unified_diff(&file.path, "", &text));
        }
    }
    Ok(finish(changes, filter, diffs))
}

/// The material from comparing the package root with the trunk working copy.
pub fn from_trunk(
    package_root: &Path,
    trunk: &Path,
    changes: &ChangeSet,
    filter: &Filter,
) -> Material {
    let mut diffs = BTreeMap::new();
    if changes.source != ChangeSource::Unknown {
        for file in changes.files.iter().filter(|f| !filter.excludes(&f.path)) {
            let before = if file.kind == ChangeKind::Added {
                Some(String::new())
            } else {
                text_of(trunk, &file.path)
            };
            let after = if file.kind == ChangeKind::Deleted {
                Some(String::new())
            } else {
                text_of(package_root, &file.path)
            };
            if let (Some(before), Some(after)) = (before, after) {
                diffs.insert(file.path.clone(), unified_diff(&file.path, &before, &after));
            }
        }
    }
    finish(changes, filter, diffs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::changes::ChangedFile;

    fn set(files: &[(&str, ChangeKind)]) -> ChangeSet {
        ChangeSet {
            source: ChangeSource::Trunk,
            base: None,
            files: files.iter().map(|(p, k)| ChangedFile { path: (*p).into(), kind: *k }).collect(),
            commits: vec![],
        }
    }

    #[test]
    fn secrets_are_always_withheld_and_patterns_apply() {
        let filter = Filter::new(&crate::project::default_ai_exclude_patterns());
        for secret in [
            ".env",
            "config/.env.local",
            "certs/site.pem",
            "private.key",
            "wp-config.php",
            ".ssh/id_rsa",
            "deploy/id_ed25519",
            "signing.p12",
            "certs/site.PFX",
        ] {
            assert!(filter.excludes(secret), "{secret}");
        }
        for excluded in
            ["vendor/autoload.php", "assets/app.min.js", "composer.lock", "inc/vendor/x.php"]
        {
            assert!(filter.excludes(excluded), "{excluded}");
        }
        for kept in ["plugin.php", "readme.txt", "assets/app.js", "inc/vendors.php"] {
            assert!(!filter.excludes(kept), "{kept}");
        }
        assert!(!Filter::new(&[]).excludes("vendor/a.php"));
    }

    #[test]
    fn git_diff_is_split_per_file() {
        let out = "diff --git a/a.php b/a.php\n--- a/a.php\n+++ b/a.php\n@@ -1 +1 @@\n-x\n+y\ndiff --git a/inc/b.js b/inc/b.js\n+z\n";
        let files = split_git_diff(out);
        assert_eq!(files.len(), 2);
        assert!(files["a.php"].starts_with("diff --git a/a.php"));
        assert!(files["a.php"].ends_with("+y\n"));
        assert_eq!(files["inc/b.js"], "diff --git a/inc/b.js b/inc/b.js\n+z\n");

        let renamed = "diff --git a/old.php b/new.php\nsimilarity index 90%\nrename from old.php\nrename to new.php\n--- a/old.php\n+++ b/new.php\n-rename from x\n";
        assert_eq!(renamed_from(renamed), Some("old.php"));
        assert_eq!(renamed_from(&files["a.php"]), None);
    }

    #[test]
    fn trunk_material_is_prioritised_filtered_and_budgeted() {
        let src = tempfile::tempdir().unwrap();
        let trunk = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(src.path().join("vendor")).unwrap();
        std::fs::write(src.path().join("style.css"), "a {}\n").unwrap();
        std::fs::write(src.path().join("plugin.php"), "<?php\n// new\n").unwrap();
        std::fs::write(trunk.path().join("plugin.php"), "<?php\n").unwrap();
        std::fs::write(src.path().join("vendor/lib.php"), "<?php\n").unwrap();
        std::fs::write(src.path().join(".env"), "SECRET=1\n").unwrap();
        std::fs::write(trunk.path().join("old.txt"), "bye\n").unwrap();
        let changes = set(&[
            (".env", ChangeKind::Added),
            ("old.txt", ChangeKind::Deleted),
            ("plugin.php", ChangeKind::Modified),
            ("style.css", ChangeKind::Added),
            ("vendor/lib.php", ChangeKind::Added),
        ]);
        let filter = Filter::new(&crate::project::default_ai_exclude_patterns());
        let material = from_trunk(src.path(), trunk.path(), &changes, &filter);
        let order: Vec<&str> = material.diffs.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(order, ["plugin.php", "style.css", "old.txt"]);
        assert!(material.diffs[0].1.contains("+// new"));
        assert_eq!(material.withheld, ["vendor/lib.php", ".env"], "priority order");
        assert!(!material.joined().contains("SECRET"));
        assert!(
            material.stat.starts_with("modified: plugin.php\nadded: style.css\ndeleted: old.txt")
        );
        assert!(!material.needs_summaries());

        let big = Material {
            diffs: vec![("a".into(), "x".repeat(DIFF_BUDGET_CHARS + 1))],
            ..Material::default()
        };
        assert!(big.needs_summaries());
    }

    #[test]
    fn oversized_files_are_listed_but_not_diffed() {
        let src = tempfile::tempdir().unwrap();
        let trunk = tempfile::tempdir().unwrap();
        let size = usize::try_from(MAX_DIFF_FILE_BYTES).unwrap() + 1;
        std::fs::write(src.path().join("bundle.js"), "x".repeat(size)).unwrap();
        std::fs::write(src.path().join("plugin.php"), "<?php\n").unwrap();
        let changes = set(&[("bundle.js", ChangeKind::Added), ("plugin.php", ChangeKind::Added)]);
        let material = from_trunk(src.path(), trunk.path(), &changes, &Filter::new(&[]));
        let diffed: Vec<&str> = material.diffs.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(diffed, ["plugin.php"]);
        assert!(material.stat.contains("added: bundle.js"));
    }

    fn git(repo: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(["-c", "user.email=test@example.org", "-c", "user.name=Test"])
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    }

    /// A repository holding `files`, tagged `v1.0.0`.
    fn tagged_repo(files: &[(&str, &str)]) -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        for (name, text) in files {
            std::fs::write(repo.path().join(name), text).unwrap();
        }
        git(repo.path(), &["init", "-q"]);
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-q", "-m", "First"]);
        git(repo.path(), &["tag", "v1.0.0"]);
        repo
    }

    /// The material for `path` since `v1.0.0`, with the default exclude patterns.
    async fn material_since_tag(repo: &Path, path: &str) -> Material {
        let bin =
            crate::tools::discover_git(None).await.path.map(std::path::PathBuf::from).unwrap();
        let cancel = tokio_util::sync::CancellationToken::new();
        let runner = Git::new(&bin, repo, &crate::report::NullReporter, &cancel);
        let changes = ChangeSet {
            source: ChangeSource::Git,
            base: Some("v1.0.0".into()),
            files: vec![ChangedFile { path: path.into(), kind: ChangeKind::Modified }],
            commits: vec![],
        };
        let filter = Filter::new(&crate::project::default_ai_exclude_patterns());
        from_git(&runner, repo, &changes, &filter).await.unwrap()
    }

    #[tokio::test]
    async fn git_diffs_of_non_ascii_names_are_kept() {
        let repo = tagged_repo(&[("café.php", "<?php\n")]);
        std::fs::write(repo.path().join("café.php"), "<?php\n// changed\n").unwrap();

        let material = material_since_tag(repo.path(), "café.php").await;
        assert_eq!(material.diffs.len(), 1, "the diff was dropped");
        assert_eq!(material.diffs[0].0, "café.php");
        assert!(material.diffs[0].1.contains("+// changed"));
    }

    #[tokio::test]
    async fn a_file_renamed_from_a_secret_never_shows_the_secret() {
        let settings = "DB_HOST=localhost\nDB_NAME=shop\nDB_USER=shop\nDB_PREFIX=wp_\n";
        let repo = tagged_repo(&[(".env", &format!("{settings}DB_PASSWORD=hunter2\n"))]);
        git(repo.path(), &["mv", ".env", "settings-sample.txt"]);
        let sample = format!("{settings}DB_PASSWORD=\n");
        std::fs::write(repo.path().join("settings-sample.txt"), sample).unwrap();

        let material = material_since_tag(repo.path(), "settings-sample.txt").await;
        assert_eq!(material.diffs.len(), 1);
        let diff = &material.diffs[0].1;
        assert!(!diff.contains("hunter2"), "{diff}");
        assert!(diff.contains("+DB_PASSWORD="), "the new file is still sent: {diff}");
    }
}
