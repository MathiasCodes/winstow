use crate::error::{Result, StowError};
use crate::{fs_ops, logger, path_utils, planner};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

/// Unstow operation manager
pub struct Unstower {
    stow_dir: PathBuf,
    target_dir: PathBuf,
    dry_run: bool,
    no_folding: bool,
    /// Translate `dot-` prefixed names to `.`-prefixed names in the target (--dotfiles)
    dotfiles: bool,
}

/// State of a target entry once the plan has been executed
#[derive(Debug, PartialEq)]
enum PlannedState {
    /// The entry stays as it is
    Kept,
    /// The entry is removed
    Removed,
    /// The directory is replaced by a directory symlink to this package directory
    Refolded(PathBuf),
}

impl Unstower {
    /// Create a new Unstower from a StowContext
    pub fn from_context(context: &crate::config::StowContext) -> Self {
        Self {
            stow_dir: context.stow_dir().to_owned(),
            target_dir: context.target_dir().to_owned(),
            dry_run: context.is_dry_run(),
            no_folding: context.is_no_folding(),
            dotfiles: context.is_dotfiles(),
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
            no_folding: false,
            dotfiles: false,
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
    /// Returns what happens to `target_dir` itself
    fn plan_unstow_directory(
        &self,
        package_dir: &Path,
        target_dir: &Path,
        plan: &mut planner::Plan,
    ) -> Result<PlannedState> {
        // If target doesn't exist, nothing to unstow
        if !target_dir.exists() {
            return Ok(PlannedState::Kept);
        }

        // Names of target entries that are planned for removal
        let mut removed_names = HashSet::new();
        // Names of target subdirectories that are refolded, with the directory they link to
        let mut refolded = HashMap::new();
        // Names of package entries
        let mut package_names = HashSet::new();

        // Read the package directory to know what to look for
        let entries = fs::read_dir(package_dir)
            .map_err(|e| StowError::io_error(package_dir.to_path_buf(), e))?;

        for entry in entries {
            let entry = entry.map_err(|e| StowError::io_error(package_dir.to_path_buf(), e))?;
            let package_item = entry.path();
            let name = entry.file_name();
            // With --dotfiles, package entries named `dot-foo` map to `.foo` in the target
            let target_name = if self.dotfiles {
                path_utils::dotfiles_name(&name)
            } else {
                name.clone()
            };
            let target_item = target_dir.join(&target_name);
            package_names.insert(name_key(&target_name));

            // Skip if target doesn't exist
            if !target_item.exists() {
                continue;
            }

            let metadata = entry
                .metadata()
                .map_err(|e| StowError::io_error(package_item.clone(), e))?;

            let state = if metadata.is_dir() {
                self.plan_unstow_dir_item(&package_item, &target_item, plan)?
            } else {
                self.plan_unstow_file(&package_item, &target_item, plan)?
            };

            match state {
                PlannedState::Kept => {}
                PlannedState::Removed => {
                    removed_names.insert(name_key(&target_name));
                }
                PlannedState::Refolded(link_target) => {
                    refolded.insert(name_key(&target_name), link_target);
                }
            }
        }

        // Remove dangling links into this package left behind by renamed or deleted entries
        let target_entries = fs::read_dir(target_dir)
            .map_err(|e| StowError::io_error(target_dir.to_path_buf(), e))?;
        for entry in target_entries {
            let entry = entry.map_err(|e| StowError::io_error(target_dir.to_path_buf(), e))?;
            let key = name_key(&entry.file_name());
            if package_names.contains(&key) {
                continue;
            }

            let target_item = entry.path();
            if self.is_dangling_link_into(&target_item, package_dir)? {
                plan.add(planner::Action::RemoveLink { path: target_item });
                removed_names.insert(key);
            }
        }

        // Don't try to remove the root target directory
        if *target_dir == self.target_dir {
            return Ok(PlannedState::Kept);
        }

        // Prune target_dir only if nothing but planned removals is left in it,
        // e.g. keep AppData\Roaming\yazi when it still contains yazi's state
        let mut remaining = fs::read_dir(target_dir)
            .map_err(|e| StowError::io_error(target_dir.to_path_buf(), e))?;
        let will_be_empty = remaining.all(|entry| match entry {
            Ok(entry) => removed_names.contains(&name_key(&entry.file_name())),
            Err(_) => false,
        });

        if will_be_empty {
            plan.add(planner::Action::RemoveEmptyDir {
                path: target_dir.to_path_buf(),
            });
            return Ok(PlannedState::Removed);
        }

        if !self.no_folding
            && let Some(fold_target) = self.refold_target(target_dir, &removed_names, &refolded)?
        {
            self.plan_refold(target_dir, &fold_target, &removed_names, plan)?;
            return Ok(PlannedState::Refolded(fold_target));
        }

        logger::verbose(&format!(
            "Keeping non-empty directory: {}",
            target_dir.display()
        ));

        Ok(PlannedState::Kept)
    }

    /// Determine whether `target_dir` can be folded again, as GNU Stow does
    /// This is the case if every entry left in it is a symlink into one and the same
    /// package directory, under the entry's own name
    /// Returns that package directory
    fn refold_target(
        &self,
        target_dir: &Path,
        removed_names: &HashSet<String>,
        refolded: &HashMap<String, PathBuf>,
    ) -> Result<Option<PathBuf>> {
        let mut common_parent: Option<PathBuf> = None;
        let mut remaining_names = HashSet::new();

        let entries = fs::read_dir(target_dir)
            .map_err(|e| StowError::io_error(target_dir.to_path_buf(), e))?;
        for entry in entries {
            let entry = entry.map_err(|e| StowError::io_error(target_dir.to_path_buf(), e))?;
            let name = entry.file_name();
            let key = name_key(&name);
            if removed_names.contains(&key) {
                continue;
            }
            remaining_names.insert(key.clone());

            let path = entry.path();
            let link_target = if let Some(link_target) = refolded.get(&key) {
                link_target.clone()
            } else if fs_ops::is_symlink(&path) && path.exists() {
                resolve_link(&path)?
            } else {
                // A real file or directory, or a dangling link
                return Ok(None);
            };

            let (Some(link_name), Some(parent)) = (link_target.file_name(), link_target.parent())
            else {
                return Ok(None);
            };
            if name_key(link_name) != key {
                return Ok(None);
            }

            match &common_parent {
                None => common_parent = Some(parent.to_path_buf()),
                Some(common) if path_utils::paths_equal(common, parent) => {}
                Some(_) => return Ok(None),
            }
        }

        let Some(parent) = common_parent else {
            return Ok(None);
        };
        if !parent.is_dir()
            || !path_utils::is_within(&parent, &self.stow_dir)
            || path_utils::paths_equal(&parent, &self.stow_dir)
        {
            return Ok(None);
        }

        // Folding would expose every source entry, including entries omitted by
        // --ignore or entries whose target name differs under --dotfiles.
        let mut source_names = HashSet::new();
        for entry in fs::read_dir(&parent).map_err(|e| StowError::io_error(parent.clone(), e))? {
            let entry = entry.map_err(|e| StowError::io_error(parent.clone(), e))?;
            source_names.insert(name_key(&entry.file_name()));
        }
        Ok((source_names == remaining_names).then_some(parent))
    }

    /// Plan replacing `target_dir` by a directory symlink to `fold_target`
    fn plan_refold(
        &self,
        target_dir: &Path,
        fold_target: &Path,
        removed_names: &HashSet<String>,
        plan: &mut planner::Plan,
    ) -> Result<()> {
        logger::verbose(&format!(
            "Refolding directory: {} -> {}",
            target_dir.display(),
            fold_target.display()
        ));

        // Remove what is left; refolded subdirectories are links once their actions have run
        let entries = fs::read_dir(target_dir)
            .map_err(|e| StowError::io_error(target_dir.to_path_buf(), e))?;
        for entry in entries {
            let entry = entry.map_err(|e| StowError::io_error(target_dir.to_path_buf(), e))?;
            let key = name_key(&entry.file_name());
            if !removed_names.contains(&key) {
                plan.add(planner::Action::RemoveLink { path: entry.path() });
            }
        }

        plan.add(planner::Action::RemoveEmptyDir {
            path: target_dir.to_path_buf(),
        });

        let target_parent = target_dir.parent().unwrap_or(target_dir);
        let target_parent_abs = path_utils::normalize_path(target_parent)?;
        let relative_path = path_utils::compute_relative_path(&target_parent_abs, fold_target)?;
        plan.add(planner::Action::CreateDirLink {
            link_path: target_dir.to_path_buf(),
            target_path: relative_path,
        });

        Ok(())
    }

    /// Plan unstowing a file
    /// Returns what happens to the target entry
    fn plan_unstow_file(
        &self,
        package_file: &Path,
        target_file: &Path,
        plan: &mut planner::Plan,
    ) -> Result<PlannedState> {
        // Check if target is a symlink
        if !fs_ops::is_symlink(target_file) {
            // Not a symlink, might be a conflict or already removed
            logger::verbose(&format!(
                "Target is not a symlink, skipping: {}",
                target_file.display()
            ));
            return Ok(PlannedState::Kept);
        }

        if !self.link_points_to(target_file, package_file)? {
            logger::verbose(&format!(
                "Symlink points elsewhere, skipping: {}",
                target_file.display()
            ));
            return Ok(PlannedState::Kept);
        }

        // This symlink is from our package, remove it
        plan.add(planner::Action::RemoveLink {
            path: target_file.to_path_buf(),
        });

        Ok(PlannedState::Removed)
    }

    /// Plan unstowing a directory
    /// Returns what happens to the target directory or link
    fn plan_unstow_dir_item(
        &self,
        package_dir: &Path,
        target_dir: &Path,
        plan: &mut planner::Plan,
    ) -> Result<PlannedState> {
        // Check if target is a symlink
        if fs_ops::is_symlink(target_dir) {
            if !self.link_points_to(target_dir, package_dir)? {
                logger::verbose(&format!(
                    "Directory symlink points elsewhere, skipping: {}",
                    target_dir.display()
                ));
                return Ok(PlannedState::Kept);
            }

            // This directory symlink is from our package, remove it
            plan.add(planner::Action::RemoveLink {
                path: target_dir.to_path_buf(),
            });
            return Ok(PlannedState::Removed);
        }

        if target_dir.is_dir() {
            // It's a real directory, recurse into it
            return self.plan_unstow_directory(package_dir, target_dir, plan);
        }

        logger::verbose(&format!(
            "Target is not a directory or symlink, skipping: {}",
            target_dir.display()
        ));

        Ok(PlannedState::Kept)
    }

    /// Check whether the symlink at `link_path` points to `package_path`
    fn link_points_to(&self, link_path: &Path, package_path: &Path) -> Result<bool> {
        // Normalize both paths for comparison
        let link_target_norm = resolve_link(link_path)?;
        let package_path_norm = path_utils::normalize_path(package_path)?;

        let points_to = path_utils::paths_equal(&link_target_norm, &package_path_norm);
        Ok(points_to)
    }

    /// Check whether `path` is a symlink whose target is missing and would be below `package_dir`
    fn is_dangling_link_into(&self, path: &Path, package_dir: &Path) -> Result<bool> {
        if !fs_ops::is_symlink(path) || path.exists() {
            return Ok(false);
        }

        let link_target_norm = resolve_link(path)?;
        Ok(path_utils::is_within(&link_target_norm, package_dir))
    }
}

/// Resolve the target of the symlink at `link_path` to an absolute path
fn resolve_link(link_path: &Path) -> Result<PathBuf> {
    let link_target = fs_ops::read_symlink(link_path)?;
    let link_target_abs = if link_target.is_relative() {
        link_path.parent().unwrap_or(link_path).join(&link_target)
    } else {
        link_target
    };

    path_utils::normalize_path(&link_target_abs)
}

/// Key for comparing file names case-insensitively, as Windows does
fn name_key(name: &OsStr) -> String {
    name.to_string_lossy().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stow::Stower;
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

    #[test]
    fn test_stow_and_unstow_with_dotfiles() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a package with dot- prefixed files and directories
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("dot-bashrc")).unwrap();
        File::create(package_dir.join("dot-vimrc")).unwrap();
        fs::create_dir(package_dir.join("dot-config")).unwrap();
        File::create(package_dir.join("dot-config").join("settings")).unwrap();

        let stower = Stower::new(&stow_dir, &target_dir, false, false).with_dotfiles(true);
        stower.stow_package("mypackage").unwrap();

        // Links should exist under the dot- names
        assert!(target_dir.join(".bashrc").exists());
        assert!(target_dir.join(".vimrc").exists());
        assert!(target_dir.join(".config").join("settings").exists());

        // Unstow for real with the same option
        let mut unstower = Unstower::new(&stow_dir, &target_dir, false, false);
        unstower.dotfiles = true;
        unstower.unstow_package("mypackage").unwrap();

        // Symlinks should be removed
        assert!(!target_dir.join(".bashrc").exists());
        assert!(!target_dir.join(".vimrc").exists());

        // Package files remain untouched
        assert!(package_dir.join("dot-bashrc").exists());
        assert!(package_dir.join("dot-vimrc").exists());
    }

    #[test]
    fn test_unstow_with_dotfiles_mismatch_fails_safely() {
        // If --dotfiles was not used when stowing but is used when unstowing,
        // package entry dot-bashrc would map to .bashrc which doesn't exist,
        // so nothing should be removed
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("dot-bashrc")).unwrap();

        let stower = Stower::new(&stow_dir, &target_dir, false, false).with_dotfiles(true);
        stower.stow_package("mypackage").unwrap();

        // Unstow with dotfiles disabled: dot-bashrc maps to literal name, which
        // doesn't exist in target, so the .bashrc link must survive
        let unstower = Unstower::new(&stow_dir, &target_dir, false, false);
        unstower.unstow_package("mypackage").unwrap();

        assert!(target_dir.join(".bashrc").exists());
    }
}
