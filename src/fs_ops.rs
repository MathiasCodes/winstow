use std::fs;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{ERROR_PRIVILEGE_NOT_HELD, WIN32_ERROR};
use windows::Win32::Storage::FileSystem::{
    CreateSymbolicLinkW, SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE, SYMBOLIC_LINK_FLAG_DIRECTORY,
};
use windows::core::PCWSTR;

use crate::error::{Result, StowError};

/// Create a symbolic link on Windows
/// - `link_path`: The path where the symlink will be created
/// - `target_path`: The path the symlink should point to (should be relative)
/// - `is_directory`: Whether the target is a directory
pub fn create_symlink(
    link_path: impl AsRef<Path>,
    target_path: impl AsRef<Path>,
    is_directory: bool,
) -> Result<()> {
    let link_path = link_path.as_ref();
    let target_path = target_path.as_ref();

    // Ensure parent directory exists
    if let Some(parent) = link_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| StowError::io_error(parent.to_path_buf(), e))?;
    }

    // Convert paths to wide strings for Windows API
    let link_wide = to_wide_string(link_path);
    let target_wide = to_wide_string(target_path);

    // Set up flags
    let mut flags = SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE;
    if is_directory {
        flags |= SYMBOLIC_LINK_FLAG_DIRECTORY;
    }

    // Create the symbolic link
    unsafe {
        let result = CreateSymbolicLinkW(
            PCWSTR(link_wide.as_ptr()),
            PCWSTR(target_wide.as_ptr()),
            flags,
        );

        if !result {
            // Get the last error
            let error = WIN32_ERROR(windows::Win32::Foundation::GetLastError().0);

            if error == ERROR_PRIVILEGE_NOT_HELD {
                return Err(StowError::permission_denied(format!(
                    "Cannot create symlink at {}",
                    link_path.display()
                )));
            } else {
                return Err(StowError::symlink_error(
                    link_path.to_path_buf(),
                    format!("Failed to create symlink: {:?}", error),
                ));
            }
        }
    }

    Ok(())
}

/// Check if a path is a symbolic link
#[inline]
pub fn is_symlink(path: impl AsRef<Path>) -> bool {
    match path.as_ref().symlink_metadata() {
        Ok(metadata) => metadata.file_type().is_symlink(),
        Err(_) => false,
    }
}

/// Check if a path is a directory (not a file or symlink to file)
#[inline]
pub fn is_directory(path: impl AsRef<Path>) -> Result<bool> {
    let path = path.as_ref();

    match path.metadata() {
        Ok(metadata) => Ok(metadata.is_dir()),
        Err(e) => Err(StowError::io_error(path.to_owned(), e)),
    }
}

/// Read the target of a symbolic link
#[inline]
pub fn read_symlink(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    fs::read_link(path).map_err(|e| StowError::io_error(path.to_owned(), e))
}

/// Check if a directory is empty
#[inline]
pub fn is_empty_directory(path: impl AsRef<Path>) -> Result<bool> {
    let path = path.as_ref();

    if !path.is_dir() {
        return Ok(false);
    }

    match fs::read_dir(path) {
        Ok(mut entries) => Ok(entries.next().is_none()),
        Err(e) => Err(StowError::io_error(path.to_owned(), e)),
    }
}

/// Remove an empty directory
pub fn remove_empty_directory(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();

    if !is_empty_directory(path)? {
        return Err(StowError::directory_not_empty(path.to_path_buf()));
    }

    fs::remove_dir(path).map_err(|e| StowError::io_error(path.to_path_buf(), e))
}

/// Ensure parent directories exist, creating them if necessary
pub fn ensure_parent_dirs(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();

    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| StowError::io_error(parent.to_owned(), e))?;
    }

    Ok(())
}

/// Convert a path to a null-terminated wide string for Windows API calls
fn to_wide_string(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    let path_str = path.as_os_str();
    let mut wide: Vec<u16> = path_str.encode_wide().collect();
    wide.push(0); // Null terminator
    wide
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn test_is_symlink_on_regular_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        assert!(!is_symlink(&file_path));
    }

    #[test]
    fn test_is_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let empty_dir = temp_dir.path().join("empty");
        fs::create_dir(&empty_dir).unwrap();

        assert!(is_empty_directory(&empty_dir).unwrap());

        // Add a file
        File::create(empty_dir.join("file.txt")).unwrap();
        assert!(!is_empty_directory(&empty_dir).unwrap());
    }

    #[test]
    fn test_remove_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let empty_dir = temp_dir.path().join("empty");
        fs::create_dir(&empty_dir).unwrap();

        assert!(remove_empty_directory(&empty_dir).is_ok());
        assert!(!empty_dir.exists());
    }

    #[test]
    fn test_remove_non_empty_directory_fails() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path().join("nonempty");
        fs::create_dir(&dir).unwrap();
        File::create(dir.join("file.txt")).unwrap();

        assert!(remove_empty_directory(&dir).is_err());
    }

    #[test]
    fn test_ensure_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir
            .path()
            .join("a")
            .join("b")
            .join("c")
            .join("file.txt");

        assert!(ensure_parent_dirs(&nested_path).is_ok());
        assert!(nested_path.parent().unwrap().exists());
    }

    #[test]
    fn test_is_directory() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path().join("dir");
        fs::create_dir(&dir).unwrap();
        let file = temp_dir.path().join("file.txt");
        File::create(&file).unwrap();

        assert!(is_directory(&dir).unwrap());
        assert!(!is_directory(&file).unwrap());
    }

    // Note: Actual symlink creation tests require either Developer Mode or admin privileges
    // These tests are marked as ignored and can be run manually with proper permissions
    #[test]
    #[ignore]
    fn test_create_file_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("target.txt");
        File::create(&target).unwrap();

        let link = temp_dir.path().join("link.txt");
        let result = create_symlink(&link, &target, false);

        if result.is_ok() {
            assert!(link.exists());
            assert!(is_symlink(&link));
        } else {
            // May fail if Developer Mode is not enabled
            println!(
                "Symlink creation failed (may need Developer Mode): {:?}",
                result
            );
        }
    }

    #[test]
    #[ignore]
    fn test_create_directory_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("target_dir");
        fs::create_dir(&target).unwrap();

        let link = temp_dir.path().join("link_dir");
        let result = create_symlink(&link, &target, true);

        if result.is_ok() {
            assert!(link.exists());
            assert!(is_symlink(&link));
        } else {
            println!(
                "Symlink creation failed (may need Developer Mode): {:?}",
                result
            );
        }
    }

    #[test]
    fn test_read_symlink_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent");

        // Reading a nonexistent symlink should fail
        let result = read_symlink(&nonexistent);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_symlink_regular_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        // Reading a regular file as symlink should fail
        let result = read_symlink(&file_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_empty_directory_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent");

        // Nonexistent path should return false (not a directory)
        let result = is_empty_directory(&nonexistent);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_is_empty_directory_on_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        // File should return false (not a directory)
        let result = is_empty_directory(&file_path);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_remove_empty_directory_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent");

        // Removing nonexistent directory should fail
        let result = remove_empty_directory(&nonexistent);
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_parent_dirs_already_exists() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Parent already exists
        let result = ensure_parent_dirs(&file_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_directory_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent");

        // Nonexistent path should return error
        let result = is_directory(&nonexistent);
        assert!(result.is_err());
    }
}
