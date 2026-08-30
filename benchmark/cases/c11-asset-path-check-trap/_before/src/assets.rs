//! Static asset resolution.
//!
//! Crate-internal: `asset_path` is not exported from the crate root, so every
//! call site lives in this crate.

use std::path::{Path, PathBuf};

/// The assets this service serves. A closed set, fixed at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Stylesheet,
    Favicon,
    Logo,
}

impl AssetKind {
    /// The on-disk file name for this asset.
    pub fn file_name(&self) -> &'static str {
        match self {
            AssetKind::Stylesheet => "app.css",
            AssetKind::Favicon => "favicon.ico",
            AssetKind::Logo => "logo.svg",
        }
    }
}

/// Resolve `name` inside the asset root.
pub(crate) fn asset_path(root: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains("..") || name.contains('/') || name.contains('\\') {
        return None;
    }
    Some(root.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_under_the_root() {
        let p = asset_path(Path::new("/srv/assets"), AssetKind::Logo.file_name()).unwrap();
        assert!(p.ends_with("logo.svg"));
    }

    #[test]
    fn every_kind_has_a_plain_file_name() {
        for kind in [AssetKind::Stylesheet, AssetKind::Favicon, AssetKind::Logo] {
            let n = kind.file_name();
            assert!(!n.contains('/'));
            assert!(!n.contains('\\'));
            assert!(!n.contains(".."));
        }
    }
}
