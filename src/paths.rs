//! Cross-platform path prefix checks.

use std::path::{Path, PathBuf};

/// Returns true if `path` is equal to `root` or is a child of `root`.
///
/// On macOS, FSEvents (and `notify`) often report paths under `/private/var`
/// while temp directories use `/var`. Strip that prefix so rule matching works.
pub fn path_is_under(path: &Path, root: &Path) -> bool {
    let path = normalize_path(path);
    let root = normalize_path(root);

    if path == root {
        return true;
    }

    let mut prefix = root.as_os_str().to_os_string();
    prefix.push(std::ffi::OsStr::new(std::path::MAIN_SEPARATOR_STR));
    path.starts_with(Path::new(&prefix))
}

fn normalize_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(stripped) = text.strip_prefix("/private") {
        return PathBuf::from(stripped);
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_under_matches_child() {
        let root = PathBuf::from("/tmp/watch");
        let file = PathBuf::from("/tmp/watch/doc.pdf");
        assert!(path_is_under(&file, &root));
    }

    #[test]
    fn path_is_under_handles_private_prefix() {
        let root = PathBuf::from("/var/tmp/watch");
        let file = PathBuf::from("/private/var/tmp/watch/doc.pdf");
        assert!(path_is_under(&file, &root));
    }

    #[test]
    fn path_is_under_rejects_unrelated_paths() {
        let root = PathBuf::from("/tmp/watch");
        let file = PathBuf::from("/tmp/other/doc.pdf");
        assert!(!path_is_under(&file, &root));
    }
}
