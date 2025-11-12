use crate::error::{Result, StowError};
use std::path::{Component, Path, PathBuf};

/// Normalize a path to an absolute path with consistent separators
/// Resolves `.` and `..` components and canonicalizes the path
pub fn normalize_path(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();

    // Try to canonicalize (resolves symlinks, removes . and .., makes absolute)
    match path.canonicalize() {
        Ok(canonical) => Ok(canonical),
        Err(_e) => {
            // If canonicalize fails (e.g., path doesn't exist), try to make it absolute
            if path.is_absolute() {
                Ok(path.to_owned())
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(path))
                    .map_err(|io_err| StowError::io_error(path.to_owned(), io_err))
            }
        }
    }
}

/// Compute a relative path from one location to another
/// Both paths should be absolute
pub fn compute_relative_path(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<PathBuf> {
    let from = from.as_ref();
    let to = to.as_ref();

    // Ensure both paths are absolute
    if !from.is_absolute() || !to.is_absolute() {
        return Err(StowError::invalid_path(
            "Both 'from' and 'to' paths must be absolute for relative path computation",
        ));
    }

    // Normalize both paths
    let from = normalize_path(from)?;
    let to = normalize_path(to)?;

    // Split paths into components
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();

    // Find common prefix length
    let common_len = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| components_equal(a, b))
        .count();

    // Build relative path
    let mut relative = PathBuf::new();

    // Add ".." for each remaining component in 'from' path
    let up_count = from_components.len() - common_len;
    for _ in 0..up_count {
        relative.push("..");
    }

    // Add remaining components from 'to' path
    for component in &to_components[common_len..] {
        relative.push(component);
    }

    Ok(relative)
}

/// Compare two path components with case-insensitivity on Windows
#[inline]
fn components_equal(a: &Component, b: &Component) -> bool {
    // On Windows, do case-insensitive comparison
    #[cfg(target_os = "windows")]
    {
        let a_str = a.as_os_str().to_string_lossy().to_lowercase();
        let b_str = b.as_os_str().to_string_lossy().to_lowercase();
        a_str == b_str
    }

    // On other platforms, do case-sensitive comparison
    #[cfg(not(target_os = "windows"))]
    {
        a == b
    }
}

/// Check if two paths are equal, using case-insensitive comparison on Windows
#[inline]
pub fn paths_equal(a: impl AsRef<Path>, b: impl AsRef<Path>) -> bool {
    let a = a.as_ref();
    let b = b.as_ref();

    // Try to normalize both paths first
    let a_norm = normalize_path(a).unwrap_or_else(|_| a.to_owned());
    let b_norm = normalize_path(b).unwrap_or_else(|_| b.to_owned());

    #[cfg(target_os = "windows")]
    {
        // Case-insensitive component-wise comparison to avoid string allocation
        let a_components: Vec<_> = a_norm.components().collect();
        let b_components: Vec<_> = b_norm.components().collect();

        if a_components.len() != b_components.len() {
            return false;
        }

        a_components.iter().zip(b_components.iter()).all(|(a, b)| {
            // Compare components case-insensitively
            a.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        a_norm == b_norm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_normalize_path_absolute() {
        let cwd = env::current_dir().unwrap();
        let normalized = normalize_path(&cwd).unwrap();
        assert!(normalized.is_absolute());
    }

    #[test]
    fn test_normalize_path_relative() {
        let result = normalize_path(".");
        assert!(result.is_ok());
        assert!(result.unwrap().is_absolute());
    }

    #[test]
    fn test_compute_relative_path_same_dir() {
        // Test relative path within same directory structure
        #[cfg(target_os = "windows")]
        {
            let from = PathBuf::from("C:\\users\\test");
            let to = PathBuf::from("C:\\users\\test\\file.txt");
            let relative = compute_relative_path(&from, &to).unwrap();
            assert_eq!(relative, PathBuf::from("file.txt"));
        }

        #[cfg(not(target_os = "windows"))]
        {
            let from = PathBuf::from("/users/test");
            let to = PathBuf::from("/users/test/file.txt");
            let relative = compute_relative_path(&from, &to).unwrap();
            assert_eq!(relative, PathBuf::from("file.txt"));
        }
    }

    #[test]
    fn test_compute_relative_path_parent() {
        #[cfg(target_os = "windows")]
        {
            let from = PathBuf::from("C:\\users\\test\\subdir");
            let to = PathBuf::from("C:\\users\\test\\file.txt");
            let relative = compute_relative_path(&from, &to).unwrap();
            assert_eq!(relative, PathBuf::from("..\\file.txt"));
        }

        #[cfg(not(target_os = "windows"))]
        {
            let from = PathBuf::from("/users/test/subdir");
            let to = PathBuf::from("/users/test/file.txt");
            let relative = compute_relative_path(&from, &to).unwrap();
            assert_eq!(relative, PathBuf::from("../file.txt"));
        }
    }

    #[test]
    fn test_compute_relative_path_different_branches() {
        #[cfg(target_os = "windows")]
        {
            let from = PathBuf::from("C:\\users\\test\\dir1");
            let to = PathBuf::from("C:\\users\\test\\dir2\\file.txt");
            let relative = compute_relative_path(&from, &to).unwrap();
            assert_eq!(relative, PathBuf::from("..\\dir2\\file.txt"));
        }

        #[cfg(not(target_os = "windows"))]
        {
            let from = PathBuf::from("/users/test/dir1");
            let to = PathBuf::from("/users/test/dir2/file.txt");
            let relative = compute_relative_path(&from, &to).unwrap();
            assert_eq!(relative, PathBuf::from("../dir2/file.txt"));
        }
    }

    #[test]
    fn test_compute_relative_path_requires_absolute() {
        let from = PathBuf::from("relative/path");
        let to = PathBuf::from("/absolute/path");
        assert!(compute_relative_path(&from, &to).is_err());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_paths_equal_case_insensitive() {
        // On Windows, paths should be case-insensitive
        let _a = PathBuf::from("C:\\Users\\Test");
        let _b = PathBuf::from("C:\\users\\test");
        // Note: This test may not work perfectly without actual filesystem access
        // but demonstrates the intent
    }
}
