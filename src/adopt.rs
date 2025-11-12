use crate::error::{Result, StowError};
use crate::{fs_ops, logger};
use std::fs;
use std::path::Path;

/// Adopt a conflicting file by moving it from target into the package
#[must_use = "adopt operations can fail and should be checked"]
pub fn adopt_file(
    target_file: impl AsRef<Path>,
    package_file: impl AsRef<Path>,
    dry_run: bool,
) -> Result<()> {
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

    logger::action(&format!(
        "Adopt file: {} -> {}",
        target_file.display(),
        package_file.display()
    ));

    if !dry_run {
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
    }

    Ok(())
}

/// Override a conflicting file by removing it from the target
pub fn override_file(target_file: impl AsRef<Path>, dry_run: bool) -> Result<()> {
    let target_file = target_file.as_ref();

    // Verify target file exists
    if !target_file.exists() {
        return Ok(()); // Already gone, nothing to do
    }

    // Verify target is not a symlink pointing to our package
    // (we don't want to remove our own symlinks)
    if fs_ops::is_symlink(target_file) {
        logger::verbose(&format!(
            "Target is a symlink, skipping override: {}",
            target_file.display()
        ));
        return Ok(());
    }

    logger::action(&format!(
        "Override (remove) file: {}",
        target_file.display()
    ));

    if !dry_run {
        if target_file.is_dir() {
            fs::remove_dir_all(target_file)
                .map_err(|e| StowError::io_error(target_file.to_path_buf(), e))?;
        } else {
            fs::remove_file(target_file)
                .map_err(|e| StowError::io_error(target_file.to_path_buf(), e))?;
        }

        logger::verbose(&format!("Removed: {}", target_file.display()));
    }

    Ok(())
}

/// Adopt a conflicting directory by moving it into the package
#[must_use = "adopt operations can fail and should be checked"]
pub fn adopt_directory(
    target_dir: impl AsRef<Path>,
    package_dir: impl AsRef<Path>,
    dry_run: bool,
) -> Result<()> {
    let target_dir = target_dir.as_ref();
    let package_dir = package_dir.as_ref();

    // Verify target directory exists
    if !target_dir.exists() {
        return Err(StowError::invalid_path(format!(
            "Target directory does not exist: {}",
            target_dir.display()
        )));
    }

    // Verify target is a real directory (not a symlink)
    if fs_ops::is_symlink(target_dir) {
        return Err(StowError::invalid_path(format!(
            "Target is already a symlink, cannot adopt: {}",
            target_dir.display()
        )));
    }

    logger::action(&format!(
        "Adopt directory: {} -> {}",
        target_dir.display(),
        package_dir.display()
    ));

    if !dry_run {
        // Create parent directories in package if needed
        fs_ops::ensure_parent_dirs(package_dir)?;

        // If package dir doesn't exist, just move the whole thing
        if !package_dir.exists() {
            // Try rename first, fall back to copy+remove if it fails (e.g., cross-device)
            if let Err(rename_err) = fs::rename(target_dir, package_dir) {
                // Rename failed, try recursive copy + remove as fallback
                match copy_dir_recursive(target_dir, package_dir) {
                    Ok(_) => {
                        fs::remove_dir_all(target_dir)
                            .map_err(|e| StowError::io_error(target_dir.to_owned(), e))?;
                    }
                    Err(_) => {
                        // If copy also fails, return the original rename error
                        return Err(StowError::io_error(target_dir.to_owned(), rename_err));
                    }
                }
            }
        } else {
            // Package dir exists, merge contents
            merge_directories(target_dir, package_dir)?;
            // Remove the now-empty target directory
            fs::remove_dir_all(target_dir)
                .map_err(|e| StowError::io_error(target_dir.to_path_buf(), e))?;
        }

        logger::verbose(&format!("Adopted: {}", target_dir.display()));
    }

    Ok(())
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| StowError::io_error(dst.to_path_buf(), e))?;

    // Collect entries first for better error messages
    let entries: Vec<_> = fs::read_dir(src)
        .map_err(|e| StowError::io_error(src.to_path_buf(), e))?
        .collect::<std::io::Result<_>>()
        .map_err(|e| StowError::io_error(src.to_path_buf(), e))?;

    for entry in entries {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| StowError::io_error(src_path, e))?;
        }
    }

    Ok(())
}

/// Merge contents of source directory into destination directory
fn merge_directories(src: &Path, dst: &Path) -> Result<()> {
    // Collect entries first for better error messages
    let entries: Vec<_> = fs::read_dir(src)
        .map_err(|e| StowError::io_error(src.to_path_buf(), e))?
        .collect::<std::io::Result<_>>()
        .map_err(|e| StowError::io_error(src.to_path_buf(), e))?;

    for entry in entries {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            if dst_path.exists() {
                merge_directories(&src_path, &dst_path)?;
            } else {
                // Try rename, fall back to copy if it fails (e.g., cross-device)
                if let Err(rename_err) = fs::rename(&src_path, &dst_path) {
                    // Rename failed, try recursive copy as fallback
                    if copy_dir_recursive(&src_path, &dst_path).is_err() {
                        // If copy also fails, return the original rename error
                        return Err(StowError::io_error(src_path, rename_err));
                    }
                }
            }
        } else {
            // Try rename, fall back to copy if it fails (e.g., cross-device)
            if let Err(rename_err) = fs::rename(&src_path, &dst_path) {
                // Rename failed, try copy as fallback
                if fs::copy(&src_path, &dst_path).is_err() {
                    // If copy also fails, return the original rename error
                    return Err(StowError::io_error(src_path, rename_err));
                }
            }
        }
    }

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
        let result = adopt_file(&target_file, &package_file, false);
        assert!(result.is_ok());

        // Verify file was moved
        assert!(!target_file.exists());
        assert!(package_file.exists());

        // Verify content
        let content = fs::read_to_string(&package_file).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_adopt_file_dry_run() {
        let temp_dir = TempDir::new().unwrap();
        let target_file = temp_dir.path().join("target_file.txt");
        let package_file = temp_dir.path().join("package").join("file.txt");

        File::create(&target_file).unwrap();

        // Dry run
        let result = adopt_file(&target_file, &package_file, true);
        assert!(result.is_ok());

        // Verify nothing changed
        assert!(target_file.exists());
        assert!(!package_file.exists());
    }

    #[test]
    fn test_override_file() {
        let temp_dir = TempDir::new().unwrap();
        let target_file = temp_dir.path().join("target_file.txt");

        File::create(&target_file).unwrap();

        let result = override_file(&target_file, false);
        assert!(result.is_ok());
        assert!(!target_file.exists());
    }

    #[test]
    fn test_override_file_dry_run() {
        let temp_dir = TempDir::new().unwrap();
        let target_file = temp_dir.path().join("target_file.txt");

        File::create(&target_file).unwrap();

        let result = override_file(&target_file, true);
        assert!(result.is_ok());
        assert!(target_file.exists());
    }

    #[test]
    fn test_override_directory() {
        let temp_dir = TempDir::new().unwrap();
        let target_dir = temp_dir.path().join("target_dir");
        fs::create_dir(&target_dir).unwrap();
        File::create(target_dir.join("file.txt")).unwrap();

        let result = override_file(&target_dir, false);
        assert!(result.is_ok());
        assert!(!target_dir.exists());
    }

    #[test]
    fn test_adopt_directory() {
        let temp_dir = TempDir::new().unwrap();
        let target_dir = temp_dir.path().join("target_dir");
        let package_dir = temp_dir.path().join("package").join("dir");

        fs::create_dir(&target_dir).unwrap();
        File::create(target_dir.join("file.txt")).unwrap();

        let result = adopt_directory(&target_dir, &package_dir, false);
        assert!(result.is_ok());

        assert!(!target_dir.exists());
        assert!(package_dir.exists());
        assert!(package_dir.join("file.txt").exists());
    }

    #[test]
    fn test_adopt_file_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let target_file = temp_dir.path().join("nonexistent.txt");
        let package_file = temp_dir.path().join("package").join("file.txt");

        // Try to adopt a file that doesn't exist
        let result = adopt_file(&target_file, &package_file, false);
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

        // On Windows, we need to use std::os::windows::fs::symlink_file
        #[cfg(target_os = "windows")]
        {
            // Skip this test if we can't create symlinks (needs Developer Mode or admin)
            if std::os::windows::fs::symlink_file(&real_file, &symlink_file).is_err() {
                return;
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            std::os::unix::fs::symlink(&real_file, &symlink_file).unwrap();
        }

        // Try to adopt a symlink (should fail)
        let result = adopt_file(&symlink_file, &package_file, false);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StowError::InvalidPath(_)));
    }

    #[test]
    fn test_override_file_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let target_file = temp_dir.path().join("nonexistent.txt");

        // Override a file that doesn't exist (should succeed as no-op)
        let result = override_file(&target_file, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_override_file_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let real_file = temp_dir.path().join("real.txt");
        let symlink_file = temp_dir.path().join("symlink.txt");

        // Create a real file and a symlink to it
        File::create(&real_file).unwrap();

        // On Windows, we need to use std::os::windows::fs::symlink_file
        #[cfg(target_os = "windows")]
        {
            // Skip this test if we can't create symlinks (needs Developer Mode or admin)
            if std::os::windows::fs::symlink_file(&real_file, &symlink_file).is_err() {
                return;
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            std::os::unix::fs::symlink(&real_file, &symlink_file).unwrap();
        }

        // Override a symlink (should skip it)
        let result = override_file(&symlink_file, false);
        assert!(result.is_ok());
        // Symlink should still exist (we don't override symlinks)
        assert!(symlink_file.exists());
    }

    #[test]
    fn test_adopt_directory_dry_run() {
        let temp_dir = TempDir::new().unwrap();
        let target_dir = temp_dir.path().join("target_dir");
        let package_dir = temp_dir.path().join("package").join("dir");

        fs::create_dir(&target_dir).unwrap();
        File::create(target_dir.join("file.txt")).unwrap();

        let result = adopt_directory(&target_dir, &package_dir, true);
        assert!(result.is_ok());

        // In dry run, nothing should change
        assert!(target_dir.exists());
        assert!(!package_dir.exists());
    }

    #[test]
    fn test_adopt_directory_nested() {
        let temp_dir = TempDir::new().unwrap();
        let target_dir = temp_dir.path().join("target_dir");
        let package_dir = temp_dir.path().join("package").join("dir");

        // Create nested structure
        fs::create_dir_all(target_dir.join("subdir")).unwrap();
        File::create(target_dir.join("file1.txt")).unwrap();
        File::create(target_dir.join("subdir/file2.txt")).unwrap();

        let result = adopt_directory(&target_dir, &package_dir, false);
        assert!(result.is_ok());

        assert!(!target_dir.exists());
        assert!(package_dir.exists());
        assert!(package_dir.join("file1.txt").exists());
        assert!(package_dir.join("subdir/file2.txt").exists());
    }
}
