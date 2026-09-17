//! Which files leave the package root: `.distignore`, built-in defaults, and
//! the hard-excluded list that always applies.

use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::Serialize;
use ts_rs::TS;

use super::PackageError;
use crate::detect::DISTIGNORE_FILE;

/// Built-in exclusions, used only when the package root has no `.distignore` (plan §7.2).
pub const DEFAULT_EXCLUDES: &[&str] = &[
    ".git",
    ".github",
    ".gitignore",
    ".gitattributes",
    ".gitlab-ci.yml",
    ".svn",
    ".svnpush.json",
    ".distignore",
    ".editorconfig",
    "node_modules",
    "tests",
    "test",
    "phpunit.xml",
    "phpunit.xml.dist",
    "phpcs.xml",
    "phpcs.xml.dist",
    "phpstan.neon",
    "phpstan.neon.dist",
    "composer.json",
    "composer.lock",
    "package.json",
    "package-lock.json",
    "yarn.lock",
    "webpack.config.js",
    "vite.config.*",
    "tsconfig.json",
    ".babelrc",
    ".eslintrc*",
    ".prettierrc*",
    "docker-compose.yml",
    "Dockerfile",
    ".docker",
    ".wordpress-org",
    "README.md",
    "CHANGELOG.md",
    "CONTRIBUTING.md",
    "SECURITY.md",
    ".DS_Store",
    "Thumbs.db",
    "*.log",
    "*.zip",
    "*.map",
    ".phpunit.result.cache",
    "/release",
    "/build",
    "/dist",
    "/coverage",
    "/.idea",
    "/.vscode",
    "*.swp",
];

/// Always excluded, whatever `.distignore` says (plan §7.3).
pub const HARD_EXCLUDES: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    ".svnpush.json",
    ".distignore",
    "*.zip",
    "*.tar.gz",
    ".DS_Store",
    "Thumbs.db",
];

/// Which list decided the exclusions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum ExclusionSource {
    /// The package root's `.distignore`.
    Distignore,
    /// The built-in default list.
    Defaults,
}

/// The compiled exclusion rules for one package root.
#[derive(Debug)]
pub struct Exclusions {
    rules: Gitignore,
    hard: Gitignore,
    source: ExclusionSource,
    excluded_dirs: Vec<PathBuf>,
}

fn bad(err: &ignore::Error) -> PackageError {
    PackageError::BadDistignore { reason: err.to_string() }
}

fn compile(root: &Path, lines: &[&str], case_insensitive: bool) -> Result<Gitignore, PackageError> {
    let mut builder = GitignoreBuilder::new(root);
    builder.case_insensitive(case_insensitive).map_err(|e| bad(&e))?;
    for line in lines {
        builder.add_line(None, line).map_err(|e| bad(&e))?;
    }
    builder.build().map_err(|e| bad(&e))
}

impl Exclusions {
    /// Loads the rules for `root`. `excluded_dirs` are absolute folders that
    /// never ship, such as the app's own build output directory.
    pub fn load(root: &Path, excluded_dirs: &[PathBuf]) -> Result<Self, PackageError> {
        let distignore = root.join(DISTIGNORE_FILE);
        let (rules, source) = if distignore.is_file() {
            let mut builder = GitignoreBuilder::new(root);
            if let Some(err) = builder.add(&distignore) {
                return Err(bad(&err));
            }
            (builder.build().map_err(|e| bad(&e))?, ExclusionSource::Distignore)
        } else {
            (compile(root, DEFAULT_EXCLUDES, false)?, ExclusionSource::Defaults)
        };
        Ok(Self {
            rules,
            hard: compile(root, HARD_EXCLUDES, true)?,
            source,
            excluded_dirs: excluded_dirs.to_vec(),
        })
    }

    /// Only the always-excluded names, for folders that are not packages
    /// (the assets folder). Reports [`ExclusionSource::Defaults`].
    pub fn hard_only(root: &Path) -> Result<Self, PackageError> {
        Ok(Self {
            rules: compile(root, &[], false)?,
            hard: compile(root, HARD_EXCLUDES, true)?,
            source: ExclusionSource::Defaults,
            excluded_dirs: Vec::new(),
        })
    }

    /// Which list is in effect.
    pub fn source(&self) -> ExclusionSource {
        self.source
    }

    /// Whether `abs` (a path under the root) is excluded. Parents are the
    /// caller's responsibility: a walk stops descending into excluded folders.
    pub fn is_excluded(&self, abs: &Path, is_dir: bool) -> bool {
        if is_dir && self.excluded_dirs.iter().any(|d| abs.starts_with(d)) {
            return true;
        }
        self.hard.matched(abs, is_dir).is_ignore() || self.rules.matched(abs, is_dir).is_ignore()
    }
}

/// Packaged files that a `.gitignore` in the package root or one of their
/// folders ignores (warning W08). `files` are `/`-separated relative paths.
pub fn gitignored(root: &Path, files: &[String]) -> Vec<String> {
    let mut matchers: std::collections::HashMap<PathBuf, Option<Gitignore>> =
        std::collections::HashMap::new();
    let mut out = Vec::new();
    for rel in files {
        let abs = root.join(rel);
        let mut dir = abs.parent();
        let mut ignored = false;
        while let Some(current) = dir {
            if !current.starts_with(root) {
                break;
            }
            let matcher = matchers.entry(current.to_path_buf()).or_insert_with(|| {
                let file = current.join(".gitignore");
                file.is_file().then(|| {
                    let mut builder = GitignoreBuilder::new(current);
                    builder.add(&file);
                    builder.build().ok()
                })?
            });
            if let Some(m) = matcher
                && m.matched_path_or_any_parents(&abs, false).is_ignore()
            {
                ignored = true;
                break;
            }
            dir = current.parent();
        }
        if ignored {
            out.push(rel.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules_for(dir: &Path) -> Exclusions {
        Exclusions::load(dir, &[]).unwrap()
    }

    #[test]
    fn defaults_apply_without_distignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let ex = rules_for(root);
        assert_eq!(ex.source(), ExclusionSource::Defaults);
        assert!(ex.is_excluded(&root.join("node_modules"), true));
        assert!(ex.is_excluded(&root.join("src/tests"), true));
        assert!(ex.is_excluded(&root.join("vite.config.ts"), false));
        assert!(ex.is_excluded(&root.join("build"), true));
        assert!(!ex.is_excluded(&root.join("assets/build"), true));
        assert!(!ex.is_excluded(&root.join("vendor"), true));
        assert!(!ex.is_excluded(&root.join("plugin.php"), false));
    }

    #[test]
    fn distignore_replaces_defaults_but_not_hard_excludes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join(".distignore"), "/vendor\ndocs\n!docs/keep.md\n").unwrap();
        let ex = rules_for(root);
        assert_eq!(ex.source(), ExclusionSource::Distignore);
        assert!(ex.is_excluded(&root.join("vendor"), true));
        assert!(!ex.is_excluded(&root.join("admin/js/vendor"), true));
        assert!(!ex.is_excluded(&root.join("node_modules"), true));
        assert!(ex.is_excluded(&root.join(".git"), true));
        assert!(ex.is_excluded(&root.join("sub/backup.ZIP"), false));
        assert!(ex.is_excluded(&root.join("x.tar.gz"), false));
        assert!(ex.is_excluded(&root.join(".distignore"), false));
    }

    #[test]
    fn excluded_dirs_are_absolute() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let out = root.join("app-builds");
        let ex = Exclusions::load(root, std::slice::from_ref(&out)).unwrap();
        assert!(ex.is_excluded(&out, true));
    }

    #[test]
    fn gitignored_reads_root_and_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("assets/js")).unwrap();
        std::fs::write(root.join(".gitignore"), "/dist-out.js\n").unwrap();
        std::fs::write(root.join("assets/.gitignore"), "*.min.js\n").unwrap();
        let files = vec![
            "dist-out.js".to_owned(),
            "assets/js/app.min.js".to_owned(),
            "assets/js/app.js".to_owned(),
        ];
        assert_eq!(gitignored(root, &files), ["dist-out.js", "assets/js/app.min.js"]);
    }
}
