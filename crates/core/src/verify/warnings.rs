//! Warnings W01–W10 (plan §5.4). They are shown and never block.

use std::collections::BTreeSet;

use crate::readme::README_FILE;
use crate::text;
use crate::version::Version;

use super::{CheckResult, Severity, VerifyInput};

/// Packages above this size warn (W05).
pub const PACKAGE_WARN_BYTES: u64 = 10 * 1024 * 1024;
/// Single files above this size warn (W05).
pub const FILE_WARN_BYTES: u64 = 2 * 1024 * 1024;
/// A `vendor/` folder above this size warns (W10).
pub const VENDOR_WARN_BYTES: u64 = 5 * 1024 * 1024;
/// WordPress.org shows at most this many tags (W03).
pub const MAX_TAGS: usize = 5;

fn check(id: &str, title: &str) -> CheckResult {
    CheckResult::new(id, Severity::Warn, title)
}

fn megabytes(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mb = bytes as f64 / (1024.0 * 1024.0);
    format!("{mb:.1} MB")
}

pub(super) fn all(input: &VerifyInput<'_>) -> Vec<CheckResult> {
    vec![
        w01(input),
        w02(input),
        w03(input),
        w04(input),
        w05(input),
        w06(input),
        w07(input),
        w08(input),
        w09(input),
        w10(input),
    ]
}

fn w01(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("W01", "Git working tree is clean");
    match input.git_dirty {
        None => c.skip("Git is not available for this project."),
        Some([]) => c.pass("No uncommitted changes."),
        Some(paths) => c
            .fail(
                format!(
                    "{} uncommitted change(s): the release will not match any commit.",
                    paths.len()
                ),
                "Commit or stash your changes before releasing.",
            )
            .with_paths(paths.to_vec()),
    }
}

fn major_minor(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.trim().split(['.', '-']);
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

fn w02(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("W02", "Tested up to is the current WordPress version");
    let Some(current) = input.current_wordpress else {
        return c.skip("The WordPress version lookup is off or unavailable.");
    };
    let Some(tested) = input.facts.readme.as_ref().and_then(|r| r.header("Tested up to")) else {
        return c.skip("readme.txt has no Tested up to header.");
    };
    match (major_minor(&tested.value), major_minor(current)) {
        (Some(t), Some(cur)) if t < cur => c
            .fail(
                format!("Tested up to is {}, WordPress {current} is current.", tested.value),
                format!(
                    "Test with WordPress {current}, then set \"Tested up to: {}.{}\".",
                    cur.0, cur.1
                ),
            )
            .with_paths(vec![README_FILE.to_owned()]),
        (Some(_), Some(_)) => c.pass(format!("Tested up to {}.", tested.value)),
        _ => c.skip("The versions could not be compared."),
    }
}

fn w03(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("W03", "Readme has five tags or fewer");
    let Some(tags) = input.facts.readme.as_ref().and_then(|r| r.header("Tags")) else {
        return c.skip("readme.txt has no Tags header.");
    };
    let count = tags.value.split(',').filter(|t| !t.trim().is_empty()).count();
    if count > MAX_TAGS {
        c.fail(
            format!("{count} tags. WordPress.org shows only the first five."),
            "Keep the five tags that matter most.",
        )
        .with_paths(vec![README_FILE.to_owned()])
    } else {
        c.pass(format!("{count} tag(s)."))
    }
}

fn screenshot_number(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    let rest = lower.strip_prefix("screenshot-")?;
    let (number, ext) = rest.split_once('.')?;
    let image = matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "webp");
    image.then(|| number.parse().ok())?
}

fn w04(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("W04", "Readme screenshots match the assets folder");
    let Some(assets) = input.assets else {
        return c.skip("No assets folder is configured.");
    };
    let Some(readme) = &input.facts.readme else {
        return c.skip("readme.txt is missing.");
    };
    let listed: BTreeSet<u32> = readme
        .sections
        .iter()
        .find(|s| s.title.eq_ignore_ascii_case("Screenshots"))
        .map(|s| {
            s.body.lines().filter_map(|line| line.trim().split_once('.')?.0.parse().ok()).collect()
        })
        .unwrap_or_default();
    let files: BTreeSet<u32> = assets.iter().filter_map(|n| screenshot_number(n)).collect();
    let no_file: Vec<String> =
        listed.difference(&files).map(|n| format!("screenshot-{n}")).collect();
    let no_caption: Vec<String> =
        files.difference(&listed).map(|n| format!("screenshot-{n}")).collect();
    if no_file.is_empty() && no_caption.is_empty() {
        return c.pass(format!("{} screenshot(s) match.", listed.len()));
    }
    let mut parts = Vec::new();
    if !no_file.is_empty() {
        parts.push(format!("captions without an image: {}", no_file.join(", ")));
    }
    if !no_caption.is_empty() {
        parts.push(format!("images without a caption: {}", no_caption.join(", ")));
    }
    c.fail(
        format!("Screenshots differ, {}.", parts.join("; ")),
        "Number the == Screenshots == list to match screenshot-N files in the assets folder.",
    )
    .with_paths(no_file.into_iter().chain(no_caption).collect())
}

fn w05(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("W05", "Package and files are a reasonable size");
    let total: u64 = input.files.iter().map(|f| f.size).sum();
    let large: Vec<String> =
        input.files.iter().filter(|f| f.size > FILE_WARN_BYTES).map(|f| f.rel.to_owned()).collect();
    if total <= PACKAGE_WARN_BYTES && large.is_empty() {
        return c.pass(format!("{} in total.", megabytes(total)));
    }
    let mut parts = Vec::new();
    if total > PACKAGE_WARN_BYTES {
        parts.push(format!("the package is {}", megabytes(total)));
    }
    if !large.is_empty() {
        parts.push(format!("{} file(s) are over 2 MB", large.len()));
    }
    c.fail(
        format!("Large package: {}.", parts.join(", ")),
        "Check that nothing belongs in .distignore.",
    )
    .with_paths(large)
}

fn w06(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("W06", "Version is not a pre-release");
    match Version::parse(input.version) {
        Ok(v) if v.is_prerelease() => c.fail(
            format!("{v} is a pre-release. WordPress.org serves it to everyone once the stable tag points to it."),
            "Release it only if you mean every site to receive it.",
        ),
        Ok(v) => c.pass(format!("{v} is a final release.")),
        Err(_) => c.skip("The version is invalid (V02)."),
    }
}

fn w07(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("W07", "Readme requirements match the plugin header");
    let Some(readme) = &input.facts.readme else {
        return c.skip("readme.txt is missing.");
    };
    let header = &input.facts.header;
    let pairs = [
        ("Requires PHP", header.requires_php.as_deref()),
        ("Requires at least", header.requires_at_least.as_deref()),
    ];
    let differences: Vec<String> = pairs
        .iter()
        .filter_map(|(name, in_header)| {
            let in_readme = readme.header(name)?.value.as_str();
            let in_header = (*in_header)?;
            (in_readme != in_header).then(|| {
                format!("{name} is {in_readme} in readme.txt and {in_header} in the header")
            })
        })
        .collect();
    if differences.is_empty() {
        c.pass("Requirements agree.")
    } else {
        c.fail(
            format!("{}.", differences.join("; ")),
            "Use the same values in readme.txt and the plugin header.",
        )
        .with_paths(vec![README_FILE.to_owned(), input.facts.main_file.clone()])
    }
}

fn w08(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("W08", "No packaged file is ignored by .gitignore");
    if input.gitignored.is_empty() {
        c.pass("Nothing in the package is git-ignored.")
    } else {
        c.fail(
            format!(
                "{} packaged file(s) are git-ignored, probably build output.",
                input.gitignored.len()
            ),
            "If they are needed, keep them; otherwise add them to .distignore.",
        )
        .with_paths(input.gitignored.to_vec())
    }
}

fn w09(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("W09", "readme.txt has consistent line endings and no BOM");
    let Some(readme) = input.readme_text else {
        return c.skip("readme.txt is missing.");
    };
    let report = text::eol_report(readme);
    let mut problems = Vec::new();
    if report.is_mixed() {
        problems.push(format!("mixed line endings ({} CRLF, {} LF)", report.crlf, report.lf));
    }
    if report.has_bom {
        problems.push("a UTF-8 byte order mark".to_owned());
    }
    if problems.is_empty() {
        c.pass("Line endings are consistent.")
    } else {
        c.fail(
            format!("readme.txt has {}.", problems.join(" and ")),
            "Save readme.txt as UTF-8 without BOM, with one line-ending style.",
        )
        .with_paths(vec![README_FILE.to_owned()])
    }
}

fn w10(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("W10", "vendor/ is not oversized");
    let vendor: u64 =
        input.files.iter().filter(|f| f.rel.starts_with("vendor/")).map(|f| f.size).sum();
    if vendor > VENDOR_WARN_BYTES {
        c.fail(
            format!("vendor/ is {}.", megabytes(vendor)),
            "Install with composer install --no-dev --optimize-autoloader before releasing.",
        )
        .with_paths(vec!["vendor/".to_owned()])
    } else if vendor == 0 {
        c.pass("No vendor/ folder is packaged.")
    } else {
        c.pass(format!("vendor/ is {}.", megabytes(vendor)))
    }
}
