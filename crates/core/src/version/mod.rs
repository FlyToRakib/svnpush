//! Versions: parse, compare, suggest, and every place a plugin declares one.

mod sources;

use std::cmp::Ordering;
use std::fmt;

use crate::error::Coded;

pub use sources::{
    VersionLocation, VersionSource, VersionSourceKind, read_sources, validate_location,
    write_version,
};

/// A plugin version: `x.y` or `x.y.z` with an optional pre-release suffix.
///
/// Comparison normalises `1.0` to `1.0.0` and orders pre-release suffixes the
/// way PHP's `version_compare` does (`beta9` < `beta10` < `RC1`); the text is
/// kept exactly as written because it names the tag folder.
#[derive(Debug, Clone)]
pub struct Version {
    raw: String,
    semver: semver::Version,
}

impl Version {
    /// Parses a version in the format check V02 accepts.
    pub fn parse(raw: &str) -> Result<Self, VersionError> {
        let invalid = || VersionError::Invalid { value: raw.to_owned() };
        let (core, pre) = match raw.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (raw, None),
        };
        let parts: Vec<&str> = core.split('.').collect();
        if !(2..=3).contains(&parts.len()) {
            return Err(invalid());
        }
        let mut numbers = [0_u64; 3];
        for (slot, part) in numbers.iter_mut().zip(&parts) {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid());
            }
            *slot = part.parse().map_err(|_| invalid())?;
        }
        let pre = match pre {
            None => semver::Prerelease::EMPTY,
            Some(p) => {
                let well_formed = p
                    .split('.')
                    .all(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric()));
                if !well_formed {
                    return Err(invalid());
                }
                semver::Prerelease::new(p).map_err(|_| invalid())?
            }
        };
        Ok(Self {
            raw: raw.to_owned(),
            semver: semver::Version {
                major: numbers[0],
                minor: numbers[1],
                patch: numbers[2],
                pre,
                build: semver::BuildMetadata::EMPTY,
            },
        })
    }

    /// The version exactly as written.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Whether the version carries a pre-release suffix (warning W06).
    pub fn is_prerelease(&self) -> bool {
        !self.semver.pre.is_empty()
    }

    /// The next patch release: `1.2` → `1.2.1`, `1.2.3` → `1.2.4`,
    /// `1.3.0-beta1` → `1.3.0`.
    #[must_use]
    pub fn next_patch(&self) -> Self {
        let v = &self.semver;
        let (patch, raw) = if self.is_prerelease() {
            (v.patch, format!("{}.{}.{}", v.major, v.minor, v.patch))
        } else {
            let patch = v.patch.saturating_add(1);
            (patch, format!("{}.{}.{}", v.major, v.minor, patch))
        };
        Self { raw, semver: semver::Version::new(v.major, v.minor, patch) }
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let (a, b) = (&self.semver, &other.semver);
        (a.major, a.minor, a.patch)
            .cmp(&(b.major, b.minor, b.patch))
            .then_with(|| compare_suffix(&canonical(&a.pre), &canonical(&b.pre)))
    }
}

/// A suffix split the way `version_compare` canonicalises it: at `.`, `-`,
/// `_`, `+` and wherever digits and letters meet.
fn canonical(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (i, c) in text.char_indices() {
        let kind_changes =
            text[start..i].chars().last().is_some_and(|p| p.is_ascii_digit() != c.is_ascii_digit());
        if !c.is_ascii_alphanumeric() || kind_changes {
            if start < i {
                parts.push(&text[start..i]);
            }
            start = if c.is_ascii_alphanumeric() { i } else { i + c.len_utf8() };
        }
    }
    if start < text.len() {
        parts.push(&text[start..]);
    }
    parts
}

/// `version_compare`'s rank for a word: `dev` < `alpha`/`a` < `beta`/`b` <
/// `RC` < a number (`#`) < `pl`/`p`. PHP matches by prefix; any other word
/// ranks lowest.
fn rank(part: &str) -> i8 {
    const FORMS: [(&str, i8); 8] =
        [("dev", 0), ("alpha", 1), ("a", 1), ("beta", 2), ("b", 2), ("rc", 3), ("#", 4), ("p", 5)];
    let lower = part.to_ascii_lowercase();
    FORMS.iter().find(|(form, _)| lower.starts_with(form)).map_or(-6, |&(_, r)| r)
}

fn is_number(part: &str) -> bool {
    part.bytes().all(|b| b.is_ascii_digit())
}

fn compare_numbers(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// Compares two canonical suffixes part by part, as `version_compare` does.
/// A missing part counts as a number, so `1.0-beta1` < `1.0` < `1.0-pl1`.
fn compare_suffix(a: &[&str], b: &[&str]) -> Ordering {
    let word = |part: &str| if is_number(part) { rank("#") } else { rank(part) };
    for (x, y) in a.iter().zip(b) {
        let order = if is_number(x) && is_number(y) {
            compare_numbers(x, y)
        } else {
            word(x).cmp(&word(y))
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    match (a.get(b.len()), b.get(a.len())) {
        (Some(x), _) if is_number(x) => Ordering::Greater,
        (Some(x), _) => rank(x).cmp(&rank("#")),
        (_, Some(y)) if is_number(y) => Ordering::Less,
        (_, Some(y)) => rank("#").cmp(&rank(y)),
        _ => Ordering::Equal,
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// The newest valid version in `candidates`, for example tag folder names.
pub fn newest<'a>(candidates: impl IntoIterator<Item = &'a str>) -> Option<Version> {
    candidates.into_iter().filter_map(|c| Version::parse(c.trim_end_matches('/')).ok()).max()
}

/// The version a changelog title starts with, such as `1.2.3` in `v1.2.3 - 2026-09-01`.
pub fn extract_leading(title: &str) -> Option<String> {
    let rest = title.trim().strip_prefix(['v', 'V']).unwrap_or_else(|| title.trim());
    let core_len = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
    let mut end = core_len;
    if rest[core_len..].starts_with('-') {
        let suffix = &rest[core_len + 1..];
        let suffix_len =
            suffix.find(|c: char| !(c.is_ascii_alphanumeric() || c == '.')).unwrap_or(suffix.len());
        if suffix_len > 0 {
            end = core_len + 1 + suffix_len;
        }
    }
    let candidate = rest[..end].trim_end_matches('.');
    Version::parse(candidate).ok().map(|v| v.as_str().to_owned())
}

/// A version problem.
#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    /// The text is not a valid plugin version.
    #[error("\"{value}\" is not a valid version")]
    Invalid { value: String },
    /// A custom version location's pattern is unusable.
    #[error("the pattern for {path} is invalid: {reason}")]
    BadPattern { path: String, reason: String },
    /// A version location's path leaves the package root.
    #[error("the version location \"{path}\" is not a file inside the plugin folder")]
    OutsideRoot { path: String },
    /// A version location matched nothing.
    #[error("no version found in {path}")]
    NotFound { path: String },
    /// A file involved could not be loaded or saved.
    #[error(transparent)]
    Edit(#[from] crate::edit::EditError),
}

impl Coded for VersionError {
    fn code(&self) -> &'static str {
        match self {
            Self::Invalid { .. } => "VERSION_INVALID",
            Self::BadPattern { .. } => "VERSION_BAD_PATTERN",
            Self::OutsideRoot { .. } => "VERSION_PATH_OUTSIDE",
            Self::NotFound { .. } => "VERSION_NOT_FOUND",
            Self::Edit(inner) => inner.code(),
        }
    }

    fn fix(&self) -> Option<String> {
        match self {
            Self::Invalid { .. } => Some(
                "Use x.y or x.y.z, optionally followed by a pre-release suffix such as -beta1."
                    .to_owned(),
            ),
            Self::BadPattern { .. } => Some(
                "Use a regular expression with exactly one capture group around the version."
                    .to_owned(),
            ),
            Self::OutsideRoot { .. } => Some(
                "Use a path relative to the plugin folder, such as includes/version.php, in project settings or .svnpush.json."
                    .to_owned(),
            ),
            Self::NotFound { path } => Some(format!(
                "Check that {path} still contains the version, or update the pattern in project settings."
            )),
            Self::Edit(inner) => inner.fix(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn accepts_the_v02_formats() {
        for ok in ["1.0", "1.2.3", "10.20.30", "1.2.0-beta1", "2.0-rc.1"] {
            assert!(Version::parse(ok).is_ok(), "{ok}");
        }
        for bad in ["1", "1.2.3.4", "v1.2", "1.a", "1.2.", "", "1.2-", "1.2-be_ta", "trunk"] {
            assert!(Version::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn compares_with_normalisation() {
        assert_eq!(v("1.0"), v("1.0.0"));
        assert!(v("1.0.1") > v("1.0"));
        assert!(v("1.2.0-beta1") < v("1.2.0"));
        assert!(v("1.10") > v("1.9.9"));
        assert_eq!(v("1.0").as_str(), "1.0");
    }

    #[test]
    fn next_patch() {
        assert_eq!(v("1.2").next_patch().as_str(), "1.2.1");
        assert_eq!(v("1.2.3").next_patch().as_str(), "1.2.4");
        assert_eq!(v("1.3.0-beta1").next_patch().as_str(), "1.3.0");
        let top = format!("1.0.{}", u64::MAX);
        assert_eq!(v(&top).next_patch().as_str(), top);
    }

    #[test]
    fn orders_pre_releases_like_php_version_compare() {
        assert!(v("1.0-beta10") > v("1.0-beta9"));
        assert!(v("1.0-RC1") > v("1.0-beta2"));
        assert!(v("1.0-rc1") > v("1.0-b3"));
        assert!(v("1.0-alpha2") > v("1.0-dev"));
        assert!(v("1.0-dev") > v("1.0-foo"));
        assert!(v("1.0") > v("1.0-RC2"));
        assert!(v("1.0-pl1") > v("1.0"));
        assert!(v("1.0-beta.2") > v("1.0-beta1"));
        assert_eq!(v("1.0-beta1"), v("1.0-beta.1"));
        assert_eq!(v("1.0-Beta1"), v("1.0.0-beta1"));
        let tags = ["1.0-beta9", "1.0-beta10", "1.0-alpha11"];
        assert_eq!(newest(tags).unwrap().as_str(), "1.0-beta10");
    }

    #[test]
    fn newest_ignores_non_versions() {
        let tags = ["1.0/", "1.10/", "1.9", "beta/", "2.0-rc1"];
        assert_eq!(newest(tags).unwrap().as_str(), "2.0-rc1");
        assert!(newest(["trunk"]).is_none());
    }

    #[test]
    fn extracts_versions_from_titles() {
        assert_eq!(extract_leading("1.2.3").as_deref(), Some("1.2.3"));
        assert_eq!(extract_leading("v2.0 - 2026-01-01").as_deref(), Some("2.0"));
        assert_eq!(extract_leading("1.3.0-beta1 (preview)").as_deref(), Some("1.3.0-beta1"));
        assert_eq!(extract_leading("Version 1.0"), None);
        assert_eq!(extract_leading("1.0."), Some("1.0".to_owned()));
    }
}
