//! Blocking checks V01–V16 (plan §5.4).

use crate::readme::{README_FILE, REQUIRED_HEADERS};
use crate::version::Version;

use super::{CheckResult, Severity, VerifyInput, WorkingCopyState};

/// The shortest short description WordPress.org truncates beyond.
pub const SHORT_DESCRIPTION_LIMIT: usize = 150;

/// The oldest Subversion with `--password-from-stdin`.
pub const MIN_SVN: (u64, u64) = (1, 10);

const README_MISSING: &str = "readme.txt is missing from the package root.";
const README_FIX: &str = "Add a readme.txt that follows the WordPress.org readme standard.";

fn check(id: &str, title: &str) -> CheckResult {
    CheckResult::new(id, Severity::Block, title)
}

fn same_version(a: &str, b: &str) -> bool {
    match (Version::parse(a), Version::parse(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

pub(super) fn all(input: &VerifyInput<'_>) -> Vec<CheckResult> {
    let mut out = vec![
        v01(input),
        v02(input),
        v03(input),
        v04(input),
        v05(input),
        v06(input),
        v07(input),
        v08(input),
        v09(input),
    ];
    out.extend(package(input));
    out.extend([v14(input), v15(input.working_copy), v16(input)]);
    out
}

pub(super) fn package(input: &VerifyInput<'_>) -> Vec<CheckResult> {
    vec![v10(input), v11(input), v12(input), v13(input)]
}

fn v01(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V01", "Every version source holds the same value");
    let listing = || {
        input
            .facts
            .versions
            .iter()
            .map(|s| {
                let value = if s.value.is_empty() { "missing" } else { &s.value };
                format!("{}: {value}", s.label)
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    let mismatched: Vec<String> = input
        .facts
        .versions
        .iter()
        .filter(|s| s.value != input.version)
        .map(|s| s.path.clone())
        .collect();
    if mismatched.is_empty() {
        c.pass(format!("All sources read {}.", input.version))
    } else {
        c.fail(
            format!("Expected {} everywhere. Found {}.", input.version, listing()),
            format!("Set every source to {}.", input.version),
        )
        .with_paths(mismatched)
    }
}

fn v02(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V02", "Version is valid");
    match Version::parse(input.version) {
        Ok(_) => c.pass(format!("{} is a valid version.", input.version)),
        Err(_) => c.fail(
            format!("\"{}\" is not a valid version.", input.version),
            "Use x.y or x.y.z, optionally followed by a pre-release suffix such as -beta1.",
        ),
    }
}

fn v03(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V03", "Version is newer than the previous release and not yet tagged");
    let Ok(version) = Version::parse(input.version) else {
        return c.fail("The version cannot be compared because it is invalid.", "Fix V02 first.");
    };
    let newest_tag = crate::version::newest(input.server_tags.iter().map(String::as_str));
    let previous =
        input.previous.and_then(|p| Version::parse(p).ok()).into_iter().chain(newest_tag).max();
    let suggestion =
        previous.as_ref().map_or_else(|| "1.0.0".to_owned(), |p| p.next_patch().to_string());

    if let Some(tag) = input
        .server_tags
        .iter()
        .map(|t| t.trim_end_matches('/'))
        .find(|t| same_version(t, input.version))
    {
        return c
            .fail(
                format!("tags/{tag} already exists on the server."),
                format!("Release a new version, for example {suggestion}."),
            )
            .with_paths(vec![format!("tags/{tag}")]);
    }
    match previous {
        Some(p) if version <= p => c.fail(
            format!("{version} is not newer than the previous release {p}."),
            format!("Use a newer version, for example {suggestion}."),
        ),
        Some(p) => c.pass(format!("{version} is newer than {p}.")),
        None => c.pass("This is the first release."),
    }
}

fn v04(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V04", "Readme changelog names this version first");
    let Some(readme) = &input.facts.readme else {
        return c.fail(README_MISSING, README_FIX);
    };
    match readme.changelog.first() {
        Some(entry) if entry.version.as_deref().is_some_and(|v| same_version(v, input.version)) => {
            c.pass(format!("The newest changelog entry is {}.", entry.title))
        }
        Some(entry) => c
            .fail(
                format!("The newest changelog entry is \"{}\".", entry.title),
                format!("Insert a \"= {} =\" entry at the top of == Changelog ==.", input.version),
            )
            .with_paths(vec![README_FILE.to_owned()]),
        None => c
            .fail(
                "The readme has no changelog entries.",
                format!("Insert a \"= {} =\" entry under == Changelog ==.", input.version),
            )
            .with_paths(vec![README_FILE.to_owned()]),
    }
}

fn v05(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V05", "Readme has every required header");
    let Some(readme) = &input.facts.readme else {
        return c.fail(README_MISSING, README_FIX);
    };
    let mut missing: Vec<&str> = Vec::new();
    if readme.name.is_none() {
        missing.push("=== Plugin Name ===");
    }
    missing.extend(
        REQUIRED_HEADERS.iter().filter(|h| readme.header(h).is_none_or(|v| v.value.is_empty())),
    );
    if missing.is_empty() {
        c.pass("All required headers are present.")
    } else {
        c.fail(
            format!("Missing: {}.", missing.join(", ")),
            "Add the missing lines to the header block at the top of readme.txt.",
        )
        .with_paths(vec![README_FILE.to_owned()])
    }
}

fn v06(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V06", "Stable tag is this version");
    let Some(readme) = &input.facts.readme else {
        return c.fail(README_MISSING, README_FIX);
    };
    let fix = format!("Set \"Stable tag: {}\" in readme.txt.", input.version);
    match readme.header("Stable tag").map(|h| h.value.as_str()) {
        Some(tag) if tag.eq_ignore_ascii_case("trunk") => c
            .fail(
                "Stable tag is trunk. WordPress.org would serve unreleased trunk code instead of the tag.",
                fix,
            )
            .with_paths(vec![README_FILE.to_owned()]),
        Some(tag) if tag == input.version => c.pass(format!("Stable tag is {tag}.")),
        Some(tag) => c
            .fail(format!("Stable tag is {tag}."), fix)
            .with_paths(vec![README_FILE.to_owned()]),
        None => c
            .fail("readme.txt has no Stable tag header.", fix)
            .with_paths(vec![README_FILE.to_owned()]),
    }
}

fn v07(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V07", "Short description is 150 characters or fewer");
    let Some(readme) = &input.facts.readme else {
        return c.fail(README_MISSING, README_FIX);
    };
    let count = readme.short_description.chars().count();
    if count == 0 {
        c.fail(
            "The short description is empty.",
            "Add one sentence under the readme headers describing the plugin.",
        )
        .with_paths(vec![README_FILE.to_owned()])
    } else if count > SHORT_DESCRIPTION_LIMIT {
        c.fail(
            format!("The short description is {count} characters."),
            format!("Shorten it by {} characters.", count - SHORT_DESCRIPTION_LIMIT),
        )
        .with_paths(vec![README_FILE.to_owned()])
    } else {
        c.pass(format!("{count} characters."))
    }
}

fn v08(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V08", "Text Domain equals the slug");
    let slug = &input.facts.slug;
    let fix = format!("Set \"Text Domain: {slug}\" in the plugin header.");
    match input.facts.header.text_domain.as_deref() {
        Some(domain) if domain == slug => c.pass(format!("Text Domain is {domain}.")),
        Some(domain) => c
            .fail(format!("Text Domain is {domain}, the slug is {slug}."), fix)
            .with_paths(vec![input.facts.main_file.clone()]),
        None => c
            .fail("The plugin header has no Text Domain.", fix)
            .with_paths(vec![input.facts.main_file.clone()]),
    }
}

fn v09(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V09", "Main plugin file blocks direct access");
    let compact: String = input.main_file_text.chars().filter(|ch| !ch.is_whitespace()).collect();
    let guarded = ["ABSPATH", "WPINC"].iter().any(|constant| {
        compact.contains(&format!("defined('{constant}')"))
            || compact.contains(&format!("defined(\"{constant}\")"))
    });
    if guarded {
        c.pass("The file checks ABSPATH before running.")
    } else {
        c.fail(
            "The main plugin file does not check ABSPATH.",
            "Add near the top: if ( ! defined( 'ABSPATH' ) ) { exit; }",
        )
        .with_paths(vec![input.facts.main_file.clone()])
    }
}

fn v10(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V10", "Package contains every required path");
    let mut required = vec![input.facts.main_file.clone(), README_FILE.to_owned()];
    required.extend(input.required_paths.iter().map(|p| p.trim_matches('/').to_owned()));
    required.dedup();
    let missing: Vec<String> = required
        .into_iter()
        .filter(|path| {
            !input.files.iter().any(|f| f.rel == path || f.rel.starts_with(&format!("{path}/")))
        })
        .collect();
    if missing.is_empty() {
        c.pass("All required paths are in the package.")
    } else {
        c.fail(
            format!("Missing from the package: {}.", missing.join(", ")),
            "Add the files, or remove the rule in .distignore that excludes them.",
        )
        .with_paths(missing)
    }
}

fn file_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn has_extension(rel: &str, extensions: &[&str]) -> bool {
    let lower = file_name(rel).to_ascii_lowercase();
    extensions.iter().any(|ext| lower.ends_with(ext))
}

fn v11(input: &VerifyInput<'_>) -> CheckResult {
    const NAMES: [&str; 7] =
        [".git", ".svn", ".hg", ".svnpush.json", ".distignore", ".ds_store", "thumbs.db"];
    let c = check("V11", "Package contains no forbidden path");
    let offending: Vec<String> = input
        .files
        .iter()
        .filter(|f| {
            f.rel.split('/').any(|part| NAMES.contains(&part.to_ascii_lowercase().as_str()))
                || has_extension(f.rel, &[".zip", ".tar.gz"])
        })
        .map(|f| f.rel.to_owned())
        .collect();
    if offending.is_empty() {
        c.pass("No forbidden paths.")
    } else {
        c.fail(
            format!("{} forbidden path(s) in the package.", offending.len()),
            "These are always excluded; remove them from the package root.",
        )
        .with_paths(offending)
    }
}

fn v12(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V12", "Package contains no archive, executable or version-control folder");
    let mut blocked = vec![".zip", ".tar", ".gz", ".tgz", ".rar", ".7z", ".exe", ".dll", ".msi"];
    if !input.allow_phar {
        blocked.push(".phar");
    }
    let offending: Vec<String> = input
        .files
        .iter()
        .filter(|f| {
            has_extension(f.rel, &blocked)
                || f.rel.split('/').rev().skip(1).any(|dir| dir == ".git" || dir == ".svn")
        })
        .map(|f| f.rel.to_owned())
        .collect();
    if offending.is_empty() {
        c.pass("No archives, executables or version-control folders.")
    } else {
        c.fail(
            format!("{} file(s) WordPress.org does not accept.", offending.len()),
            "Exclude them in .distignore. A .phar can be allowed in project settings.",
        )
        .with_paths(offending)
    }
}

fn v13(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V13", "Zip entries use / and sit under the slug folder");
    let Some(entries) = input.zip_entries else {
        return c.skip("Runs after Build creates the zip.");
    };
    let prefix = format!("{}/", input.facts.slug);
    let offending: Vec<String> =
        entries.iter().filter(|e| e.contains('\\') || !e.starts_with(&prefix)).cloned().collect();
    if entries.is_empty() {
        c.fail("The zip is empty.", "Rebuild the package.")
    } else if offending.is_empty() {
        c.pass(format!("{} entries under {prefix}.", entries.len()))
    } else {
        c.fail(
            format!("{} entries are outside {prefix} or use \\.", offending.len()),
            "Rebuild the package.",
        )
        .with_paths(offending)
    }
}

/// Parses `major.minor` from `svn --version --quiet` output such as `1.14.5 (r1922182)`.
pub fn svn_major_minor(output: &str) -> Option<(u64, u64)> {
    let token = output.split_whitespace().next()?;
    let mut parts = token.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

fn v14(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V14", "Subversion 1.10 or newer is available");
    let install = "Install Subversion: on Windows TortoiseSVN with command-line tools, choco install svn, or SlikSVN; on macOS brew install subversion; on Linux apt install subversion.";
    match input.svn_version.map(|v| (v, svn_major_minor(v))) {
        None => c.fail("Subversion was not found.", install),
        Some((raw, Some(found))) if found >= MIN_SVN => c.pass(format!("svn {}", raw.trim())),
        Some((raw, _)) => c.fail(format!("svn {} is older than 1.10.", raw.trim()), install),
    }
}

/// V15 on its own, for Preview SVN to repeat after updating the working copy.
pub fn v15(working_copy: &WorkingCopyState) -> CheckResult {
    let c = check("V15", "SVN working copy is conflict-free and up to date");
    match working_copy {
        WorkingCopyState::NotCreated => {
            c.skip("The working copy is created and updated in Preview SVN.")
        }
        WorkingCopyState::Clean => c.pass("No conflicts, nothing newer on the server."),
        WorkingCopyState::Conflicts(paths) => c
            .fail(format!("{} conflicted path(s).", paths.len()), "Reset the working copy.")
            .with_paths(paths.clone()),
        WorkingCopyState::OutOfDate(paths) => c
            .fail(
                format!("{} path(s) changed on the server.", paths.len()),
                "Update the working copy, or reset it.",
            )
            .with_paths(paths.clone()),
    }
}

fn v16(input: &VerifyInput<'_>) -> CheckResult {
    let c = check("V16", "SVN credentials are in the vault");
    match input.has_credentials {
        None => c.skip("Dry run: credentials are needed only to publish."),
        Some(true) => c.pass("Credentials found."),
        Some(false) => c.fail(
            "No SVN password is stored for this project's account.",
            "Open Vault and add the account's SVN password.",
        ),
    }
}
