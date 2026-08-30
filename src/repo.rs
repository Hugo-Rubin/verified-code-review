//! Sandboxed access to a benchmark repository.
//!
//! Every filesystem read performed on behalf of an agent goes through
//! `RepoRoot`. Paths are resolved against the root and rejected if they escape
//! it. This is the boundary that keeps a model-supplied path from reading
//! `~/.ssh/id_rsa` or the case's own `ground_truth.json`.

use anyhow::{bail, Context, Result};
use std::path::{Component, Path, PathBuf};

/// Files an agent must never read, regardless of where they sit.
///
/// Ground truth lives outside the repository directory, so this is belt and
/// braces — but a case author who misplaces a file should not silently leak
/// the answers.
const DENIED_FILE_NAMES: &[&str] = &["ground_truth.json"];

#[derive(Debug, Clone)]
pub struct RepoRoot {
    root: PathBuf,
}

impl RepoRoot {
    /// Canonicalize `root` and treat it as the sandbox boundary.
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let canonical = std::fs::canonicalize(root)
            .with_context(|| format!("repository root does not exist: {}", root.display()))?;
        if !canonical.is_dir() {
            bail!(
                "repository root is not a directory: {}",
                canonical.display()
            );
        }
        Ok(Self { root: canonical })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Resolve a repository-relative path to an absolute path inside the root.
    ///
    /// Rejects absolute paths, parent traversal, Windows drive prefixes, and
    /// symlinks that point outside the root.
    pub fn resolve(&self, relative: &str) -> Result<PathBuf> {
        let relative = relative.replace('\\', "/");
        let relative = relative.trim_start_matches("./");

        if relative.is_empty() {
            bail!("empty path");
        }

        let candidate = Path::new(relative);

        // Reject anything that is not a plain sequence of normal components.
        // This catches `/etc/passwd`, `C:\Windows`, `..`, and `//server/share`
        // before any filesystem call happens.
        for component in candidate.components() {
            match component {
                Component::Normal(_) => {}
                Component::CurDir => {}
                Component::ParentDir => {
                    bail!("path escapes the repository root: {relative}")
                }
                Component::RootDir | Component::Prefix(_) => {
                    bail!("absolute paths are not allowed: {relative}")
                }
            }
        }

        if let Some(name) = candidate.file_name().and_then(|n| n.to_str()) {
            if DENIED_FILE_NAMES.contains(&name) {
                bail!("access to {name} is not permitted");
            }
        }

        let joined = self.root.join(candidate);

        // Re-check after canonicalization so a symlink pointing outside the
        // root is caught too. A path that does not exist yet cannot be
        // canonicalized, so fall back to the lexical check above.
        match std::fs::canonicalize(&joined) {
            Ok(canonical) => {
                if !canonical.starts_with(&self.root) {
                    bail!("path escapes the repository root: {relative}");
                }
                Ok(canonical)
            }
            Err(_) => Ok(joined),
        }
    }

    /// Read a file inside the sandbox, as UTF-8 with lossy replacement.
    pub fn read_to_string(&self, relative: &str) -> Result<String> {
        let path = self.resolve(relative)?;
        let bytes = std::fs::read(&path).with_context(|| format!("reading {}", relative))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// List every file in the repository, as repository-relative POSIX paths,
    /// sorted for determinism. Skips `target/` and `.git/`.
    pub fn list_files(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out)?;
        out.sort();
        Ok(out)
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    let entries = std::fs::read_dir(dir).with_context(|| format!("listing {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if name == ".git" || name == "target" {
            continue;
        }

        if path.is_dir() {
            walk(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, RepoRoot) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        std::fs::create_dir_all(dir.path().join("target")).unwrap();
        std::fs::write(dir.path().join("target/junk.rs"), "junk").unwrap();
        let root = RepoRoot::new(dir.path()).unwrap();
        (dir, root)
    }

    #[test]
    fn resolves_normal_relative_path() {
        let (_d, root) = fixture();
        let p = root.resolve("src/lib.rs").unwrap();
        assert!(p.starts_with(root.path()));
    }

    #[test]
    fn accepts_windows_separators() {
        let (_d, root) = fixture();
        assert!(root.resolve("src\\lib.rs").is_ok());
    }

    #[test]
    fn rejects_parent_traversal() {
        let (_d, root) = fixture();
        for bad in ["../secret", "src/../../secret", "./../../etc/passwd", ".."] {
            assert!(root.resolve(bad).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn rejects_absolute_paths() {
        let (_d, root) = fixture();
        for bad in ["/etc/passwd", "C:\\Windows\\System32", "//server/share/x"] {
            assert!(root.resolve(bad).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn rejects_ground_truth_by_name() {
        let (_d, root) = fixture();
        assert!(root.resolve("ground_truth.json").is_err());
        assert!(root.resolve("src/ground_truth.json").is_err());
    }

    #[test]
    fn rejects_empty_path() {
        let (_d, root) = fixture();
        assert!(root.resolve("").is_err());
        assert!(root.resolve("./").is_err());
    }

    #[test]
    fn reads_file_contents() {
        let (_d, root) = fixture();
        assert_eq!(root.read_to_string("src/lib.rs").unwrap(), "fn main() {}\n");
    }

    #[test]
    fn read_of_traversal_path_fails() {
        let (_d, root) = fixture();
        assert!(root.read_to_string("../../../etc/passwd").is_err());
    }

    #[test]
    fn listing_skips_target_and_is_sorted() {
        let (_d, root) = fixture();
        let files = root.list_files().unwrap();
        assert_eq!(
            files,
            vec!["Cargo.toml".to_string(), "src/lib.rs".to_string()]
        );
    }

    #[test]
    fn missing_root_is_an_error() {
        assert!(RepoRoot::new("definitely/not/a/real/path").is_err());
    }
}
