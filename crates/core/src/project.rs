//! Projects: the app-level list in `projects.json`, per-project settings,
//! the optional team file `.svnpush.json`, and where everything lives on disk.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::detect::PROJECT_CONFIG_FILE;
use crate::error::Coded;
use crate::version::VersionLocation;

/// The `schema` value written into every config file.
pub const SCHEMA: u32 = 1;

/// The default assets folder, used when it exists.
pub const DEFAULT_ASSETS_FOLDER: &str = ".wordpress-org";

/// Every folder SVNpush writes under the app data directory (plan §6.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    /// Paths under `<app-data>/svnpush`.
    pub fn new(app_data: &Path) -> Self {
        Self { root: app_data.join("svnpush") }
    }

    /// The `svnpush` folder itself.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `projects.json`
    pub fn projects_file(&self) -> PathBuf {
        self.root.join("projects.json")
    }

    /// `providers.json`
    pub fn providers_file(&self) -> PathBuf {
        self.root.join("providers.json")
    }

    /// `accounts.json` (SVN account names; passwords are in the keychain).
    pub fn accounts_file(&self) -> PathBuf {
        self.root.join("accounts.json")
    }

    /// `settings.json`
    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    /// `wc/<slug>/`, the sparse working copy.
    pub fn working_copy(&self, slug: &str) -> PathBuf {
        self.root.join("wc").join(slug)
    }

    /// `builds/`
    pub fn builds(&self) -> PathBuf {
        self.root.join("builds")
    }

    /// `runs/<slug>/`
    pub fn runs(&self, slug: &str) -> PathBuf {
        self.root.join("runs").join(slug)
    }

    /// `logs/`
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
}

/// Per-project settings. Every field has a default (plan §6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ProjectSettings {
    /// Package root relative to the project folder; empty means the folder itself.
    pub package_root: String,
    /// Main plugin file relative to the project folder; `None` auto-detects.
    pub main_file: Option<String>,
    /// Extra version locations.
    pub version_locations: Vec<VersionLocation>,
    /// Paths that must be in the package besides the main file and `readme.txt`.
    pub required_paths: Vec<String>,
    /// Shell command run before Build, shown before it runs.
    pub pre_build_command: Option<String>,
    /// Assets folder relative to the project; `None` uses `.wordpress-org/` when present,
    /// an empty string means no assets folder.
    pub assets_folder: Option<String>,
    /// Whether `.phar` files may ship (check V12).
    pub allow_phar: bool,
    /// After publishing, create a local git tag `v<version>` and a release commit.
    pub post_publish_git_tag: bool,
    /// After publishing, open the plugin page.
    pub post_publish_open_page: bool,
    /// The vault SVN account's username; `None` uses the only account.
    pub svn_account: Option<String>,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            package_root: String::new(),
            main_file: None,
            version_locations: Vec::new(),
            required_paths: Vec::new(),
            pre_build_command: None,
            assets_folder: None,
            allow_phar: false,
            post_publish_git_tag: false,
            post_publish_open_page: true,
            svn_account: None,
        }
    }
}

/// The last finished release of a project, shown in the Projects list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LastRelease {
    /// The version released or rehearsed.
    pub version: String,
    /// ISO 8601 UTC time.
    pub finished: String,
    /// `Complete`, `DryRun`, `Failed`, `Cancelled`, `PublishedUnverified`.
    pub outcome: String,
}

/// One project in `projects.json`, keyed by its absolute folder path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Project {
    /// Absolute path of the project folder.
    pub path: String,
    /// Display name (the plugin name when detected).
    pub name: String,
    /// The plugin's SVN URL.
    pub svn_url: String,
    /// Slug derived from the SVN URL.
    pub slug: String,
    /// Settings.
    #[serde(default)]
    pub settings: ProjectSettings,
    /// The most recent run outcome.
    #[serde(default)]
    pub last_release: Option<LastRelease>,
    /// ISO 8601 UTC time the project was added.
    pub created: String,
}

impl Project {
    /// The package root: the project folder or a subfolder of it.
    pub fn package_root(&self) -> PathBuf {
        let base = PathBuf::from(&self.path);
        if self.settings.package_root.trim().is_empty() {
            base
        } else {
            base.join(self.settings.package_root.trim_matches(['/', '\\']))
        }
    }

    /// The assets folder in effect, if any.
    pub fn assets_folder(&self) -> Option<PathBuf> {
        let base = PathBuf::from(&self.path);
        match self.settings.assets_folder.as_deref() {
            Some("") => None,
            Some(folder) => Some(base.join(folder)),
            None => Some(base.join(DEFAULT_ASSETS_FOLDER)).filter(|p| p.is_dir()),
        }
    }

    /// The public plugin page on WordPress.org.
    pub fn plugin_page(&self) -> String {
        format!("https://wordpress.org/plugins/{}/", self.slug)
    }
}

/// The file `projects.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProjectsFile {
    schema: u32,
    projects: Vec<Project>,
}

/// A config file problem.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Reading or writing failed.
    #[error("{action} {path}: {source}")]
    Io { action: &'static str, path: String, source: std::io::Error },
    /// The JSON is malformed or has the wrong shape.
    #[error("{path} is not valid: {reason}")]
    Invalid { path: String, reason: String },
    /// The file was written by a newer SVNpush.
    #[error("{path} uses schema {found}; this SVNpush understands schema {SCHEMA}")]
    NewerSchema { path: String, found: u64 },
}

impl Coded for ConfigError {
    fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "CONFIG_IO",
            Self::Invalid { .. } => "CONFIG_INVALID",
            Self::NewerSchema { .. } => "CONFIG_NEWER_SCHEMA",
        }
    }

    fn fix(&self) -> Option<String> {
        match self {
            Self::Io { .. } => None,
            Self::Invalid { path, .. } => Some(format!("Correct or remove {path}.")),
            Self::NewerSchema { .. } => Some("Update SVNpush.".to_owned()),
        }
    }
}

/// Reads a JSON config file; a missing file is `None`.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, ConfigError> {
    let shown = path.display().to_string();
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(ConfigError::Io { action: "read", path: shown, source }),
    };
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| ConfigError::Invalid { path: shown.clone(), reason: e.to_string() })?;
    if let Some(found) =
        value.get("schema").and_then(Value::as_u64).filter(|s| *s > u64::from(SCHEMA))
    {
        return Err(ConfigError::NewerSchema { path: shown, found });
    }
    serde_json::from_value(value)
        .map(Some)
        .map_err(|e| ConfigError::Invalid { path: shown, reason: e.to_string() })
}

/// Writes a JSON config file atomically (temporary file, then rename).
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), ConfigError> {
    let shown = path.display().to_string();
    let io = |action, source| ConfigError::Io { action, path: shown.clone(), source };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io("create", e))?;
    }
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| ConfigError::Invalid { path: shown.clone(), reason: e.to_string() })?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, format!("{text}\n")).map_err(|e| io("write", e))?;
    std::fs::rename(&temp, path).map_err(|e| io("replace", e))
}

/// Loads every project.
pub fn load_projects(paths: &AppPaths) -> Result<Vec<Project>, ConfigError> {
    Ok(read_json::<ProjectsFile>(&paths.projects_file())?.map(|f| f.projects).unwrap_or_default())
}

/// Saves every project.
pub fn save_projects(paths: &AppPaths, projects: &[Project]) -> Result<(), ConfigError> {
    write_json(
        &paths.projects_file(),
        &ProjectsFile { schema: SCHEMA, projects: projects.to_vec() },
    )
}

/// Settings that belong to one machine and are ignored in `.svnpush.json`.
const MACHINE_SPECIFIC: [&str; 1] = ["svn_account"];

/// The project with `.svnpush.json` overlaid: team values override app values,
/// key by key. The file's `svn_url` overrides too; machine-specific keys are ignored.
pub fn with_team_config(project: &Project) -> Result<Project, ConfigError> {
    let file = PathBuf::from(&project.path).join(PROJECT_CONFIG_FILE);
    let Some(team) = read_json::<Value>(&file)? else {
        return Ok(project.clone());
    };
    let shown = file.display().to_string();
    let invalid = |reason: String| ConfigError::Invalid { path: shown.clone(), reason };
    let Value::Object(team) = team else {
        return Err(invalid("expected a JSON object".to_owned()));
    };

    let mut merged = project.clone();
    if let Some(url) = team.get("svn_url").and_then(Value::as_str) {
        url.clone_into(&mut merged.svn_url);
    }
    if let Some(Value::Object(overrides)) = team.get("settings") {
        let mut base =
            serde_json::to_value(&project.settings).map_err(|e| invalid(e.to_string()))?;
        if let Value::Object(fields) = &mut base {
            for (key, value) in overrides {
                if !MACHINE_SPECIFIC.contains(&key.as_str()) {
                    fields.insert(key.clone(), value.clone());
                }
            }
        }
        merged.settings = serde_json::from_value(base).map_err(|e| invalid(e.to_string()))?;
    }
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(dir: &Path) -> Project {
        Project {
            path: dir.display().to_string(),
            name: "Demo".into(),
            svn_url: "https://plugins.svn.wordpress.org/demo".into(),
            slug: "demo".into(),
            settings: ProjectSettings::default(),
            last_release: None,
            created: "2026-09-17T00:00:00Z".into(),
        }
    }

    #[test]
    fn projects_round_trip_with_schema() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        assert!(load_projects(&paths).unwrap().is_empty());
        let p = project(dir.path());
        save_projects(&paths, std::slice::from_ref(&p)).unwrap();
        let text = std::fs::read_to_string(paths.projects_file()).unwrap();
        assert!(text.contains("\"schema\": 1"));
        assert_eq!(load_projects(&paths).unwrap(), [p]);
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        std::fs::create_dir_all(paths.root()).unwrap();
        std::fs::write(paths.projects_file(), r#"{"schema": 2, "projects": []}"#).unwrap();
        let err = load_projects(&paths).unwrap_err();
        assert_eq!(err.code(), "CONFIG_NEWER_SCHEMA");
    }

    #[test]
    fn team_config_overrides_keys_but_not_machine_ones() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = project(dir.path());
        p.settings.svn_account = Some("me".into());
        p.settings.pre_build_command = Some("npm run build".into());
        std::fs::write(
            dir.path().join(".svnpush.json"),
            r#"{"schema": 1, "settings": {"required_paths": ["build"], "svn_account": "team", "post_publish_open_page": false}}"#,
        )
        .unwrap();
        let merged = with_team_config(&p).unwrap();
        assert_eq!(merged.settings.required_paths, ["build"]);
        assert!(!merged.settings.post_publish_open_page);
        assert_eq!(merged.settings.svn_account.as_deref(), Some("me"));
        assert_eq!(merged.settings.pre_build_command.as_deref(), Some("npm run build"));
    }

    #[test]
    fn assets_folder_defaults_to_wordpress_org_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = project(dir.path());
        assert!(p.assets_folder().is_none());
        std::fs::create_dir_all(dir.path().join(".wordpress-org")).unwrap();
        assert_eq!(p.assets_folder(), Some(dir.path().join(".wordpress-org")));
        p.settings.assets_folder = Some(String::new());
        assert!(p.assets_folder().is_none());
    }
}
