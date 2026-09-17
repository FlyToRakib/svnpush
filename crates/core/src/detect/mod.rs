//! Step 1, Detect: the facts about a plugin that every later step relies on.

pub mod header;
mod slug;

use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

use crate::edit::{self, EditError};
use crate::error::Coded;
use crate::readme::{self, README_FILE, Readme};
use crate::version::{self, VersionError, VersionLocation, VersionSource};

pub use header::Header;
pub use slug::{slug_from_svn_url, suggested_svn_url};

/// The optional per-project file teams commit to share settings.
pub const PROJECT_CONFIG_FILE: &str = ".svnpush.json";

/// The WP-CLI `dist-archive` exclusion file.
pub const DISTIGNORE_FILE: &str = ".distignore";

/// What to detect and where.
#[derive(Debug, Clone, Copy)]
pub struct DetectOptions<'a> {
    /// The project's SVN URL; the slug is derived from it.
    pub svn_url: &'a str,
    /// The main plugin file chosen in project settings, when set.
    pub main_file: Option<&'a str>,
    /// Extra version locations configured for the project.
    pub version_locations: &'a [VersionLocation],
}

/// Everything Step 1 learns from the package root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PluginFacts {
    /// Plugin name from the header, else the readme, else the slug.
    pub name: String,
    /// Slug derived from the SVN URL (authoritative).
    pub slug: String,
    /// Main plugin file, relative to the package root.
    pub main_file: String,
    /// Header fields of the main plugin file.
    pub header: Header,
    /// The parsed `readme.txt`, when present.
    pub readme: Option<Readme>,
    /// Every version source found.
    pub versions: Vec<VersionSource>,
    /// Whether the package root folder name equals the slug (a warning when not).
    pub folder_matches_slug: bool,
    /// Whether a `.distignore` exists in the package root.
    pub has_distignore: bool,
    /// Whether a `.svnpush.json` exists in the package root.
    pub has_project_config: bool,
}

/// A detection failure.
#[derive(Debug, thiserror::Error)]
pub enum DetectError {
    /// The package root is missing or not a folder.
    #[error("{path} is not a folder")]
    NotAFolder { path: String },
    /// No PHP file in the package root has a `Plugin Name:` header.
    #[error(
        "no main plugin file found: no PHP file in the package root has a \"Plugin Name:\" header"
    )]
    NoMainFile,
    /// More than one PHP file has a `Plugin Name:` header.
    #[error("more than one main plugin file found: {}", candidates.join(", "))]
    MultipleMainFiles { candidates: Vec<String> },
    /// The configured main file does not exist or has no header.
    #[error("{path} is not a main plugin file")]
    NotAMainFile { path: String },
    /// The SVN URL does not end in a valid plugin slug.
    #[error("\"{url}\" does not end in a plugin slug")]
    InvalidSvnUrl { url: String },
    /// Listing the package root failed.
    #[error("could not read {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    /// A file could not be read.
    #[error(transparent)]
    Edit(#[from] EditError),
    /// A version location is invalid.
    #[error(transparent)]
    Version(#[from] VersionError),
}

impl Coded for DetectError {
    fn code(&self) -> &'static str {
        match self {
            Self::NotAFolder { .. } => "DETECT_NOT_A_FOLDER",
            Self::NoMainFile => "DETECT_NO_MAIN_FILE",
            Self::MultipleMainFiles { .. } => "DETECT_MULTIPLE_MAIN_FILES",
            Self::NotAMainFile { .. } => "DETECT_NOT_A_MAIN_FILE",
            Self::InvalidSvnUrl { .. } => "DETECT_INVALID_SVN_URL",
            Self::Io { .. } => "DETECT_IO",
            Self::Edit(inner) => inner.code(),
            Self::Version(inner) => inner.code(),
        }
    }

    fn fix(&self) -> Option<String> {
        match self {
            Self::NotAFolder { .. } => Some("Choose the plugin folder again.".to_owned()),
            Self::NoMainFile => Some(
                "Add a plugin header with \"Plugin Name:\" to the main PHP file, or set the package root in project settings.".to_owned(),
            ),
            Self::MultipleMainFiles { .. } | Self::NotAMainFile { .. } => {
                Some("Choose the main file in project settings.".to_owned())
            }
            Self::InvalidSvnUrl { .. } => Some(
                "Use the plugin's SVN URL, for example https://plugins.svn.wordpress.org/my-plugin."
                    .to_owned(),
            ),
            Self::Io { .. } => None,
            Self::Edit(inner) => inner.fix(),
            Self::Version(inner) => inner.fix(),
        }
    }
}

/// Every PHP file in the package root (not subfolders) with a `Plugin Name:` header, sorted.
pub fn main_file_candidates(root: &Path) -> Result<Vec<String>, DetectError> {
    let io = |source| DetectError::Io {
        path: root.display().to_string(),
        source,
    };
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(root).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_php = Path::new(&name)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("php"));
        if !is_php || !entry.path().is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        if header::is_main_plugin_file(&String::from_utf8_lossy(&bytes)) {
            candidates.push(name);
        }
    }
    candidates.sort();
    Ok(candidates)
}

fn resolve_main_file(root: &Path, chosen: Option<&str>) -> Result<String, DetectError> {
    if let Some(path) = chosen {
        let ok = root.join(path).is_file()
            && edit::read_text(root, path).is_ok_and(|t| header::is_main_plugin_file(&t));
        return if ok {
            Ok(path.to_owned())
        } else {
            Err(DetectError::NotAMainFile {
                path: path.to_owned(),
            })
        };
    }
    let mut candidates = main_file_candidates(root)?;
    match candidates.len() {
        0 => Err(DetectError::NoMainFile),
        1 => Ok(candidates.remove(0)),
        _ => Err(DetectError::MultipleMainFiles { candidates }),
    }
}

/// Runs Step 1 against the package root.
pub fn detect(root: &Path, options: DetectOptions<'_>) -> Result<PluginFacts, DetectError> {
    if !root.is_dir() {
        return Err(DetectError::NotAFolder {
            path: root.display().to_string(),
        });
    }
    let slug = slug_from_svn_url(options.svn_url).ok_or_else(|| DetectError::InvalidSvnUrl {
        url: options.svn_url.to_owned(),
    })?;
    let main_file = resolve_main_file(root, options.main_file)?;
    let header = header::parse(&edit::read_text(root, &main_file)?);

    let readme = if root.join(README_FILE).is_file() {
        Some(readme::parse(&edit::read_text(root, README_FILE)?))
    } else {
        None
    };

    let versions = version::read_sources(root, &main_file, options.version_locations)?;

    let name = header
        .name
        .clone()
        .or_else(|| readme.as_ref().and_then(|r| r.name.clone()))
        .unwrap_or_else(|| slug.clone());

    let folder_matches_slug = root
        .file_name()
        .is_some_and(|n| n.to_string_lossy() == slug);

    Ok(PluginFacts {
        name,
        slug,
        main_file,
        header,
        readme,
        versions,
        folder_matches_slug,
        has_distignore: root.join(DISTIGNORE_FILE).is_file(),
        has_project_config: root.join(PROJECT_CONFIG_FILE).is_file(),
    })
}
