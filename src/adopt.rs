use crate::error::{Result, StowError};
use crate::{fs_ops, logger};
use std::fs;
use std::path::Path;

/// Adopt a conflicting file by moving it from target into the package
/// The package's version of the file is replaced
#[must_use = "adopt operations can fail and should be checked"]
pub fn adopt_file(target_file: impl AsRef<Path>, package_file: impl AsRef<Path>) -> Result<()> {
    let target_file = target_file.as_ref();
    let package_file = package_file.as_ref();

    // Verify target file exists
    if !target_file.exists() {
        return Err(StowError::invalid_path(format!(
            "Target file does not exist: {}",
            target_file.display()
        )));
    }

    // Verify target is not a symlink (we don't adopt symlinks)
    if fs_ops::is_symlink(target_file) {
        return Err(StowError::invalid_path(format!(
            "Target is already a symlink, cannot adopt: {}",
            target_file.display()
        )));
    }

    // Create parent directories in package if needed
    fs_ops::ensure_parent_dirs(package_file)?;

    // Move the file from target to package
    // Try rename first, fall back to copy+remove if it fails (e.g., cross-device)
    if let Err(rename_err) = fs::rename(target_file, package_file) {
        // Rename failed, try copy + remove as fallback
        match fs::copy(target_file, package_file) {
            Ok(_) => {
                fs::remove_file(target_file)
                    .map_err(|e| StowError::io_error(target_file.to_owned(), e))?;
            }
            Err(_) => {
                // If copy also fails, return the original rename error
                return Err(StowError::io_error(target_file.to_owned(), rename_err));
            }
        }
    }

    logger::verbose(&format!("Adopted: {}", target_file.display()));

    Ok(())
}

/// Remove a conflicting file, or a directory with all its contents, from the target
/// Symlinks are not followed; they are removed with `fs_ops::remove_symlink` instead
#[must_use = "remove operations can fail and should be checked"]
pub fn remove_path(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();

    if fs_ops::is_symlink(path) {
        return Err(StowError::invalid_path(format!(
            "Target is a symlink, refusing to remove its contents: {}",
            path.display()
        )));
    }

    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| StowError::io_error(path.to_path_buf(), e))?;
    } else {
        fs::remove_file(path).map_err(|e| StowError::io_error(path.to_path_buf(), e))?;
    }

    logger::verbose(&format!("Removed: {}", path.display()));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_adopt_file() {
        let temp_dir = TempDir::new().unwrap();
        let target_file = temp_dir.path().join("target_file.txt");
        let package_file = temp_dir.path().join("package").join("file.txt");

        // Create target file
        let mut file = File::create(&target_file).unwrap();
        file.write_all(b"test content").unwrap();

        // Adopt it
        let result = adopt_file(&target_file, &package_file);
        assert!(result.is_ok());

        // Verify file was moved
        assert!(!target_file.exists());
        assert!(package_file.exists());

        // Verify content
        let content = fs::read_to_string(&package_file).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_adopt_file_replaces_package_version() {
        let temp_dir = TempDir::new().unwrap();
        let target_file = temp_dir.path().join("target_file.txt");
        let package_file = temp_dir.path().join("package.txt");
        fs::write(&target_file, "target").unwrap();
        fs::write(&package_file, "package").unwrap();

        adopt_file(&target_file, &package_file).unwrap();

        assert!(!target_file.exists());
        assert_eq!(fs::read_to_string(&package_file).unwrap(), "target");
    }

    #[test]
    fn test_adopt_file_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let target_file = temp_dir.path().join("nonexistent.txt");
        let package_file = temp_dir.path().join("package").join("file.txt");

        // Try to adopt a file that doesn't exist
        let result = adopt_file(&target_file, &package_file);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StowError::InvalidPath(_)));
    }

    #[test]
    fn test_adopt_file_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let real_file = temp_dir.path().join("real.txt");
        let symlink_file = temp_dir.path().join("symlink.txt");
        let package_file = temp_dir.path().join("package").join("file.txt");

        // Create a real file and a symlink to it
        File::create(&real_file).unwrap();

        // Skip this test if we can't create symlinks (needs Developer Mode or admin)
        if std::os::windows::fs::symlink_file(&real_file, &symlink_file).is_err() {
            return;
        }

        // Try to adopt a symlink (should fail)
        let result = adopt_file(&symlink_file, &package_file);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StowError::InvalidPath(_)));
    }

    #[test]
    fn test_remove_path_file() {
        let temp_dir = TempDir::new().unwrap();
        let target_file = temp_dir.path().join("target_file.txt");
        File::create(&target_file).unwrap();

        let result = remove_path(&target_file);

        assert!(result.is_ok());
        assert!(!target_file.exists());
    }

    #[test]
    fn test_remove_path_directory() {
        let temp_dir = TempDir::new().unwrap();
        let target_dir = temp_dir.path().join("target_dir");
        fs::create_dir(&target_dir).unwrap();
        File::create(target_dir.join("file.txt")).unwrap();

        let result = remove_path(&target_dir);

        assert!(result.is_ok());
        assert!(!target_dir.exists());
    }

    #[test]
    fn test_remove_path_symlink_fails_and_keeps_link() {
        let temp_dir = TempDir::new().unwrap();
        let real_file = temp_dir.path().join("real.txt");
        let symlink_file = temp_dir.path().join("symlink.txt");
        File::create(&real_file).unwrap();

        // Skip this test if we can't create symlinks (needs Developer Mode or admin)
        if std::os::windows::fs::symlink_file(&real_file, &symlink_file).is_err() {
            return;
        }

        let result = remove_path(&symlink_file);

        assert!(result.is_err());
        assert!(fs_ops::is_symlink(&symlink_file));
        assert!(real_file.exists());
    }
}
