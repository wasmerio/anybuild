use std::io;
use std::path::{Path, PathBuf};

use path_clean::PathClean;

/// Make a path absolute and lexically normalize it without accessing the
/// filesystem or resolving symlinks.
pub fn normalize_absolute(path: &Path) -> io::Result<PathBuf> {
    std::path::absolute(path).map(|absolute| absolute.clean())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_absolute_makes_paths_absolute_and_cleans_components() {
        let cwd = std::env::current_dir().unwrap();

        assert_eq!(
            normalize_absolute(Path::new("parent/child/../file")).unwrap(),
            cwd.join("parent/file")
        );
    }

    #[test]
    fn normalize_absolute_rejects_an_empty_path() {
        assert!(normalize_absolute(Path::new("")).is_err());
    }
}
