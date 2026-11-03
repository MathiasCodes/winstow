use crate::error::{Result, StowError};
use crate::{fs_ops, ignore, logger, path_utils, planner};
use std::collections::HashSet;
use std::ffi::OsString;
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
    no_folding: bool,
    /// Translate `dot-` prefixed names to `.`-prefixed names in the target (--dotfiles)
    dotfiles: bool,
}

/// Decision for how to handle a directory
#[derive(Debug, PartialEq)]
enum FoldDecision {
    /// Create a single directory symlink (fold)
    Fold,
    /// Target is already a symlink to this directory (no-op)
    AlreadyLinked,
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
            no_folding: context.is_no_folding(),
            dotfiles: context.is_dotfiles(),
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
            no_folding: false,
            dotfiles: false,
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

    /// Enable dotfiles translation (--dotfiles)
    #[cfg(test)]
    pub fn with_dotfiles(mut self, dotfiles: bool) -> Self {
        self.dotfiles = dotfiles;
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

            // With --dotfiles, `dot-foo` entries are stowed as `.foo` in the target
            let target_name = if self.dotfiles {
                path_utils::dotfiles_name(&name)
            } else {
                name.clone()
            };
            let target_path = target_parent.join(&target_name);

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

        if !self.plan_replace_dangling_link(target_path, plan)? {
            return Ok(());
        }

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
                let link_target_norm = self.resolve_link(target_path)?;

                if path_utils::paths_equal(&link_target_norm, &source_norm) {
                    // Already linked correctly, skip
                    logger::verbose(&format!("Already linked: {}", target_path.display()));
                    return Ok(());
                }
            }

            // Conflict: target exists and is not the right symlink
            if !self.plan_resolve_conflict(source_path, target_path, plan)? {
                return Ok(());
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

        if !self.plan_replace_dangling_link(target_path, plan)? {
            return Ok(());
        }

        let decision = self.decide_fold(source_path, target_path)?;

        match decision {
            FoldDecision::Fold => {
                self.plan_new_dir(source_path, target_path, plan)?;
            }

            FoldDecision::AlreadyLinked => {
                logger::verbose(&format!("Already linked: {}", target_path.display()));
            }

            FoldDecision::Unfold(original_target) => {
                // Unfold the existing symlink
                plan.add(planner::Action::UnfoldDirLink {
                    link_path: target_path.to_path_buf(),
                    original_target: original_target.clone(),
                });

                // After unfolding, we need to link the original target's contents
                // and the new source's contents into the now-real directory
                let link_base = LinkBase {
                    dir: path_utils::normalize_path(target_path.parent().unwrap_or(target_path))?,
                    depth: 1,
                };
                self.plan_stow_unfolded(
                    &original_target,
                    source_path,
                    target_path,
                    &link_base,
                    plan,
                )?;
            }

            FoldDecision::Traverse => {
                // Target is already a real directory, traverse into it
                self.plan_stow_directory(source_path, target_path, plan)?;
            }

            FoldDecision::Conflict => {
                // Target exists as a file or as a link stow cannot unfold
                if self.plan_resolve_conflict(source_path, target_path, plan)? {
                    self.plan_new_dir(source_path, target_path, plan)?;
                }
            }
        }

        Ok(())
    }

    /// Plan a package directory at a target path that is free
    /// Folds it into a directory symlink, or with --no-folding creates a real
    /// directory and links the children individually
    fn plan_new_dir(
        &self,
        source_path: &Path,
        target_path: &Path,
        plan: &mut planner::Plan,
    ) -> Result<()> {
        if !self.can_fold(source_path)? {
            plan.add(planner::Action::CreateDir {
                path: target_path.to_path_buf(),
            });
            return self.plan_stow_directory(source_path, target_path, plan);
        }

        // Normalize source path once to avoid repeated syscalls
        let source_norm = path_utils::normalize_path(source_path)?;

        // Create a directory symlink
        let target_parent = target_path.parent().unwrap_or(target_path);
        let target_parent_abs = path_utils::normalize_path(target_parent)?;
        let relative_path = path_utils::compute_relative_path(&target_parent_abs, &source_norm)?;

        plan.add(planner::Action::CreateDirLink {
            link_path: target_path.to_path_buf(),
            target_path: relative_path,
        });

        Ok(())
    }

    /// A directory link exposes the entire source tree. Traverse instead when
    /// an entry below it must be translated or ignored.
    fn can_fold(&self, source_path: &Path) -> Result<bool> {
        if self.no_folding {
            return Ok(false);
        }

        let mut pending = vec![source_path.to_path_buf()];
        let mut visited = HashSet::new();
        while let Some(dir) = pending.pop() {
            let canonical = dir
                .canonicalize()
                .map_err(|e| StowError::io_error(dir.clone(), e))?;
            if !visited.insert(canonical) {
                return Ok(false);
            }

            let entries = fs::read_dir(&dir).map_err(|e| StowError::io_error(dir.clone(), e))?;
            for entry in entries {
                let entry = entry.map_err(|e| StowError::io_error(dir.clone(), e))?;
                let path = entry.path();
                let name = entry.file_name();
                if self.patterns.should_ignore(&path)
                    || self.dotfiles && path_utils::dotfiles_name(&name) != name
                {
                    return Ok(false);
                }

                let metadata = entry
                    .metadata()
                    .map_err(|e| StowError::io_error(path.clone(), e))?;
                if metadata.is_dir() {
                    pending.push(path);
                }
            }
        }

        Ok(true)
    }

    /// Plan how to resolve a conflict at `target_path` according to the conflict strategy
    /// Returns whether the conflict is resolved, so that the link can be created
    fn plan_resolve_conflict(
        &self,
        source_path: &Path,
        target_path: &Path,
        plan: &mut planner::Plan,
    ) -> Result<bool> {
        let is_link = fs_ops::is_symlink(target_path);

        match self.conflict_strategy {
            ConflictStrategy::Fail => {}
            ConflictStrategy::Adopt => {
                // Only a real file can take the place of a package file
                if !is_link && target_path.is_file() && source_path.is_file() {
                    plan.add(planner::Action::AdoptFile {
                        target_path: target_path.to_path_buf(),
                        package_path: source_path.to_path_buf(),
                    });
                    return Ok(true);
                }
            }
            ConflictStrategy::Override => {
                if !is_link {
                    plan.add(planner::Action::RemovePath {
                        path: target_path.to_path_buf(),
                    });
                    return Ok(true);
                }

                // Replace links of other packages, but never links the user created
                let link_target_norm = self.resolve_link(target_path)?;
                if path_utils::is_within(&link_target_norm, &self.stow_dir) {
                    plan.add(planner::Action::RemoveLink {
                        path: target_path.to_path_buf(),
                    });
                    return Ok(true);
                }
            }
        }

        plan.add_conflict(target_path);
        Ok(false)
    }

    /// Plan the removal of a dangling symlink at `target_path` that stow owns,
    /// e.g. one left behind after a file in a package has been renamed
    /// A dangling link pointing outside the stow directory is a conflict
    /// Returns whether `target_path` is free for a new link, or was never a dangling link
    fn plan_replace_dangling_link(
        &self,
        target_path: &Path,
        plan: &mut planner::Plan,
    ) -> Result<bool> {
        if !fs_ops::is_symlink(target_path) || target_path.exists() {
            return Ok(true);
        }

        let link_target_norm = self.resolve_link(target_path)?;
        if !path_utils::is_within(&link_target_norm, &self.stow_dir) {
            plan.add_conflict(target_path);
            return Ok(false);
        }

        logger::verbose(&format!(
            "Replacing dangling link: {} -> {}",
            target_path.display(),
            link_target_norm.display()
        ));
        plan.add(planner::Action::RemoveLink {
            path: target_path.to_path_buf(),
        });

        Ok(true)
    }

    /// Resolve the target of the symlink at `link_path` to an absolute path
    fn resolve_link(&self, link_path: &Path) -> Result<PathBuf> {
        let link_target = fs_ops::read_symlink(link_path)?;
        let link_target_abs = if link_target.is_relative() {
            link_path.parent().unwrap_or(link_path).join(&link_target)
        } else {
            link_target
        };

        path_utils::normalize_path(&link_target_abs)
    }

    /// Decide how to handle a directory
    fn decide_fold(&self, source_path: &Path, target_path: &Path) -> Result<FoldDecision> {
        if !target_path.exists() {
            // Target doesn't exist, we can fold
            return Ok(FoldDecision::Fold);
        }

        // Check if target is a symlink
        if fs_ops::is_symlink(target_path) {
            let link_target_norm = self.resolve_link(target_path)?;
            let source_norm = path_utils::normalize_path(source_path)?;

            if path_utils::paths_equal(&link_target_norm, &source_norm) {
                // Already linked to the same place
                return Ok(FoldDecision::AlreadyLinked);
            }

            // Only unfold links that stow owns and that point to a directory;
            // a file link of another package or a link the user created conflicts
            if !link_target_norm.is_dir()
                || !path_utils::is_within(&link_target_norm, &self.stow_dir)
            {
                return Ok(FoldDecision::Conflict);
            }

            // Linked to a different package, need to unfold
            return Ok(FoldDecision::Unfold(link_target_norm));
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
    /// Links contents of both the original target and the new source into the target directory.
    /// At planning time `target_dir` is still the old link, so looking at the filesystem
    /// below it would show the original package's files; the two listings are merged instead.
    fn plan_stow_unfolded(
        &self,
        original_target: &Path,
        new_source: &Path,
        target_dir: &Path,
        link_base: &LinkBase,
        plan: &mut planner::Plan,
    ) -> Result<()> {
        let original_entries = self.list_entries(original_target)?;
        let new_entries = self.list_entries(new_source)?;

        let mut replaced_names = HashSet::new();
        let mut merged_dirs = Vec::new();
        let mut new_links = Vec::new();

        for new_entry in &new_entries {
            let Some(original_entry) = original_entries.iter().find(|o| o.key == new_entry.key)
            else {
                new_links.push(new_entry);
                continue;
            };

            if self.patterns.should_defer(&new_entry.path) {
                logger::verbose(&format!(
                    "Deferring: {} (already stowed)",
                    new_entry.path.display()
                ));
            } else if original_entry.is_dir && new_entry.is_dir {
                // Both packages provide this directory, unfold it as well
                replaced_names.insert(new_entry.key.as_str());
                merged_dirs.push((original_entry, new_entry));
            } else if self.conflict_strategy == ConflictStrategy::Override {
                // Replace the other package's link, its files stay untouched
                logger::verbose(&format!(
                    "Overriding: {} (stowed by another package)",
                    target_dir.join(&new_entry.target_name).display()
                ));
                replaced_names.insert(new_entry.key.as_str());
                new_links.push(new_entry);
            } else {
                // Keep planning to report every conflict, the plan will not be executed
                plan.add_conflict(target_dir.join(&new_entry.target_name));
            }
        }

        for original_entry in &original_entries {
            if !replaced_names.contains(original_entry.key.as_str()) {
                self.plan_unfolded_link(original_entry, target_dir, link_base, plan)?;
            }
        }

        for (original_entry, new_entry) in merged_dirs {
            let nested_base = LinkBase {
                dir: link_base.dir.clone(),
                depth: link_base.depth + 1,
            };
            self.plan_stow_unfolded(
                &original_entry.path,
                &new_entry.path,
                &target_dir.join(&new_entry.target_name),
                &nested_base,
                plan,
            )?;
        }

        for new_entry in new_links {
            self.plan_unfolded_link(new_entry, target_dir, link_base, plan)?;
        }

        Ok(())
    }

    /// Plan a link inside a directory that is being unfolded
    fn plan_unfolded_link(
        &self,
        entry: &PackageEntry,
        target_dir: &Path,
        link_base: &LinkBase,
        plan: &mut planner::Plan,
    ) -> Result<()> {
        let source_norm = path_utils::normalize_path(&entry.path)?;
        let base_to_source = path_utils::compute_relative_path(&link_base.dir, &source_norm)?;
        let mut relative_path: PathBuf = std::iter::repeat_n("..", link_base.depth).collect();
        relative_path.push(base_to_source);

        let link_path = target_dir.join(&entry.target_name);

        if entry.is_dir && !self.can_fold(&entry.path)? {
            plan.add(planner::Action::CreateDir {
                path: link_path.clone(),
            });
            let nested_base = LinkBase {
                dir: link_base.dir.clone(),
                depth: link_base.depth + 1,
            };
            for child in self.list_entries(&entry.path)? {
                self.plan_unfolded_link(&child, &link_path, &nested_base, plan)?;
            }
            return Ok(());
        }

        let action = if entry.is_dir {
            planner::Action::CreateDirLink {
                link_path,
                target_path: relative_path,
            }
        } else {
            planner::Action::CreateFileLink {
                link_path,
                target_path: relative_path,
            }
        };
        plan.add(action);

        Ok(())
    }

    /// List the entries of a package directory that are not ignored
    fn list_entries(&self, dir: &Path) -> Result<Vec<PackageEntry>> {
        let entries = fs::read_dir(dir).map_err(|e| StowError::io_error(dir.to_path_buf(), e))?;

        let mut result = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| StowError::io_error(dir.to_owned(), e))?;
            let path = entry.path();

            if self.patterns.should_ignore(&path) {
                logger::verbose(&format!("Ignoring: {}", path.display()));
                continue;
            }

            let metadata = entry
                .metadata()
                .map_err(|e| StowError::io_error(path.clone(), e))?;
            let name = entry.file_name();
            let target_name = if self.dotfiles {
                path_utils::dotfiles_name(&name)
            } else {
                name
            };

            result.push(PackageEntry {
                key: target_name.to_string_lossy().to_lowercase(),
                target_name,
                path,
                is_dir: metadata.is_dir(),
            });
        }

        Ok(result)
    }
}

/// An entry of a package directory
struct PackageEntry {
    /// Name for case-insensitive comparison, as Windows does
    key: String,
    target_name: OsString,
    path: PathBuf,
    is_dir: bool,
}

/// Real directory from which links inside an unfolded directory are computed
/// Paths below an unfolded link cannot be canonicalised, since that would
/// resolve the old link; `depth` counts the levels between `dir` and the link's parent
struct LinkBase {
    dir: PathBuf,
    depth: usize,
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

    #[test]
    fn test_stow_with_dotfiles() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a package with dot- prefixed files and directories
        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("dot-bashrc")).unwrap();
        File::create(package_dir.join("regular.txt")).unwrap();
        fs::create_dir(package_dir.join("dot-git")).unwrap();
        File::create(package_dir.join("dot-git").join("config")).unwrap();

        let stower = Stower::new(&stow_dir, &target_dir, false, false).with_dotfiles(true);

        let result = stower.stow_package("mypackage");
        assert!(result.is_ok(), "{:?}", result);

        // dot-bashrc should be stowed as .bashrc
        assert!(target_dir.join(".bashrc").exists());
        assert!(fs_ops::is_symlink(&target_dir.join(".bashrc")));

        // Regular files are unchanged
        assert!(target_dir.join("regular.txt").exists());

        // dot-git directory should be stowed as .git
        assert!(target_dir.join(".git").join("config").exists());

        // The dot- names should not exist in the target
        assert!(!target_dir.join("dot-bashrc").exists());
        assert!(!target_dir.join("dot-git").exists());
    }

    #[test]
    fn test_stow_without_dotfiles_leaves_names_unchanged() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        let package_dir = stow_dir.join("mypackage");
        fs::create_dir(&package_dir).unwrap();
        File::create(package_dir.join("dot-bashrc")).unwrap();

        let stower = Stower::new(&stow_dir, &target_dir, false, false);
        let result = stower.stow_package("mypackage");
        assert!(result.is_ok(), "{:?}", result);

        // Without --dotfiles, the literal name is used
        assert!(target_dir.join("dot-bashrc").exists());
        assert!(!target_dir.join(".bashrc").exists());
    }
}
