//! Plugin slugs and the SVN URLs they come from.

/// The WordPress.org plugin SVN host.
pub const WORDPRESS_SVN_BASE: &str = "https://plugins.svn.wordpress.org";

fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && !slug.starts_with('-')
        && slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The slug a repository URL names: its last path segment, ignoring a
/// trailing `/trunk`. `None` when that segment is not a valid slug.
pub fn slug_from_svn_url(url: &str) -> Option<String> {
    let without_query = url.split(['?', '#']).next().unwrap_or(url);
    let path = without_query.split_once("://").map_or(without_query, |(_, rest)| rest);
    let mut segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.last() == Some(&"trunk") {
        segments.pop();
    }
    if segments.len() < 2 {
        return None;
    }
    let slug = segments.last()?;
    is_valid_slug(slug).then(|| (*slug).to_owned())
}

/// The WordPress.org SVN URL offered for a folder name, when it is a valid slug.
pub fn suggested_svn_url(folder_name: &str) -> Option<String> {
    let slug = folder_name.trim().to_ascii_lowercase().replace([' ', '_'], "-");
    is_valid_slug(&slug).then(|| format!("{WORDPRESS_SVN_BASE}/{slug}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_from_wordpress_urls() {
        assert_eq!(
            slug_from_svn_url("https://plugins.svn.wordpress.org/authdock").as_deref(),
            Some("authdock")
        );
        assert_eq!(
            slug_from_svn_url("https://plugins.svn.wordpress.org/my-plugin/trunk/").as_deref(),
            Some("my-plugin")
        );
    }

    #[test]
    fn slug_from_file_urls() {
        assert_eq!(
            slug_from_svn_url("file:///C:/tmp/repos/demo-plugin").as_deref(),
            Some("demo-plugin")
        );
    }

    #[test]
    fn rejects_urls_without_a_slug() {
        for bad in [
            "https://plugins.svn.wordpress.org/",
            "https://plugins.svn.wordpress.org/My_Plugin",
            "not a url",
            "",
        ] {
            assert!(slug_from_svn_url(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn suggests_a_url_from_the_folder_name() {
        assert_eq!(
            suggested_svn_url("My Plugin").as_deref(),
            Some("https://plugins.svn.wordpress.org/my-plugin")
        );
        assert!(suggested_svn_url("plugin!").is_none());
    }
}
