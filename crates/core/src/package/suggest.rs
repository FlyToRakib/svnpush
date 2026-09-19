//! The rules used when a plugin has no `.distignore`, and the `.distignore`
//! SVNpush proposes from them. A `build/` or `dist/` folder that the plugin's
//! own code loads (block plugins load `build/index.js`) is kept, because
//! leaving it out would publish a broken plugin.

use std::path::Path;

use super::rules::DEFAULT_EXCLUDES;

/// Folders that usually hold development output but sometimes runtime assets.
const BUILT_FOLDERS: [&str; 2] = ["build", "dist"];

/// Folders never searched for references: dependencies and version control.
const SKIPPED_DIRS: [&str; 4] = ["node_modules", "vendor", ".git", ".svn"];

/// Files larger than this are not searched for references.
const MAX_SCANNED_BYTES: u64 = 1024 * 1024;

const HEADER: &str = "\
# Files and folders that are NOT released to WordPress.org.
# Same syntax as .gitignore: one pattern per line, a leading / means the
# plugin folder itself, and ! keeps something a pattern above left out.
# SVNpush created this file. Edit it, then commit it with your plugin.
";

/// Whether PHP code or a `block.json` in the plugin refers to `folder/`.
fn referenced(root: &Path, folder: &str) -> bool {
    let needles = [
        format!("'{folder}/"),
        format!("\"{folder}/"),
        format!("/{folder}/"),
        format!("./{folder}/"),
    ];
    walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(e.file_type().is_dir()
                && e.depth() > 0
                && (SKIPPED_DIRS.contains(&name.as_ref()) || (e.depth() == 1 && name == folder)))
        })
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let name = e.file_name().to_string_lossy();
            name.ends_with(".php") || name == "block.json"
        })
        .filter(|e| e.metadata().is_ok_and(|m| m.len() <= MAX_SCANNED_BYTES))
        .any(|e| {
            std::fs::read_to_string(e.path())
                .is_ok_and(|text| needles.iter().any(|needle| text.contains(needle.as_str())))
        })
}

/// The built-in rules for this plugin: the defaults, keeping any build
/// folder the plugin's code loads.
pub fn default_rules(root: &Path) -> Vec<&'static str> {
    DEFAULT_EXCLUDES
        .iter()
        .copied()
        .filter(|line| {
            let folder = line.trim_start_matches('/');
            !(line.starts_with('/') && BUILT_FOLDERS.contains(&folder) && referenced(root, folder))
        })
        .collect()
}

/// The `.distignore` SVNpush proposes: a short header and [`default_rules`].
pub fn suggested_distignore(root: &Path) -> String {
    let mut text = HEADER.to_owned();
    for line in default_rules(root) {
        text.push_str(line);
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_folders_the_code_loads_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("plugin.php"), "<?php\n").unwrap();
        assert!(default_rules(root).contains(&"/build"));
        assert!(default_rules(root).contains(&"/dist"));

        std::fs::write(
            root.join("plugin.php"),
            "<?php\nregister_block_type( __DIR__ . '/build/block' );\n",
        )
        .unwrap();
        let rules = default_rules(root);
        assert!(!rules.contains(&"/build"), "build/ is loaded by the plugin");
        assert!(rules.contains(&"/dist"));
    }

    #[test]
    fn block_json_references_count_and_dependencies_do_not() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("blocks/hero")).unwrap();
        std::fs::write(
            root.join("blocks/hero/block.json"),
            r#"{"editorScript":"file:./dist/hero.js"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("vendor/lib")).unwrap();
        std::fs::write(root.join("vendor/lib/x.php"), "<?php // '/build/'").unwrap();
        let rules = default_rules(root);
        assert!(!rules.contains(&"/dist"));
        assert!(rules.contains(&"/build"), "vendor code does not count");
    }

    #[test]
    fn the_suggestion_explains_itself_and_leaves_out_hidden_files() {
        let dir = tempfile::tempdir().unwrap();
        let text = suggested_distignore(dir.path());
        assert!(text.starts_with("# Files and folders that are NOT released"));
        assert!(text.lines().any(|l| l == ".*"));
        assert!(text.lines().any(|l| l == "/docs"));
    }
}
