use crate::error::{Result, StowError};
use crate::{fs_ops, logger, path_utils, planner};
use std::fs;
use std::path::{Path, PathBuf};

/// Unstow operation manager
pub struct Unstower {
    stow_dir: PathBuf,
    target_dir: PathBuf,
    dry_run: bool,
}

impl Unstower {
    /// Create a new Unstower from a StowContext
    pub fn from_context(context: &crate::config::StowContext) -> Self {
        Self {
            stow_dir: context.stow_dir().to_owned(),
            target_dir: context.target_dir().to_owned(),
            dry_run: context.is_dry_run(),
        }
    }

    /// Create a new Unstower (legacy constructor for backward compatibility)
    #[cfg(test)]
    pub fn new(
        stow_dir: impl Into<PathBuf>,
        target_dir: impl Into<PathBuf>,
        _verbose: bool,
        dry_run: bool,
    ) -> Self {
        Self {
            stow_dir: stow_dir.into(),
            target_dir: target_dir.into(),
            dry_run,
        }
    }

    /// Unstow a package
    #[must_use = "unstow operations can fail and should be checked"]
    pub fn unstow_package(&self, package_name: &str) -> Result<()> {
        let package_path = self.stow_dir.join(package_name);

        // Verify package exists
        if !package_path.exists() {
            return Err(StowError::package_not_found(package_name, &self.stow_dir));
        }

        logger::verbose(&format!("Unstowing package: {}", package_name));

        // Create a plan
        let mut plan = planner::Plan::new();

        // Find and plan removal of all symlinks pointing to this package
        self.plan_unstow_directory(&package_path, &self.target_dir, &mut plan)?;

        logger::verbose(&format!("Plan has {} actions", plan.len()));

        // Execute the plan
        plan.execute(self.dry_run)?;

        Ok(())
    }

    /// Recursively plan unstowing a directory
    fn plan_unstow_directory(
        &self,
        package_dir: &Path,
        target_dir: &Path,
        plan: &mut planner::Plan,
    ) -> Result<()> {
        // If target doesn't exist, nothing to unstow
        if !target_dir.exists() {
            return Ok(());
        }

        // Read the package directory to know what to look for
        let entries = fs::read_dir(package_dir)
            .map_err(|e| StowError::io_error(package_dir.to_path_buf(), e))?;

        for entry in entries {
            let entry = entry.map_err(|e| StowError::io_error(package_dir.to_path_buf(), e))?;
            let package_item = entry.path();
            let name = entry.file_name();
            let target_item = target_dir.join(&name);

            // Skip if target doesn't exist
            if !target_item.exists() {
                continue;
            }

            let metadata = entry
                .metadata()
                .map_err(|e| StowError::io_error(package_item.clone(), e))?;

            if metadata.is_dir() {
                self.plan_unstow_dir_item(&package_item, &target_item, plan)?;
            } else {
                self.plan_unstow_file(&package_item, &target_item, plan)?;
            }
        }

        // After removing items, check if target_dir is empty and should be pruned
        // We'll do this after all removals
        if *target_dir != self.target_dir {
            // Don't try to remove the root target directory
            plan.add(planner::Action::RemoveEmptyDir {
                path: target_dir.to_path_buf(),
            });
        }

        Ok(())
    }

    /// Plan unstowing a file
    fn plan_unstow_file(
        &self,
        package_file: &Path,
        target_file: &Path,
        plan: &mut planner::Plan,
    ) -> Result<()> {
        // Check if target is a symlink
        if !fs_ops::is_symlink(target_file) {
            // Not a symlink, might be a conflict or already removed
            logger::verbose(&format!(
                "Target is not a symlink, skipping: {}",
                target_file.display()
            ));
            return Ok(());
        }

        // Read the symlink target
        let link_target = fs_ops::read_symlink(target_file)?;
        let link_target_abs = if link_target.is_relative() {
            target_file
                .parent()
                .unwrap_or(target_file)
                .join(&link_target)
        } else {
            link_target.clone()
        };

        // Normalize both paths for comparison
        let link_target_norm = path_utils::normalize_path(&link_target_abs)?;
        let package_file_norm = path_utils::normalize_path(package_file)?;

        // Check if the symlink points to our package
        if path_utils::paths_equal(&link_target_norm, &package_file_norm) {
            // This symlink is from our package, remove it
            plan.add(planner::Action::RemoveLink {
                path: target_file.to_path_buf(),
            });
        } else {
            logger::verbose(&format!(
                "Symlink points elsewhere, skipping: {}",
                target_file.display()
            ));
        }

        Ok(())
    }

    /// Plan unstowing a directory
    fn plan_unstow_dir_item(
        &self,
        package_dir: &Path,
        target_dir: &Path,
        plan: &mut planner::Plan,
    ) -> Result<()> {
        // Check if target is a symlink
        if fs_ops::is_symlink(target_dir) {
            // Check if it points to our package directory
            let link_target = fs_ops::read_symlink(target_dir)?;
            let link_target_abs = if link_target.is_relative() {
                target_dir.parent().unwrap_or(target_dir).join(&link_target)
            } else {
                link_target.clone()
            };

            let link_target_norm = path_utils::normalize_path(&link_target_abs)?;
            let package_dir_norm = path_utils::normalize_path(package_dir)?;

            if path_utils::paths_equal(&link_target_norm, &package_dir_norm) {
                // This directory symlink is from our package, remove it
                plan.add(planner::Action::RemoveLink {
                    path: target_dir.to_path_buf(),
                });
            } else {
                logger::verbose(&format!(
                    "Directory symlink points elsewhere, skipping: {}",
                    target_dir.display()
                ));
            }
        } else if target_dir.is_dir() {
            // It's a real directory, recurse into it
            self.plan_unstow_directory(package_dir, target_dir, plan)?;
        } else {
            logger::verbose(&format!(
                "Target is not a directory or symlink, skipping: {}",
                target_dir.display()
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn test_unstower_creation() {
        let unstower = Unstower::new("/stow", "/target", false, true);
        assert_eq!(unstower.stow_dir, PathBuf::from("/stow"));
        assert_eq!(unstower.target_dir, PathBuf::from("/target"));
    }

    #[test]
    fn test_unstow_nonexistent_package() {
        let temp_dir = TempDir::new().unwrap();
        let unstower = Unstower::new(temp_dir.path(), temp_dir.path(), false, true);

        let result = unstower.unstow_package("nonexistent");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            StowError::PackageNotFound { .. }
        ));
    }

    #[test]
    fn test_unstow_with_no_links() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a package but don't link anything
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("file.txt")).unwrap();

        // Unstow should succeed (nothing to do)
        let unstower = Unstower::new(&stow_dir, &target_dir, false, true);
        let result = unstower.unstow_package("mypackage");
        assert!(result.is_ok());
    }

    #[test]
    fn test_unstow_skips_non_symlinks() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a package
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("file.txt")).unwrap();

        // Create a regular file (not symlink) in target
        File::create(target_dir.join("file.txt")).unwrap();

        // Unstow should skip the regular file
        let unstower = Unstower::new(&stow_dir, &target_dir, false, true);
        let result = unstower.unstow_package("mypackage");
        assert!(result.is_ok());

        // File should still exist
        assert!(target_dir.join("file.txt").exists());
    }

    #[test]
    fn test_unstow_package_not_directory() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a file instead of a directory
        File::create(stow_dir.join("notadir")).unwrap();

        let unstower = Unstower::new(&stow_dir, &target_dir, false, true);
        let result = unstower.unstow_package("notadir");
        // Should fail when trying to read the directory
        assert!(result.is_err());
    }

    #[test]
    fn test_unstow_nested_structure() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a package with nested structure
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir_all(package_dir.join("subdir")).unwrap();
        File::create(package_dir.join("file1.txt")).unwrap();
        File::create(package_dir.join("subdir/file2.txt")).unwrap();

        // Unstow should handle nested structure (dry run)
        let unstower = Unstower::new(&stow_dir, &target_dir, false, true);
        let result = unstower.unstow_package("mypackage");
        assert!(result.is_ok());
    }

    #[test]
    fn test_unstow_empty_package() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create an empty package
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();

        // Unstow should succeed (nothing to do)
        let unstower = Unstower::new(&stow_dir, &target_dir, false, true);
        let result = unstower.unstow_package("mypackage");
        assert!(result.is_ok());
    }

    #[test]
    fn test_unstow_multiple_files() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a package with multiple files
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("file1.txt")).unwrap();
        File::create(package_dir.join("file2.txt")).unwrap();
        File::create(package_dir.join("file3.txt")).unwrap();

        // Unstow should handle multiple files (dry run)
        let unstower = Unstower::new(&stow_dir, &target_dir, false, true);
        let result = unstower.unstow_package("mypackage");
        assert!(result.is_ok());
    }
}
