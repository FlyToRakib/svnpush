//! The WordPress.org images in the assets folder (icon, banner, screenshots):
//! each file's role, its pixel size, and whether WordPress.org will use it.
//! Names, sizes, formats and size limits follow the official plugin-assets
//! handbook (developer.wordpress.org/plugins/wordpress-org/plugin-assets/).

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use ts_rs::TS;

/// What a file in the assets folder is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum AssetKind {
    /// `icon-128x128.png` or `icon-256x256.png`.
    Icon,
    /// `icon.svg`.
    IconSvg,
    /// `banner-772x250.png` or `banner-1544x500.png`, with an optional `-rtl` or language suffix.
    Banner,
    /// `screenshot-N.png`, with an optional language suffix.
    Screenshot,
    /// The `blueprints/` folder for the Playground live preview.
    Blueprints,
    /// Anything else: WordPress.org ignores it.
    Unknown,
}

/// One file or folder in the assets folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AssetFile {
    /// The file name.
    pub name: String,
    /// What it is for.
    pub kind: AssetKind,
    /// Width in pixels, when it is an image SVNpush can read.
    pub width: Option<u32>,
    /// Height in pixels.
    pub height: Option<u32>,
    /// Size in bytes.
    #[ts(type = "number")]
    pub bytes: u64,
    /// Why WordPress.org would not use it as intended; `None` when it is fine.
    pub problem: Option<String>,
}

/// The assets folder, checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AssetReport {
    /// The folder, when the project has one.
    pub folder: Option<String>,
    /// Whether it exists.
    pub exists: bool,
    /// Every entry, sorted by name.
    pub files: Vec<AssetFile>,
    /// Recommendations: a missing icon, banner or screenshots.
    pub suggestions: Vec<String>,
}

impl AssetReport {
    /// The files WordPress.org would not use as intended.
    pub fn problems(&self) -> impl Iterator<Item = &AssetFile> {
        self.files.iter().filter(|f| f.problem.is_some())
    }
}

const MB: u64 = 1024 * 1024;

// The importer (`class-import.php`) accepts any case, PNG, JPG, JPEG or GIF,
// an optional `-rtl` and then an optional locale such as `-de_DE_formal`.
const SUFFIX: &str = r"(-rtl)?(-[a-z]{2,3}(_[a-z]{2})?(_[a-z0-9]+)?)?\.(png|jpg|jpeg|gif)$";

static ICON: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)^icon-(128x128|256x256)\.(png|jpg|jpeg|gif)$").ok());
static BANNER: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(&format!(r"(?i)^banner-(772x250|1544x500){SUFFIX}")).ok());
static SCREENSHOT: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(&format!(r"(?i)^screenshot-([0-9]+){SUFFIX}")).ok());

fn matches(re: &LazyLock<Option<Regex>>, name: &str) -> Option<Vec<String>> {
    re.as_ref()?
        .captures(name)
        .map(|c| c.iter().map(|m| m.map_or_else(String::new, |m| m.as_str().to_owned())).collect())
}

/// Width and height of a PNG, GIF or JPEG, read from its header.
pub fn image_size(path: &Path) -> Option<(u32, u32)> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = Vec::new();
    file.by_ref().take(24).read_to_end(&mut head).ok()?;
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        let be32 = |i: usize| Some(u32::from_be_bytes(head.get(i..i + 4)?.try_into().ok()?));
        return Some((be32(16)?, be32(20)?));
    }
    if head.starts_with(b"GIF8") {
        let le16 =
            |i: usize| Some(u32::from(u16::from_le_bytes([*head.get(i)?, *head.get(i + 1)?])));
        return Some((le16(6)?, le16(8)?));
    }
    if head.starts_with(&[0xFF, 0xD8]) {
        return jpeg_size(&mut file);
    }
    None
}

/// Walks a JPEG's segments to the frame header, seeking past each one, so
/// large metadata (Photoshop's APP1/APP2) before it does not matter.
fn jpeg_size(file: &mut std::fs::File) -> Option<(u32, u32)> {
    let len = file.metadata().ok()?.len();
    let be16 = |b: [u8; 2]| u32::from(u16::from_be_bytes(b));
    let mut segment = [0_u8; 9];
    let mut pos: u64 = 2;
    while pos + 9 <= len {
        file.seek(SeekFrom::Start(pos)).ok()?;
        file.read_exact(&mut segment).ok()?;
        let marker = segment[1];
        if segment[0] != 0xFF {
            return None;
        }
        match marker {
            // A fill byte, or a marker without a length.
            0xFF => pos += 1,
            0x01 | 0xD0..=0xD7 => pos += 2,
            // Scan data or the end: no frame header came first.
            0xD9 | 0xDA => return None,
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                return Some((be16([segment[7], segment[8]]), be16([segment[5], segment[6]])));
            }
            _ => {
                let length = be16([segment[2], segment[3]]);
                if length < 2 {
                    return None;
                }
                pos += 2 + u64::from(length);
            }
        }
    }
    None
}

fn classify(name: &str, is_dir: bool) -> (AssetKind, Option<(u32, u32)>, u64) {
    if is_dir {
        let kind = if name == "blueprints" { AssetKind::Blueprints } else { AssetKind::Unknown };
        return (kind, None, 0);
    }
    if name.eq_ignore_ascii_case("icon.svg") {
        return (AssetKind::IconSvg, None, MB);
    }
    if let Some(c) = matches(&ICON, name) {
        return (AssetKind::Icon, parse_dims(&c[1]), MB);
    }
    if let Some(c) = matches(&BANNER, name) {
        return (AssetKind::Banner, parse_dims(&c[1]), 4 * MB);
    }
    if matches(&SCREENSHOT, name).is_some() {
        return (AssetKind::Screenshot, None, 10 * MB);
    }
    (AssetKind::Unknown, None, 0)
}

fn parse_dims(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.split_once(['x', 'X'])?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

fn unknown_problem(is_dir: bool) -> String {
    if is_dir {
        "WordPress.org ignores folders here, except blueprints/.".to_owned()
    } else {
        "WordPress.org ignores this file. Use a name such as icon-256x256.png, banner-772x250.png or screenshot-1.png.".to_owned()
    }
}

fn inspect_file(folder: &Path, name: &str, is_dir: bool) -> AssetFile {
    let path = folder.join(name);
    let bytes = if is_dir { 0 } else { std::fs::metadata(&path).map_or(0, |m| m.len()) };
    let (kind, expected, limit) = classify(name, is_dir);
    let size = matches!(kind, AssetKind::Icon | AssetKind::Banner | AssetKind::Screenshot)
        .then(|| image_size(&path))
        .flatten();
    let problem = match (kind, expected, size) {
        (AssetKind::Unknown, ..) => Some(unknown_problem(is_dir)),
        (_, Some((w, h)), Some((aw, ah))) if (w, h) != (aw, ah) => {
            Some(format!("It is {aw}×{ah} pixels; it must be exactly {w}×{h}."))
        }
        (AssetKind::Icon | AssetKind::Banner | AssetKind::Screenshot, _, None) => {
            Some("This is not a PNG, JPG or GIF image SVNpush can read.".to_owned())
        }
        _ if limit > 0 && bytes > limit => {
            Some(format!("It is larger than the {} MB WordPress.org allows.", limit / MB))
        }
        _ => None,
    };
    AssetFile {
        name: name.to_owned(),
        kind,
        width: size.map(|s| s.0),
        height: size.map(|s| s.1),
        bytes,
        problem,
    }
}

/// Checks every entry in `folder`. `readme_screenshots` is the number of
/// captions in the readme's Screenshots section, when known.
pub fn inspect(folder: Option<&Path>, readme_screenshots: Option<usize>) -> AssetReport {
    let Some(folder) = folder else {
        return AssetReport {
            folder: None,
            exists: false,
            files: Vec::new(),
            suggestions: Vec::new(),
        };
    };
    let shown = folder.display().to_string();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return AssetReport {
            folder: Some(shown),
            exists: false,
            files: Vec::new(),
            suggestions: Vec::new(),
        };
    };
    let mut files: Vec<AssetFile> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            (!name.starts_with('.')).then(|| inspect_file(folder, &name, e.path().is_dir()))
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));

    let has = |kind: AssetKind| files.iter().any(|f| f.kind == kind && f.problem.is_none());
    let mut suggestions = Vec::new();
    if !has(AssetKind::Icon) {
        if has(AssetKind::IconSvg) {
            suggestions
                .push("Add icon-256x256.png as well: icon.svg needs a PNG fallback.".to_owned());
        } else {
            suggestions.push(
                "Add an icon: icon-256x256.png (256×256) and icon-128x128.png (128×128)."
                    .to_owned(),
            );
        }
    }
    if !has(AssetKind::Banner) {
        suggestions.push(
            "Add a banner: banner-1544x500.png (1544×500) and banner-772x250.png (772×250)."
                .to_owned(),
        );
    }
    // Default-language screenshots only: `screenshot-1.png`, not `screenshot-1-de.png`.
    let shots = files
        .iter()
        .filter(|f| {
            f.kind == AssetKind::Screenshot
                && f.problem.is_none()
                && f.name.matches('-').count() == 1
        })
        .count();
    if let Some(captions) = readme_screenshots.filter(|c| *c != shots) {
        suggestions.push(format!("readme.txt has {captions} screenshot caption(s) and the folder has {shots} screenshot(s). Add one screenshot-N file per caption."));
    }
    AssetReport { folder: Some(shown), exists: true, files, suggestions }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(w.to_be_bytes());
        bytes.extend(h.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0]);
        bytes
    }

    fn gif(w: u16, h: u16) -> Vec<u8> {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend(w.to_le_bytes());
        bytes.extend(h.to_le_bytes());
        bytes
    }

    fn jpg(w: u16, h: u16) -> Vec<u8> {
        let mut bytes =
            vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08];
        bytes.extend(h.to_be_bytes());
        bytes.extend(w.to_be_bytes());
        bytes.extend([0; 12]);
        bytes
    }

    #[test]
    fn reads_png_gif_and_jpeg_sizes() {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes, size) in [
            ("a.png", png(772, 250), (772, 250)),
            ("b.gif", gif(128, 128), (128, 128)),
            ("c.jpg", jpg(1544, 500), (1544, 500)),
        ] {
            std::fs::write(dir.path().join(name), bytes).unwrap();
            assert_eq!(image_size(&dir.path().join(name)), Some(size), "{name}");
        }
        std::fs::write(dir.path().join("x.png"), b"nope").unwrap();
        assert_eq!(image_size(&dir.path().join("x.png")), None);
    }

    #[test]
    fn reads_jpeg_sizes_after_large_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let mut bytes = vec![0xFF, 0xD8];
        for _ in 0..6 {
            bytes.extend([0xFF, 0xE2, 0xFF, 0xFF]);
            bytes.extend(vec![0; 0xFFFF - 2]);
        }
        bytes.extend(&jpg(772, 250)[2..]);
        let path = dir.path().join("big.jpg");
        std::fs::write(&path, &bytes).unwrap();
        assert!(bytes.len() > 256 * 1024);
        assert_eq!(image_size(&path), Some((772, 250)));
        std::fs::write(&path, &bytes[..bytes.len() - 20]).unwrap();
        assert_eq!(image_size(&path), None);
    }

    #[test]
    fn names_sizes_and_suggestions_follow_the_handbook() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(d.join("icon-256x256.png"), png(256, 256)).unwrap();
        std::fs::write(d.join("banner-772x250.png"), png(800, 300)).unwrap();
        std::fs::write(d.join("banner-772x250-rtl.jpg"), jpg(772, 250)).unwrap();
        std::fs::write(d.join("screenshot-1.png"), png(1200, 900)).unwrap();
        std::fs::write(d.join("Screenshot-2.png"), png(1200, 900)).unwrap();
        std::fs::write(d.join("logo.png"), png(10, 10)).unwrap();
        std::fs::write(d.join("screenshot-3.jpeg"), jpg(10, 10)).unwrap();
        std::fs::create_dir(d.join("blueprints")).unwrap();
        std::fs::write(d.join(".DS_Store"), b"x").unwrap();

        let report = inspect(Some(d), Some(2));
        let problem = |name: &str| {
            report.files.iter().find(|f| f.name == name).and_then(|f| f.problem.clone())
        };
        assert_eq!(problem("icon-256x256.png"), None);
        assert_eq!(problem("banner-772x250-rtl.jpg"), None);
        assert_eq!(problem("screenshot-1.png"), None);
        assert_eq!(problem("blueprints"), None);
        assert!(problem("banner-772x250.png").unwrap().contains("800×300"));
        // The importer's rules: any case, .jpeg and GIF, -rtl then a locale.
        assert_eq!(problem("Screenshot-2.png"), None);
        assert_eq!(problem("screenshot-3.jpeg"), None);
        assert!(problem("logo.png").unwrap().contains("ignores"));
        assert!(!report.files.iter().any(|f| f.name == ".DS_Store"));
        assert_eq!(report.problems().count(), 2);
        assert!(report.suggestions.iter().any(|s| s.contains("2 screenshot caption(s)")));
        assert!(!report.suggestions.iter().any(|s| s.contains("Add an icon")));
    }

    #[test]
    fn names_follow_the_importer_pattern() {
        for name in [
            "banner-772x250.gif",
            "banner-1544x500.JPEG",
            "banner-772x250-rtl-he_IL.png",
            "screenshot-1-rtl.png",
            "screenshot-2-de_DE_formal.jpg",
            "ICON-128X128.PNG",
            "Icon.svg",
        ] {
            assert_ne!(classify(name, false).0, AssetKind::Unknown, "{name}");
        }
        assert_eq!(classify("ICON-128X128.PNG", false).1, Some((128, 128)));
        for name in ["screenshot-1.webp", "banner-772x250-de-rtl.png", "banner-100x100.png"] {
            assert_eq!(classify(name, false).0, AssetKind::Unknown, "{name}");
        }
    }

    #[test]
    fn a_missing_folder_is_reported_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let report = inspect(Some(&dir.path().join(".wordpress-org")), None);
        assert!(!report.exists && report.files.is_empty());
        assert!(!inspect(None, None).exists);
    }
}
