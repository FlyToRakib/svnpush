//! App settings (`settings.json`) and the optional WordPress version lookup
//! behind warning W02, cached for a day.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::clock;
use crate::project::{self, AppPaths, ConfigError, SCHEMA};

/// The public endpoint that reports the current WordPress release.
pub const WORDPRESS_VERSION_URL: &str = "https://api.wordpress.org/core/version-check/1.7/";
/// How long a looked-up version is trusted.
pub const WORDPRESS_VERSION_TTL_SECONDS: u64 = 24 * 60 * 60;

/// App-wide settings. The theme lives in the UI, not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AppSettings {
    /// Always 1.
    pub schema: u32,
    /// `svn` executable chosen by the developer; `None` searches.
    pub svn_path: Option<String>,
    /// `git` executable chosen by the developer; `None` searches.
    pub git_path: Option<String>,
    /// Ask api.wordpress.org for the current WordPress version (W02). On by default, opt-out.
    pub wordpress_version_lookup: bool,
    /// Provider record ids whose data-sharing notice the developer has seen.
    pub privacy_notice_seen: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            svn_path: None,
            git_path: None,
            wordpress_version_lookup: true,
            privacy_notice_seen: Vec::new(),
        }
    }
}

/// Loads settings, or the defaults.
pub fn load(paths: &AppPaths) -> Result<AppSettings, ConfigError> {
    Ok(project::read_json(&paths.settings_file())?.unwrap_or_default())
}

/// Saves settings.
pub fn save(paths: &AppPaths, settings: &AppSettings) -> Result<(), ConfigError> {
    project::write_json(&paths.settings_file(), &AppSettings { schema: SCHEMA, ..settings.clone() })
}

#[derive(Debug, Serialize, Deserialize)]
struct VersionCache {
    version: String,
    fetched_at: u64,
}

fn cache_file(paths: &AppPaths) -> PathBuf {
    paths.root().join("wordpress-version.json")
}

/// `offers[0].current` from the version-check response.
pub fn parse_version_check(json: &Value) -> Option<String> {
    json.pointer("/offers/0/current").and_then(Value::as_str).map(str::to_owned)
}

/// The current WordPress version: from the day-old cache, else fetched.
/// Returns `None` when the lookup is off or fails; W02 then reports Skip.
pub async fn current_wordpress_version(paths: &AppPaths, settings: &AppSettings) -> Option<String> {
    if !settings.wordpress_version_lookup {
        return None;
    }
    let now = clock::now();
    if let Ok(Some(cache)) = project::read_json::<VersionCache>(&cache_file(paths))
        && now.saturating_sub(cache.fetched_at) < WORDPRESS_VERSION_TTL_SECONDS
    {
        return Some(cache.version);
    }
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().ok()?;
    let json: Value = client.get(WORDPRESS_VERSION_URL).send().await.ok()?.json().await.ok()?;
    let version = parse_version_check(&json)?;
    if project::write_json(
        &cache_file(paths),
        &VersionCache { version: version.clone(), fetched_at: now },
    )
    .is_err()
    {
        tracing::warn!("could not cache the WordPress version");
    }
    Some(version)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn defaults_and_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        let mut settings = load(&paths).unwrap();
        assert!(settings.wordpress_version_lookup);
        settings.svn_path = Some("C:/tools/svn.exe".into());
        save(&paths, &settings).unwrap();
        assert_eq!(load(&paths).unwrap(), settings);
    }

    #[test]
    fn parses_the_version_check_offer() {
        let json = json!({ "offers": [{ "response": "upgrade", "current": "7.1" }] });
        assert_eq!(parse_version_check(&json).as_deref(), Some("7.1"));
        assert!(parse_version_check(&json!({})).is_none());
    }

    #[tokio::test]
    async fn uses_a_fresh_cache_and_skips_when_off() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        project::write_json(
            &cache_file(&paths),
            &VersionCache { version: "7.1".into(), fetched_at: clock::now() },
        )
        .unwrap();
        let on = AppSettings::default();
        assert_eq!(current_wordpress_version(&paths, &on).await.as_deref(), Some("7.1"));
        let off = AppSettings { wordpress_version_lookup: false, ..AppSettings::default() };
        assert!(current_wordpress_version(&paths, &off).await.is_none());
    }
}
