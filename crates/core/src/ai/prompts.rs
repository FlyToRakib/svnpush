//! The three AI tasks' prompts (plan §9.7). Every prompt asks for one JSON
//! object and states its shape, which the OpenAI family needs in the text.

use crate::readme::ChangelogEntry;

/// Characters of diff sent in one `draft_release` prompt before files are summarised first.
pub const DIFF_BUDGET_CHARS: usize = 60_000;
/// Characters of one file's diff sent to `summarise_file`.
pub const FILE_DIFF_BUDGET_CHARS: usize = 20_000;

/// A system and a user prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// Instructions.
    pub system: String,
    /// The material.
    pub user: String,
}

/// What the release draft is written from.
#[derive(Debug, Clone, Copy)]
pub struct DraftMaterial<'a> {
    /// Plugin name.
    pub name: &'a str,
    /// Plugin slug.
    pub slug: &'a str,
    /// Previous release, if any.
    pub previous: Option<&'a str>,
    /// The newest changelog entries, for tone.
    pub recent_entries: &'a [ChangelogEntry],
    /// Commit subjects since the previous release.
    pub commits: &'a [String],
    /// `git diff --stat` or an equivalent list of changed files.
    pub stat: &'a str,
    /// The diff, or per-file summaries when the diff was too large.
    pub changes: &'a str,
    /// Whether `changes` holds summaries rather than a diff.
    pub from_summaries: bool,
}

const DRAFT_SYSTEM: &str = "You write WordPress.org plugin release notes. You are given what changed since the previous release. \
Write for the plugin's users, not its developers. Match the tone, tense and formatting of the plugin's existing changelog entries. \
If the existing entries group items under Added, Changed, Fixed or Security, group the same way; otherwise write a plain bulleted list using * at the start of each line. \
Mention only changes that are in the material. Do not invent features, do not use marketing language, and do not mention internal refactoring users cannot notice. \
Suggest the next version by semantic versioning: patch for fixes only, minor for new backward-compatible features, major for breaking changes. \
Give the reason for the version in one sentence. Write an upgrade notice of one or two sentences only when users should know something before updating; otherwise use an empty string. \
Write a summary of one short paragraph. \
Reply with a single JSON object and nothing else, with exactly these string fields: \
{\"version\": \"\", \"reason\": \"\", \"changelog_markdown\": \"\", \"upgrade_notice\": \"\", \"summary\": \"\"}. \
changelog_markdown is the body of the entry only, without the = version = heading.";

/// The `draft_release` prompt.
pub fn draft_release(material: &DraftMaterial<'_>) -> Prompt {
    let mut user = vec![
        format!("Plugin: {} (slug {})", material.name, material.slug),
        format!(
            "Previous version: {}",
            material.previous.unwrap_or("none, this is the first release")
        ),
    ];
    if !material.recent_entries.is_empty() {
        let entries: Vec<String> = material
            .recent_entries
            .iter()
            .take(3)
            .map(|e| format!("= {} =\n{}", e.title, e.body))
            .collect();
        user.push(format!(
            "Existing changelog entries, newest first, for style:\n{}",
            entries.join("\n\n")
        ));
    }
    if !material.commits.is_empty() {
        let commits: Vec<String> = material.commits.iter().map(|c| format!("- {c}")).collect();
        user.push(format!("Commit subjects since the previous release:\n{}", commits.join("\n")));
    }
    user.push(format!("Changed files:\n{}", material.stat));
    let label = if material.from_summaries {
        "Summaries of each file's changes (the full diff was too large)"
    } else {
        "Diff"
    };
    user.push(format!("{label}:\n{}", material.changes));
    Prompt { system: DRAFT_SYSTEM.to_owned(), user: user.join("\n\n") }
}

const SUMMARY_SYSTEM: &str = "You summarise one file's diff from a WordPress plugin for someone writing release notes. \
Describe what changed for users in one or two sentences, or say it is an internal change. Do not speculate beyond the diff. \
Reply with a single JSON object and nothing else: {\"path\": \"\", \"summary\": \"\"}.";

/// The `summarise_file` prompt. The diff is cut to [`FILE_DIFF_BUDGET_CHARS`].
pub fn summarise_file(path: &str, diff: &str) -> Prompt {
    let cut = truncate_chars(diff, FILE_DIFF_BUDGET_CHARS);
    Prompt { system: SUMMARY_SYSTEM.to_owned(), user: format!("File: {path}\n\nDiff:\n{cut}") }
}

/// A failed check, for `explain_failures`.
#[derive(Debug, Clone, Copy)]
pub struct FailedCheck<'a> {
    /// Check id.
    pub id: &'a str,
    /// What it verifies.
    pub title: &'a str,
    /// What was found.
    pub message: &'a str,
    /// The engine's fix text.
    pub fix: Option<&'a str>,
}

const EXPLAIN_SYSTEM: &str = "You help a WordPress plugin developer understand why release checks failed. \
Explain each failure in plain language and give the smallest fix. \
Only when a failure can be fixed by editing readme.txt, suggest the exact edit: copy the text to replace verbatim from the readme excerpt into original, and give the corrected text in replacement. \
Never suggest edits to PHP, JavaScript or any other code; describe those fixes in the explanation instead. \
Reply with a single JSON object and nothing else: \
{\"explanation\": \"\", \"fixes\": [{\"check_id\": \"\", \"path\": \"readme.txt\", \"original\": \"\", \"replacement\": \"\"}]}. \
Use an empty fixes array when no readme edit applies.";

/// The `explain_failures` prompt.
pub fn explain_failures(checks: &[FailedCheck<'_>], excerpts: &[(String, String)]) -> Prompt {
    let failures: Vec<String> = checks
        .iter()
        .map(|c| {
            let fix = c.fix.map_or_else(String::new, |f| format!(" Suggested fix: {f}"));
            format!("- {} {}: {}{fix}", c.id, c.title, c.message)
        })
        .collect();
    let mut user = vec![format!("Failed checks:\n{}", failures.join("\n"))];
    for (path, text) in excerpts {
        user.push(format!("Excerpt of {path}:\n{}", truncate_chars(text, FILE_DIFF_BUDGET_CHARS)));
    }
    Prompt { system: EXPLAIN_SYSTEM.to_owned(), user: user.join("\n\n") }
}

/// At most `max` characters, marked when cut.
pub fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push_str("\n[cut: the rest was not sent]");
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, body: &str) -> ChangelogEntry {
        ChangelogEntry {
            title: title.into(),
            version: Some(title.into()),
            body: body.into(),
            line: 1,
        }
    }

    #[test]
    fn draft_prompt_orders_the_material_as_planned() {
        let entries = [
            entry("1.1.0", "* Fixed a."),
            entry("1.0.0", "* First."),
            entry("0.9", "* Beta."),
            entry("0.8", "* Old."),
        ];
        let commits = vec!["Add export".to_owned()];
        let prompt = draft_release(&DraftMaterial {
            name: "Demo",
            slug: "demo",
            previous: Some("1.1.0"),
            recent_entries: &entries,
            commits: &commits,
            stat: " demo.php | 4 ++--",
            changes: "diff --git a/demo.php b/demo.php",
            from_summaries: false,
        });
        let user = &prompt.user;
        let order = [
            "Plugin: Demo",
            "Previous version: 1.1.0",
            "= 1.1.0 =",
            "- Add export",
            "Changed files:",
            "Diff:",
        ];
        let positions: Vec<usize> = order.iter().map(|needle| user.find(needle).unwrap()).collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{positions:?}");
        assert!(!user.contains("= 0.8 ="), "only the last three entries");
        assert!(prompt.system.contains("Do not invent features"));
        assert!(prompt.system.contains("JSON"));
    }

    #[test]
    fn summaries_are_labelled_and_long_text_is_cut() {
        let prompt = draft_release(&DraftMaterial {
            name: "Demo",
            slug: "demo",
            previous: None,
            recent_entries: &[],
            commits: &[],
            stat: "a.php",
            changes: "a.php: added export",
            from_summaries: true,
        });
        assert!(prompt.user.contains("none, this is the first release"));
        assert!(prompt.user.contains("Summaries of each file's changes"));
        let long = "x".repeat(FILE_DIFF_BUDGET_CHARS + 10);
        assert!(summarise_file("a.php", &long).user.ends_with("[cut: the rest was not sent]"));
    }

    #[test]
    fn explain_prompt_lists_failures_and_excerpts() {
        let checks = [FailedCheck {
            id: "V06",
            title: "Stable tag",
            message: "Stable tag is trunk.",
            fix: Some("Set it."),
        }];
        let prompt =
            explain_failures(&checks, &[("readme.txt".into(), "Stable tag: trunk".into())]);
        assert!(
            prompt.user.contains("- V06 Stable tag: Stable tag is trunk. Suggested fix: Set it.")
        );
        assert!(prompt.user.contains("Excerpt of readme.txt:\nStable tag: trunk"));
        assert!(prompt.system.contains("Never suggest edits to PHP"));
    }
}
