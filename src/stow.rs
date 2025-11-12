use crate::error::{Result, StowError};
use crate::{adopt, fs_ops, ignore, logger, path_utils, planner};
use std::fs;
use std::path::{Path, PathBuf};

/// Conflict resolution strategy
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConflictStrategy {
    /// Fail on conflicts (default)
    Fail,
    /// Adopt conflicting files into the package
    Adopt,
    /// Override (remove) conflicting files
    Override,
}

/// Stow operation manager
pub struct Stower {
    stow_dir: PathBuf,
    target_dir: PathBuf,
    dry_run: bool,
    conflict_strategy: ConflictStrategy,
    patterns: ignore::PatternSet,
}

/// Decision for how to handle a directory
#[derive(Debug, PartialEq)]
enum FoldDecision {
    /// Create a single directory symlink (fold)
    Fold,
    /// Remove existing symlink and traverse into directory (unfold)
    Unfold(PathBuf), // Contains the original target
    /// Traverse into directory and link children (already a real directory)
    Traverse,
    /// Path conflicts with existing file
    Conflict,
}

impl Stower {
    /// Create a new Stower from a StowContext
    pub fn from_context(
        context: &crate::config::StowContext,
        patterns: ignore::PatternSet,
    ) -> Self {
        Self {
            stow_dir: context.stow_dir().to_owned(),
            target_dir: context.target_dir().to_owned(),
            dry_run: context.is_dry_run(),
            conflict_strategy: context.conflict_strategy(),
            patterns,
        }
    }

    /// Create a new Stower (legacy constructor for backward compatibility)
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
            conflict_strategy: ConflictStrategy::Fail,
            patterns: ignore::PatternSet::empty(),
        }
    }

    /// Set the conflict resolution strategy
    #[cfg(test)]
    pub fn with_conflict_strategy(mut self, strategy: ConflictStrategy) -> Self {
        self.conflict_strategy = strategy;
        self
    }

    /// Set the ignore and defer patterns
    #[cfg(test)]
    pub fn with_patterns(mut self, patterns: ignore::PatternSet) -> Self {
        self.patterns = patterns;
        self
    }

    /// Stow a package
    #[must_use = "stow operations can fail and should be checked"]
    pub fn stow_package(&self, package_name: &str) -> Result<()> {
        let package_path = self.stow_dir.join(package_name);

        // Verify package exists
        if !package_path.exists() {
            return Err(StowError::package_not_found(package_name, &self.stow_dir));
        }

        if !package_path.is_dir() {
            return Err(StowError::invalid_path(format!(
                "Package '{}' is not a directory",
                package_name
            )));
        }

        logger::verbose(&format!("Processing package: {}", package_name));

        // Create a plan
        let mut plan = planner::Plan::new();

        // Traverse the package and build the plan
        self.plan_stow_directory(&package_path, &self.target_dir, &mut plan)?;

        logger::verbose(&format!("Plan has {} actions", plan.len()));

        // Execute the plan
        plan.execute(self.dry_run)?;

        Ok(())
    }

    /// Recursively plan stowing a directory
    fn plan_stow_directory(
        &self,
        source_dir: &Path,
        target_parent: &Path,
        plan: &mut planner::Plan,
    ) -> Result<()> {
        // Read the source directory contents
        let entries = fs::read_dir(source_dir)
            .map_err(|e| StowError::io_error(source_dir.to_path_buf(), e))?;

        for entry in entries {
            let entry = entry.map_err(|e| StowError::io_error(source_dir.to_owned(), e))?;
            let source_path = entry.path();
            let name = entry.file_name();

            // Check if this path should be ignored
            if self.patterns.should_ignore(&source_path) {
                logger::verbose(&format!("Ignoring: {}", source_path.display()));
                continue;
            }

            let target_path = target_parent.join(&name);

            let metadata = entry
                .metadata()
                .map_err(|e| StowError::io_error(source_path.clone(), e))?;

            if metadata.is_dir() {
                // Handle directory
                self.plan_stow_dir_item(&source_path, &target_path, plan)?;
            } else {
                // Handle file
                self.plan_stow_file(&source_path, &target_path, plan)?;
            }
        }

        Ok(())
    }

    /// Plan stowing a file
    fn plan_stow_file(
        &self,
        source_path: &Path,
        target_path: &Path,
        plan: &mut planner::Plan,
    ) -> Result<()> {
        // Normalize source path once at the start to avoid repeated syscalls
        let source_norm = path_utils::normalize_path(source_path)?;

        // Check if target exists
        if target_path.exists() {
            // Check if this path should be deferred (only when target exists)
            // This matches GNU Stow's behavior: defer only if already stowed by another package
            if self.patterns.should_defer(source_path) {
                logger::verbose(&format!(
                    "Deferring: {} (already exists)",
                    source_path.display()
                ));
                return Ok(());
            }

            // Check if it's already a symlink to the same source
            if fs_ops::is_symlink(target_path) {
                let link_target = fs_ops::read_symlink(target_path)?;
                let link_target_abs = if link_target.is_relative() {
                    target_path
                        .parent()
                        .unwrap_or(target_path)
                        .join(&link_target)
                } else {
                    link_target.clone()
                };

                let link_target_norm = path_utils::normalize_path(&link_target_abs)?;

                if path_utils::paths_equal(&link_target_norm, &source_norm) {
                    // Already linked correctly, skip
                    logger::verbose(&format!("Already linked: {}", target_path.display()));
                    return Ok(());
                }
            }

            // Conflict: target exists and is not the right symlink
            // Handle based on conflict strategy
            match self.conflict_strategy {
                ConflictStrategy::Fail => {
                    return Err(StowError::conflict(target_path));
                }
                ConflictStrategy::Adopt => {
                    // Adopt the file first
                    adopt::adopt_file(target_path, source_path, self.dry_run)?;
                    // Now the target is gone, we can link it
                }
                ConflictStrategy::Override => {
                    // Remove the conflicting file
                    adopt::override_file(target_path, self.dry_run)?;
                    // Now the target is gone, we can link it
                }
            }
        }

        // Compute relative path from target to source
        let target_parent = target_path.parent().unwrap_or(target_path);
        let target_parent_abs = path_utils::normalize_path(target_parent)?;
        let relative_path = path_utils::compute_relative_path(&target_parent_abs, &source_norm)?;

        // Add action to create the symlink
        plan.add(planner::Action::CreateFileLink {
            link_path: target_path.to_owned(),
            target_path: relative_path,
        });

        Ok(())
    }

    /// Plan stowing a directory
    fn plan_stow_dir_item(
        &self,
        source_path: &Path,
        target_path: &Path,
        plan: &mut planner::Plan,
    ) -> Result<()> {
        // Check if this path should be deferred (only when target exists)
        // This matches GNU Stow's behavior: defer only if already stowed by another package
        if target_path.exists() && self.patterns.should_defer(source_path) {
            logger::verbose(&format!(
                "Deferring: {} (already exists)",
                source_path.display()
            ));
            return Ok(());
        }

        let decision = self.decide_fold(source_path, target_path)?;

        match decision {
            FoldDecision::Fold => {
                // Normalize source path once to avoid repeated syscalls
                let source_norm = path_utils::normalize_path(source_path)?;

                // Create a directory symlink
                let target_parent = target_path.parent().unwrap_or(target_path);
                let target_parent_abs = path_utils::normalize_path(target_parent)?;
                let relative_path =
                    path_utils::compute_relative_path(&target_parent_abs, &source_norm)?;

                plan.add(planner::Action::CreateDirLink {
                    link_path: target_path.to_owned(),
                    target_path: relative_path,
                });
            }

            FoldDecision::Unfold(original_target) => {
                // Unfold the existing symlink
                plan.add(planner::Action::UnfoldDirLink {
                    link_path: target_path.to_path_buf(),
                    original_target: original_target.clone(),
                });

                // After unfolding, we need to link the original target's contents
                // and the new source's contents into the now-real directory
                self.plan_stow_unfolded(&original_target, source_path, target_path, plan)?;
            }

            FoldDecision::Traverse => {
                // Target is already a real directory, traverse into it
                self.plan_stow_directory(source_path, target_path, plan)?;
            }

            FoldDecision::Conflict => {
                // Normalize source path once to avoid repeated syscalls
                let source_norm = path_utils::normalize_path(source_path)?;

                // Target exists as a file
                match self.conflict_strategy {
                    ConflictStrategy::Fail => {
                        return Err(StowError::conflict(target_path));
                    }
                    ConflictStrategy::Adopt => {
                        // Adopt the conflicting file/directory
                        if target_path.is_dir() {
                            adopt::adopt_directory(target_path, source_path, self.dry_run)?;
                        } else {
                            adopt::adopt_file(target_path, source_path, self.dry_run)?;
                        }
                        // Now create the link
                        let target_parent = target_path.parent().unwrap_or(target_path);
                        let target_parent_abs = path_utils::normalize_path(target_parent)?;
                        let relative_path =
                            path_utils::compute_relative_path(&target_parent_abs, &source_norm)?;

                        plan.add(planner::Action::CreateDirLink {
                            link_path: target_path.to_path_buf(),
                            target_path: relative_path,
                        });
                    }
                    ConflictStrategy::Override => {
                        // Remove the conflicting file/directory
                        adopt::override_file(target_path, self.dry_run)?;
                        // Now create the link
                        let target_parent = target_path.parent().unwrap_or(target_path);
                        let target_parent_abs = path_utils::normalize_path(target_parent)?;
                        let relative_path =
                            path_utils::compute_relative_path(&target_parent_abs, &source_norm)?;

                        plan.add(planner::Action::CreateDirLink {
                            link_path: target_path.to_path_buf(),
                            target_path: relative_path,
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Decide how to handle a directory
    fn decide_fold(&self, source_path: &Path, target_path: &Path) -> Result<FoldDecision> {
        if !target_path.exists() {
            // Target doesn't exist, we can fold
            return Ok(FoldDecision::Fold);
        }

        // Check if target is a symlink
        if fs_ops::is_symlink(target_path) {
            let link_target = fs_ops::read_symlink(target_path)?;
            let link_target_abs = if link_target.is_relative() {
                target_path
                    .parent()
                    .unwrap_or(target_path)
                    .join(&link_target)
            } else {
                link_target.clone()
            };

            let link_target_norm = path_utils::normalize_path(&link_target_abs)?;
            let source_norm = path_utils::normalize_path(source_path)?;

            if path_utils::paths_equal(&link_target_norm, &source_norm) {
                // Already linked to the same place, fold (no-op)
                return Ok(FoldDecision::Fold);
            } else {
                // Linked to a different place, need to unfold
                return Ok(FoldDecision::Unfold(link_target_abs));
            }
        }

        // Check if target is a directory
        if fs_ops::is_directory(target_path)? {
            // Real directory, traverse into it
            return Ok(FoldDecision::Traverse);
        }

        // Target is a regular file, conflict
        Ok(FoldDecision::Conflict)
    }

    /// Plan stowing after unfolding
    /// Links contents of both the original target and the new source into the target directory
    fn plan_stow_unfolded(
        &self,
        original_target: &Path,
        new_source: &Path,
        target_dir: &Path,
        plan: &mut planner::Plan,
    ) -> Result<()> {
        // First, link all contents from the original target
        if original_target.exists() && original_target.is_dir() {
            self.plan_stow_directory(original_target, target_dir, plan)?;
        }

        // Then, link all contents from the new source
        self.plan_stow_directory(new_source, target_dir, plan)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn test_stower_creation() {
        let stower = Stower::new("/stow", "/target", false, true);
        assert_eq!(stower.stow_dir, PathBuf::from("/stow"));
        assert_eq!(stower.target_dir, PathBuf::from("/target"));
    }

    #[test]
    fn test_stow_nonexistent_package() {
        let temp_dir = TempDir::new().unwrap();
        let stower = Stower::new(temp_dir.path(), temp_dir.path(), false, true);

        let result = stower.stow_package("nonexistent");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            StowError::PackageNotFound { .. }
        ));
    }

    #[test]
    fn test_stow_simple_file() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a package with a file
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("file.txt")).unwrap();

        // Stow it (dry run)
        let stower = Stower::new(&stow_dir, &target_dir, false, true);
        let result = stower.stow_package("mypackage");
        assert!(result.is_ok());
    }

    #[test]
    fn test_conflict_detection() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a package with a file
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("file.txt")).unwrap();

        // Create a conflicting file in target
        File::create(target_dir.join("file.txt")).unwrap();

        // Stow should detect conflict
        let stower = Stower::new(&stow_dir, &target_dir, false, true);
        let result = stower.stow_package("mypackage");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StowError::Conflict { .. }));
    }

    #[test]
    fn test_with_conflict_strategy() {
        let stower = Stower::new("/stow", "/target", false, true)
            .with_conflict_strategy(ConflictStrategy::Adopt);
        assert_eq!(stower.conflict_strategy, ConflictStrategy::Adopt);

        let stower = Stower::new("/stow", "/target", false, true)
            .with_conflict_strategy(ConflictStrategy::Override);
        assert_eq!(stower.conflict_strategy, ConflictStrategy::Override);
    }

    #[test]
    fn test_with_patterns() {
        let patterns = ignore::PatternSet::empty();
        let stower = Stower::new("/stow", "/target", false, true).with_patterns(patterns);
        // Just verify it doesn't panic
        assert_eq!(stower.stow_dir, PathBuf::from("/stow"));
    }

    #[test]
    fn test_stow_package_not_directory() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a file instead of a directory
        File::create(stow_dir.join("notadir")).unwrap();

        let stower = Stower::new(&stow_dir, &target_dir, false, true);
        let result = stower.stow_package("notadir");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StowError::InvalidPath(_)));
    }

    #[test]
    fn test_stow_with_ignore_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a package with files
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("important.txt")).unwrap();
        File::create(package_dir.join("backup.bak")).unwrap();

        // Create pattern set to ignore .bak files
        let ignore_patterns = vec!["*.bak".to_string()];
        let patterns = ignore::PatternSet::new(&ignore_patterns, &[]).unwrap();

        let stower = Stower::new(&stow_dir, &target_dir, false, true).with_patterns(patterns);

        let result = stower.stow_package("mypackage");
        assert!(result.is_ok());
    }

    #[test]
    fn test_stow_with_defer_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a package with files
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("config.txt")).unwrap();

        // Create existing file in target
        File::create(target_dir.join("config.txt")).unwrap();

        // Create pattern set to defer config.txt
        let defer_patterns = vec!["config.txt".to_string()];
        let patterns = ignore::PatternSet::new(&[], &defer_patterns).unwrap();

        let stower = Stower::new(&stow_dir, &target_dir, false, true).with_patterns(patterns);

        // Should succeed because config.txt is deferred
        let result = stower.stow_package("mypackage");
        assert!(result.is_ok());
    }

    #[test]
    fn test_conflict_strategy_adopt() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a package with a file
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("file.txt")).unwrap();

        // Create a conflicting file in target
        File::create(target_dir.join("file.txt")).unwrap();

        // Stow with adopt strategy (dry run)
        let stower = Stower::new(&stow_dir, &target_dir, false, true)
            .with_conflict_strategy(ConflictStrategy::Adopt);
        let result = stower.stow_package("mypackage");
        assert!(result.is_ok());
    }

    #[test]
    fn test_conflict_strategy_override() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a package with a file
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("file.txt")).unwrap();

        // Create a conflicting file in target
        File::create(target_dir.join("file.txt")).unwrap();

        // Stow with override strategy (dry run)
        let stower = Stower::new(&stow_dir, &target_dir, false, true)
            .with_conflict_strategy(ConflictStrategy::Override);
        let result = stower.stow_package("mypackage");
        assert!(result.is_ok());
    }
}
