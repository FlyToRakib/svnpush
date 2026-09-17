//! Server operations: commit, tag, list, cat, and post-publish verification.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::readme;

use super::wc::DEEP_FOLDERS;
use super::{Credentials, Svn, SvnError};

/// How often, and how far apart, tag verification is retried (plan §12.3).
pub const VERIFY_ATTEMPTS: u32 = 3;
/// Delay between verification attempts: three tries over thirty seconds.
pub const VERIFY_DELAY: Duration = Duration::from_secs(10);

/// The result of checking a freshly created tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", content = "reason")]
#[ts(export)]
pub enum TagVerification {
    /// The tag lists the main file and its readme carries the right stable tag.
    Verified,
    /// The tag exists but could not be confirmed; the plugin page should be checked.
    Unverified(String),
}

/// The revision number in `Committed revision N.`
fn committed_revision(stdout: &str) -> Option<u64> {
    stdout.lines().rev().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        let rest = &lower[lower.find("committed revision")? + "committed revision".len()..];
        rest.trim().trim_end_matches('.').parse().ok()
    })
}

fn join_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

impl Svn<'_> {
    /// Commits `trunk/` and `assets/` with `message`. `None` when nothing changed.
    pub async fn commit(
        &self,
        wc: &Path,
        message: &str,
        credentials: &Credentials,
    ) -> Result<Option<u64>, SvnError> {
        let folders: Vec<PathBuf> =
            DEEP_FOLDERS.iter().map(|f| wc.join(f)).filter(|p| p.is_dir()).collect();
        self.commit_paths(&folders, message, credentials).await
    }

    /// Commits only the given working-copy folders (an assets-only release).
    pub async fn commit_paths(
        &self,
        paths: &[PathBuf],
        message: &str,
        credentials: &Credentials,
    ) -> Result<Option<u64>, SvnError> {
        let mut args: Vec<OsString> = vec!["commit".into(), "--message".into(), message.into()];
        args.extend(paths.iter().map(|p| p.as_os_str().to_owned()));
        let output = self.network(args, Some(credentials), true).await?;
        if output.stdout.trim().is_empty() {
            return Ok(None);
        }
        committed_revision(&output.stdout)
            .map(Some)
            .ok_or_else(|| SvnError::Parse { detail: "no revision in commit output".to_owned() })
    }

    /// Server-side copy of `trunk` (at `revision` when known) to `tags/<version>`.
    pub async fn tag(
        &self,
        url: &str,
        version: &str,
        revision: Option<u64>,
        message: &str,
        credentials: &Credentials,
    ) -> Result<u64, SvnError> {
        // `svn copy` into an existing folder nests the copy inside it
        // (tags/1.0.0/trunk) instead of failing, so refuse explicitly.
        if self.list_tags(url, Some(credentials)).await?.iter().any(|t| t == version) {
            return Err(SvnError::TagExists { version: version.to_owned() });
        }
        let trunk = match revision {
            Some(rev) => format!("{}@{rev}", join_url(url, "trunk")),
            None => join_url(url, "trunk"),
        };
        let args: Vec<OsString> = vec![
            "copy".into(),
            trunk.into(),
            join_url(url, &format!("tags/{version}")).into(),
            "--message".into(),
            message.into(),
        ];
        let output = self.network(args, Some(credentials), true).await?;
        committed_revision(&output.stdout)
            .ok_or_else(|| SvnError::Parse { detail: "no revision in copy output".to_owned() })
    }

    /// Entry names directly under `url`; folders end with `/`.
    pub async fn list(
        &self,
        url: &str,
        credentials: Option<&Credentials>,
    ) -> Result<Vec<String>, SvnError> {
        let output = self.network(vec!["list".into(), url.into()], credentials, false).await?;
        Ok(output
            .stdout
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect())
    }

    /// Folder names under `tags/`, without the trailing `/`.
    pub async fn list_tags(
        &self,
        url: &str,
        credentials: Option<&Credentials>,
    ) -> Result<Vec<String>, SvnError> {
        Ok(self
            .list(&join_url(url, "tags"), credentials)
            .await?
            .into_iter()
            .filter(|e| e.ends_with('/'))
            .map(|e| e.trim_end_matches('/').to_owned())
            .collect())
    }

    /// The content of a file on the server.
    pub async fn cat(
        &self,
        url: &str,
        credentials: Option<&Credentials>,
    ) -> Result<String, SvnError> {
        Ok(self.network(vec!["cat".into(), url.into()], credentials, false).await?.stdout)
    }

    /// Confirms `tags/<version>/` holds the main file and a readme whose stable
    /// tag is the version. Retries to ride out replication lag.
    pub async fn verify_tag(
        &self,
        url: &str,
        version: &str,
        main_file: &str,
        credentials: Option<&Credentials>,
        delay: Duration,
    ) -> Result<TagVerification, SvnError> {
        let tag_url = join_url(url, &format!("tags/{version}"));
        let mut reason = String::new();
        for attempt in 1..=VERIFY_ATTEMPTS {
            if attempt > 1 {
                self.reporter
                    .info(&format!("Verifying the tag again ({attempt} of {VERIFY_ATTEMPTS})."));
                tokio::select! {
                    () = self.cancel.cancelled() => return Err(SvnError::Cancelled),
                    () = tokio::time::sleep(delay) => {}
                }
            }
            reason = match self.check_tag(&tag_url, version, main_file, credentials).await {
                Ok(None) => return Ok(TagVerification::Verified),
                Ok(Some(why)) => why,
                Err(SvnError::Cancelled) => return Err(SvnError::Cancelled),
                Err(other) => other.to_string(),
            };
        }
        Ok(TagVerification::Unverified(reason))
    }

    async fn check_tag(
        &self,
        tag_url: &str,
        version: &str,
        main_file: &str,
        credentials: Option<&Credentials>,
    ) -> Result<Option<String>, SvnError> {
        let entries = self.list(&format!("{tag_url}/"), credentials).await?;
        if !entries.iter().any(|e| e == main_file) {
            return Ok(Some(format!("{main_file} is not listed in the tag yet.")));
        }
        let text = self.cat(&join_url(tag_url, readme::README_FILE), credentials).await?;
        let stable = readme::parse(&text).header("Stable tag").map(|h| h.value.clone());
        Ok(match stable {
            Some(tag) if tag == version => None,
            Some(tag) => Some(format!("The tagged readme says Stable tag: {tag}.")),
            None => Some("The tagged readme has no Stable tag.".to_owned()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_committed_revision() {
        let out = "Sending        trunk/a.php\nTransmitting file data .done\nCommitting transaction...\nCommitted revision 42.\n";
        assert_eq!(committed_revision(out), Some(42));
        assert_eq!(committed_revision("nothing"), None);
    }

    #[test]
    fn joins_urls() {
        assert_eq!(join_url("https://x/p/", "/tags/1.0"), "https://x/p/tags/1.0");
    }
}
