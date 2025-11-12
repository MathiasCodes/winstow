use crate::error::{Result, StowError};
use crate::{fs_ops, logger};
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
}

/// A plan containing a sequence of actions to execute
#[derive(Debug, Default)]
pub struct Plan {
    actions: Vec<Action>,
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

    /// Get the number of actions in the plan
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Execute all actions in the plan
    #[must_use = "plan execution can fail and should be checked"]
    pub fn execute(&self, dry_run: bool) -> Result<()> {
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
                std::fs::remove_file(link_path)
                    .map_err(|e| StowError::io_error(link_path.clone(), e))?;

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
                std::fs::remove_file(path).map_err(|e| StowError::io_error(path.clone(), e))?;
            }
        }

        Action::RemoveEmptyDir { path } => {
            logger::action(&format!("Remove empty directory: {}", path.display()));

            if !dry_run {
                fs_ops::remove_empty_directory(path)?;
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
