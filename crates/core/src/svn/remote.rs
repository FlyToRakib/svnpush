//! Server operations: commit, tag, list, cat, and post-publish verification.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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

/// Log messages compare equal whatever their line endings and outer blank space.
fn normalise(message: &str) -> String {
    message.replace("\r\n", "\n").trim().to_owned()
}

/// The revision and message of the first entry of `svn log --xml`.
fn first_log_entry(xml: &str) -> Result<Option<(u64, String)>, SvnError> {
    let doc =
        roxmltree::Document::parse(xml).map_err(|e| SvnError::Parse { detail: e.to_string() })?;
    let Some(entry) = doc.descendants().find(|n| n.has_tag_name("logentry")) else {
        return Ok(None);
    };
    let revision = entry
        .attribute("revision")
        .and_then(|r| r.parse().ok())
        .ok_or_else(|| SvnError::Parse { detail: "log entry without a revision".to_owned() })?;
    let message = entry
        .children()
        .find(|c| c.has_tag_name("msg"))
        .and_then(|m| m.text())
        .unwrap_or_default()
        .to_owned();
    Ok(Some((revision, message)))
}

/// Entry names in `svn list --xml` output; folders end with `/`.
fn list_entries(xml: &str) -> Result<Vec<String>, SvnError> {
    let doc =
        roxmltree::Document::parse(xml).map_err(|e| SvnError::Parse { detail: e.to_string() })?;
    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("entry"))
        .filter_map(|entry| {
            let name = entry.children().find(|c| c.has_tag_name("name"))?.text()?;
            let slash = if entry.attribute("kind") == Some("dir") { "/" } else { "" };
            Some(format!("{name}{slash}"))
        })
        .collect())
}

/// A commit message in a UTF-8 file for `--file … --encoding UTF-8`. On
/// Windows, `--message` passes the text through the ANSI code page, which
/// corrupts other characters in the log for good.
struct MessageFile(PathBuf);

impl MessageFile {
    fn write(message: &str) -> Result<Self, SvnError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("svnpush-message-{}-{n}.txt", std::process::id()));
        std::fs::write(&path, message).map_err(|e| super::io_error("write", &path, e))?;
        Ok(Self(path))
    }

    fn args(&self) -> [OsString; 4] {
        ["--file".into(), self.0.clone().into_os_string(), "--encoding".into(), "UTF-8".into()]
    }
}

impl Drop for MessageFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl Svn<'_> {
    /// The revision `url` last changed in; `None` when it does not exist.
    pub async fn last_changed_revision(
        &self,
        url: &str,
        credentials: Option<&Credentials>,
    ) -> Result<Option<u64>, SvnError> {
        let args =
            vec!["info".into(), "--show-item".into(), "last-changed-revision".into(), url.into()];
        match self.network(args, credentials, false).await {
            Ok(output) => output.stdout.trim().parse().map(Some).map_err(|_| SvnError::Parse {
                detail: format!("unexpected revision {:?}", output.stdout.trim()),
            }),
            Err(SvnError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The revision of a commit with `message` under `url`, newer than
    /// `since` when that is known: whether a commit that reported an error,
    /// or was interrupted, reached the server. Reads `svn log --xml`, which
    /// does not depend on the language of `svn`'s messages.
    pub async fn find_commit(
        &self,
        url: &str,
        since: Option<u64>,
        message: &str,
        credentials: Option<&Credentials>,
    ) -> Result<Option<u64>, SvnError> {
        let args = vec!["log".into(), "--xml".into(), "--limit".into(), "1".into(), url.into()];
        let output = self.network(args, credentials, false).await?;
        let Some((revision, found)) = first_log_entry(&output.stdout)? else { return Ok(None) };
        let ours = normalise(&found) == normalise(message) && since.is_none_or(|s| revision > s);
        Ok(ours.then_some(revision))
    }

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
    /// Cancel does not interrupt a commit once it has started.
    pub async fn commit_paths(
        &self,
        paths: &[PathBuf],
        message: &str,
        credentials: &Credentials,
    ) -> Result<Option<u64>, SvnError> {
        let file = MessageFile::write(message)?;
        let mut args: Vec<OsString> = vec!["commit".into()];
        args.extend(file.args());
        args.extend(paths.iter().map(|p| p.as_os_str().to_owned()));
        let output = self.shielded().network(args, Some(credentials), true).await?;
        if output.stdout.trim().is_empty() {
            return Ok(None);
        }
        if let Some(revision) = committed_revision(&output.stdout) {
            return Ok(Some(revision));
        }
        // Localised output (Windows builds may ignore LC_MESSAGES): ask the server.
        let no_revision = || SvnError::Parse { detail: "no revision in commit output".to_owned() };
        let root = paths.first().and_then(|p| p.parent()).ok_or_else(no_revision)?;
        let url = self.working_copy_url(root).await?;
        self.find_commit(&url, None, message, Some(credentials))
            .await?
            .map(Some)
            .ok_or_else(no_revision)
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
        let tag_url = join_url(url, &format!("tags/{version}"));
        let file = MessageFile::write(message)?;
        let mut args: Vec<OsString> = vec!["copy".into(), trunk.into(), tag_url.clone().into()];
        args.extend(file.args());
        // Cancel does not interrupt a copy once it has started.
        let output = self.shielded().network(args, Some(credentials), true).await?;
        if let Some(revision) = committed_revision(&output.stdout) {
            return Ok(revision);
        }
        self.last_changed_revision(&tag_url, Some(credentials))
            .await?
            .ok_or_else(|| SvnError::Parse { detail: "no revision in copy output".to_owned() })
    }

    /// Entry names directly under `url`; folders end with `/`.
    pub async fn list(
        &self,
        url: &str,
        credentials: Option<&Credentials>,
    ) -> Result<Vec<String>, SvnError> {
        // XML is always UTF-8; plain output is in the console code page on Windows.
        let args = vec!["list".into(), "--xml".into(), url.into()];
        let output = self.network(args, credentials, false).await?;
        list_entries(&output.stdout)
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
    fn lists_names_with_folder_slashes() {
        let xml = "<?xml version=\"1.0\"?>\n<lists><list path=\"x\">\n<entry kind=\"dir\"><name>inc</name></entry>\n<entry kind=\"file\"><name>日本.php</name><size>1</size></entry>\n</list></lists>";
        assert_eq!(list_entries(xml).unwrap(), ["inc/", "日本.php"]);
    }

    #[test]
    fn reads_the_first_log_entry() {
        let xml = "<?xml version=\"1.0\"?>\n<log>\n<logentry revision=\"7\">\n<author>a</author>\n<msg>Release 1.0.1\n</msg>\n</logentry>\n</log>";
        let (revision, message) = first_log_entry(xml).unwrap().unwrap();
        assert_eq!(revision, 7);
        assert_eq!(normalise(&message), normalise("Release 1.0.1\r\n"));
        assert!(first_log_entry("<log></log>").unwrap().is_none());
    }

    #[test]
    fn joins_urls() {
        assert_eq!(join_url("https://x/p/", "/tags/1.0"), "https://x/p/tags/1.0");
    }
}
