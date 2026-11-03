use crate::error::{Result, StowError};
use crate::{adopt, fs_ops, logger};
use std::path::PathBuf;

/// Represents an action to be performed during stow/unstow operations
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Create a file symlink
    CreateFileLink {
        link_path: PathBuf,
        target_path: PathBuf,
    },
    /// Create a directory symlink (folding)
    CreateDirLink {
        link_path: PathBuf,
        target_path: PathBuf,
    },
    /// Unfold an existing directory symlink
    /// This removes the dir symlink and recreates it as a real directory
    UnfoldDirLink {
        link_path: PathBuf,
        original_target: PathBuf,
    },
    /// Remove a symlink (file or directory)
    RemoveLink { path: PathBuf },
    /// Remove an empty directory
    RemoveEmptyDir { path: PathBuf },
    /// Move a conflicting file from the target into the package (--adopt)
    AdoptFile {
        target_path: PathBuf,
        package_path: PathBuf,
    },
    /// Remove a conflicting file or directory with all its contents (--override)
    RemovePath { path: PathBuf },
}

/// A plan containing a sequence of actions to execute
/// Conflicts found while planning are collected, so that all of them can be
/// reported at once and nothing is changed if there is any
#[derive(Debug, Default)]
pub struct Plan {
    actions: Vec<Action>,
    conflicts: Vec<PathBuf>,
}

impl Plan {
    /// Create a new empty plan
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an action to the plan
    pub fn add(&mut self, action: Action) {
        self.actions.push(action);
    }

    /// Record a path that conflicts with the package
    pub fn add_conflict(&mut self, path: impl Into<PathBuf>) {
        self.conflicts.push(path.into());
    }

    /// Get the number of actions in the plan
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Execute all actions in the plan
    /// Fails without changing anything if conflicts have been recorded
    #[must_use = "plan execution can fail and should be checked"]
    pub fn execute(&self, dry_run: bool) -> Result<()> {
        if !self.conflicts.is_empty() {
            return Err(StowError::conflicts(self.conflicts.clone()));
        }

        for action in &self.actions {
            execute_action(action, dry_run)?;
        }
        Ok(())
    }
}

/// Execute a single action
fn execute_action(action: &Action, dry_run: bool) -> Result<()> {
    match action {
        Action::CreateFileLink {
            link_path,
            target_path,
        } => {
            logger::action(&format!(
                "Create file link: {} -> {}",
                link_path.display(),
                target_path.display()
            ));

            if !dry_run {
                fs_ops::create_symlink(link_path, target_path, false)?;
            }
        }

        Action::CreateDirLink {
            link_path,
            target_path,
        } => {
            logger::action(&format!(
                "Create directory link: {} -> {}",
                link_path.display(),
                target_path.display()
            ));

            if !dry_run {
                fs_ops::create_symlink(link_path, target_path, true)?;
            }
        }

        Action::UnfoldDirLink {
            link_path,
            original_target,
        } => {
            logger::action(&format!(
                "Unfold directory link: {} (was -> {})",
                link_path.display(),
                original_target.display()
            ));

            if !dry_run {
                // Remove the symlink
                fs_ops::remove_symlink(link_path)?;

                // Create a real directory
                std::fs::create_dir(link_path)
                    .map_err(|e| StowError::io_error(link_path.clone(), e))?;

                // Now we need to populate it with links to the original target's contents
                // This is handled by the stow logic after unfolding
            }
        }

        Action::RemoveLink { path } => {
            logger::action(&format!("Remove link: {}", path.display()));

            if !dry_run {
                fs_ops::remove_symlink(path)?;
            }
        }

        Action::RemoveEmptyDir { path } => {
            logger::action(&format!("Remove empty directory: {}", path.display()));

            if !dry_run {
                fs_ops::remove_empty_directory(path)?;
            }
        }

        Action::AdoptFile {
            target_path,
            package_path,
        } => {
            logger::action(&format!(
                "Adopt file: {} -> {}",
                target_path.display(),
                package_path.display()
            ));

            if !dry_run {
                adopt::adopt_file(target_path, package_path)?;
            }
        }

        Action::RemovePath { path } => {
            logger::action(&format!("Override (remove): {}", path.display()));

            if !dry_run {
                adopt::remove_path(path)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_creation() {
        let mut plan = Plan::new();
        assert_eq!(plan.len(), 0);

        plan.add(Action::CreateFileLink {
            link_path: PathBuf::from("link"),
            target_path: PathBuf::from("target"),
        });

        assert_eq!(plan.len(), 1);
    }

    #[test]
    fn test_execute_dry_run() {
        let mut plan = Plan::new();
        plan.add(Action::CreateFileLink {
            link_path: PathBuf::from("/nonexistent/link"),
            target_path: PathBuf::from("/nonexistent/target"),
        });

        // Dry run should not fail even with invalid paths
        let result = plan.execute(true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_create_dir_link_dry_run() {
        let mut plan = Plan::new();
        plan.add(Action::CreateDirLink {
            link_path: PathBuf::from("/nonexistent/link"),
            target_path: PathBuf::from("/nonexistent/target"),
        });

        // Dry run should not fail
        let result = plan.execute(true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_unfold_dir_link_dry_run() {
        let mut plan = Plan::new();
        plan.add(Action::UnfoldDirLink {
            link_path: PathBuf::from("/nonexistent/link"),
            original_target: PathBuf::from("/nonexistent/target"),
        });

        // Dry run should not fail
        let result = plan.execute(true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_remove_link_dry_run() {
        let mut plan = Plan::new();
        plan.add(Action::RemoveLink {
            path: PathBuf::from("/nonexistent/link"),
        });

        // Dry run should not fail
        let result = plan.execute(true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_remove_empty_dir_dry_run() {
        let mut plan = Plan::new();
        plan.add(Action::RemoveEmptyDir {
            path: PathBuf::from("/nonexistent/dir"),
        });

        // Dry run should not fail
        let result = plan.execute(true);
        assert!(result.is_ok());
    }

    #[test]
    fn execute_with_conflicts_fails_without_running_actions() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let file = temp_dir.path().join("file.txt");
        std::fs::write(&file, "keep").unwrap();
        let mut plan = Plan::new();
        plan.add(Action::RemovePath { path: file.clone() });
        plan.add_conflict(temp_dir.path().join("first"));
        plan.add_conflict(temp_dir.path().join("second"));

        let result = plan.execute(false);

        assert!(matches!(
            result,
            Err(StowError::Conflict { ref paths }) if paths.len() == 2
        ));
        assert!(file.exists());
    }

    #[test]
    fn execute_adopt_and_remove_dry_run_changes_nothing() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let target_file = temp_dir.path().join("target.txt");
        let package_file = temp_dir.path().join("package.txt");
        let removed_dir = temp_dir.path().join("dir");
        std::fs::write(&target_file, "target").unwrap();
        std::fs::create_dir(&removed_dir).unwrap();
        let mut plan = Plan::new();
        plan.add(Action::AdoptFile {
            target_path: target_file.clone(),
            package_path: package_file.clone(),
        });
        plan.add(Action::RemovePath {
            path: removed_dir.clone(),
        });

        plan.execute(true).unwrap();

        assert!(target_file.exists());
        assert!(!package_file.exists());
        assert!(removed_dir.exists());
    }

    #[test]
    fn test_plan_multiple_action_types() {
        let mut plan = Plan::new();

        plan.add(Action::CreateFileLink {
            link_path: PathBuf::from("link1"),
            target_path: PathBuf::from("target1"),
        });

        plan.add(Action::CreateDirLink {
            link_path: PathBuf::from("link2"),
            target_path: PathBuf::from("target2"),
        });

        plan.add(Action::RemoveLink {
            path: PathBuf::from("link3"),
        });

        assert_eq!(plan.len(), 3);

        // Dry run should succeed
        let result = plan.execute(true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_plan_len() {
        let mut plan = Plan::new();
        assert_eq!(plan.len(), 0);

        plan.add(Action::CreateFileLink {
            link_path: PathBuf::from("link"),
            target_path: PathBuf::from("target"),
        });

        assert_eq!(plan.len(), 1);

        plan.add(Action::RemoveLink {
            path: PathBuf::from("link2"),
        });

        assert_eq!(plan.len(), 2);
    }
}
