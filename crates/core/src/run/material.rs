//! Step 2.1 for the AI: the diff stat and the per-file diffs a prompt is
//! written from, prioritised and filtered (plan §5.2, §9.8).

use std::collections::BTreeMap;
use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::ai::prompts::{DIFF_BUDGET_CHARS, FILE_DIFF_BUDGET_CHARS, truncate_chars};
use crate::detect::git::Git;
use crate::edit;
use crate::readme::README_FILE;
use crate::tools::ProcessError;

use super::changes::{ChangeKind, ChangeSet, ChangeSource};
use super::steps::unified_diff;

/// At most this many files are summarised one by one when the diff is too large.
pub const MAX_SUMMARISED_FILES: usize = 25;

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

/// Files that look like secrets never reach a prompt, whatever the settings say.
pub fn looks_secret(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    name.starts_with(".env") || has_extension(&name, &["pem", "key"]) || name == "wp-config.php"
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

    /// Whether `path` stays out of the prompt.
    pub fn excludes(&self, path: &str) -> bool {
        looks_secret(path)
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

fn text_of(root: &Path, rel: &str) -> Option<String> {
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
            "({} file(s) withheld from the AI by the exclude patterns)",
            material.withheld.len()
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
    let output =
        git.output(&["diff", "-M", "--no-color", &base, "--", "."]).await?.unwrap_or_default();
    let mut diffs = split_git_diff(&output);
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
        for secret in
            [".env", "config/.env.local", "certs/site.pem", "private.key", "wp-config.php"]
        {
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
}
