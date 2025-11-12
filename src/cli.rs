use clap::Parser;
use std::path::PathBuf;

/// Windows-native symlink farm manager inspired by GNU Stow
#[derive(Parser, Debug)]
#[command(name = "winstow")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Stow packages (default action)
    #[arg(short = 'S', long = "stow")]
    pub stow: bool,

    /// Unstow (delete) packages
    #[arg(short = 'D', long = "delete")]
    pub delete: bool,

    /// Restow packages (unstow then stow)
    #[arg(short = 'R', long = "restow")]
    pub restow: bool,

    /// Stow directory containing packages (default: current directory)
    #[arg(short = 'd', long = "dir")]
    pub stow_dir: Option<PathBuf>,

    /// Target directory where symlinks will be created (default: user's home directory)
    #[arg(short = 't', long = "target")]
    pub target_dir: Option<PathBuf>,

    /// Enable verbose output
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Perform a dry run without making any changes
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,

    /// Move conflicting files into the package (adopt)
    #[arg(long = "adopt")]
    pub adopt: bool,

    /// Remove conflicting files (destructive)
    #[arg(long = "override")]
    pub override_conflicts: bool,

    /// Skip files matching pattern (can be used multiple times)
    #[arg(long = "ignore", value_name = "PATTERN")]
    pub ignore: Vec<String>,

    /// Skip files matching pattern if they already exist in target (can be used multiple times)
    #[arg(long = "defer", value_name = "PATTERN")]
    pub defer: Vec<String>,

    /// Package names to operate on
    #[arg(value_name = "PACKAGE", required = true)]
    pub packages: Vec<String>,
}

/// Action to perform (derived from CLI flags)
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Stow,
    Delete,
    Restow,
}

impl Cli {
    /// Determine which action to perform based on flags
    /// Returns error if multiple conflicting actions are specified
    /// Default action is Stow if no action flag is specified
    pub fn action(&self) -> Result<Action, String> {
        let action_count = [self.stow, self.delete, self.restow]
            .iter()
            .filter(|&&x| x)
            .count();

        match action_count {
            0 => Ok(Action::Stow), // Default action
            1 => {
                if self.delete {
                    Ok(Action::Delete)
                } else if self.restow {
                    Ok(Action::Restow)
                } else {
                    Ok(Action::Stow)
                }
            }
            _ => Err(
                "Multiple actions specified. Use only one of: -S/--stow, -D/--delete, -R/--restow"
                    .to_string(),
            ),
        }
    }

    /// Check if adopt or override flags are used with delete action (which is invalid)
    pub fn validate_flags(&self) -> Result<(), String> {
        let action = self.action()?;

        if action == Action::Delete && (self.adopt || self.override_conflicts) {
            return Err("--adopt and --override cannot be used with -D/--delete".to_string());
        }

        if action == Action::Delete && (!self.ignore.is_empty() || !self.defer.is_empty()) {
            return Err("--ignore and --defer cannot be used with -D/--delete".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_action_is_stow() {
        let cli = Cli::try_parse_from(["winstow", "mypackage"]).unwrap();
        assert_eq!(cli.action().unwrap(), Action::Stow);
        assert_eq!(cli.packages, vec!["mypackage"]);
    }

    #[test]
    fn test_explicit_stow_action() {
        let cli = Cli::try_parse_from(["winstow", "-S", "mypackage"]).unwrap();
        assert_eq!(cli.action().unwrap(), Action::Stow);
        assert!(cli.stow);
    }

    #[test]
    fn test_delete_action() {
        let cli = Cli::try_parse_from(["winstow", "-D", "mypackage"]).unwrap();
        assert_eq!(cli.action().unwrap(), Action::Delete);
        assert!(cli.delete);
    }

    #[test]
    fn test_restow_action() {
        let cli = Cli::try_parse_from(["winstow", "-R", "mypackage"]).unwrap();
        assert_eq!(cli.action().unwrap(), Action::Restow);
        assert!(cli.restow);
    }

    #[test]
    fn test_multiple_actions_error() {
        let cli = Cli::try_parse_from(["winstow", "-S", "-D", "mypackage"]).unwrap();
        assert!(cli.action().is_err());
    }

    #[test]
    fn test_verbose_and_dry_run_flags() {
        let cli = Cli::try_parse_from(["winstow", "-v", "-n", "mypackage"]).unwrap();
        assert!(cli.verbose);
        assert!(cli.dry_run);
    }

    #[test]
    fn test_adopt_with_stow() {
        let cli = Cli::try_parse_from(["winstow", "--adopt", "mypackage"]).unwrap();
        assert!(cli.adopt);
        assert_eq!(cli.action().unwrap(), Action::Stow);
        assert!(cli.validate_flags().is_ok());
    }

    #[test]
    fn test_adopt_with_delete_is_invalid() {
        let cli = Cli::try_parse_from(["winstow", "-D", "--adopt", "mypackage"]).unwrap();
        assert!(cli.validate_flags().is_err());
    }

    #[test]
    fn test_multiple_packages() {
        let cli = Cli::try_parse_from(["winstow", "pkg1", "pkg2", "pkg3"]).unwrap();
        assert_eq!(cli.packages, vec!["pkg1", "pkg2", "pkg3"]);
    }

    #[test]
    fn test_ignore_patterns() {
        let cli = Cli::try_parse_from([
            "winstow",
            "--ignore",
            "*.bak",
            "--ignore",
            "*.tmp",
            "mypackage",
        ])
        .unwrap();
        assert_eq!(cli.ignore, vec!["*.bak", "*.tmp"]);
    }
}
