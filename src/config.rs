use crate::error::{Result, StowError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::{env, fs};

/// Configuration for winstow
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Default stow directory
    #[serde(rename = "default-dir")]
    pub default_dir: Option<String>,

    /// Default target directory
    #[serde(rename = "default-target")]
    pub default_target: Option<String>,

    /// Default ignore patterns
    #[serde(default)]
    pub ignore: Vec<String>,

    /// Default defer patterns
    #[serde(default)]
    pub defer: Vec<String>,

    /// Default verbose mode
    #[serde(default)]
    pub verbose: bool,
}

impl Config {
    /// Load configuration from standard locations
    /// Checks in order:
    /// 1. .winstowrc in current directory
    /// 2. .winstowrc in home directory
    /// 3. config.toml in APPDATA/winstow/
    pub fn load() -> Result<Self> {
        // Try current directory
        if let Ok(cwd) = env::current_dir() {
            let config_path = cwd.join(".winstowrc");
            if config_path.exists() {
                return Self::load_from(&config_path);
            }
        }

        // Try home directory
        if let Ok(home) = env::var("USERPROFILE") {
            let config_path = PathBuf::from(home).join(".winstowrc");
            if config_path.exists() {
                return Self::load_from(&config_path);
            }
        }

        // Try APPDATA
        if let Ok(appdata) = env::var("APPDATA") {
            let config_path = PathBuf::from(appdata).join("winstow").join("config.toml");
            if config_path.exists() {
                return Self::load_from(&config_path);
            }
        }

        // No config file found, return default
        Ok(Self::default())
    }

    /// Load configuration from a specific file
    pub fn load_from(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path)
            .map_err(|e| StowError::config_error(format!("Failed to read config file: {}", e)))?;

        let config: Config = toml::from_str(&contents)
            .map_err(|e| StowError::config_error(format!("Failed to parse config file: {}", e)))?;

        Ok(config)
    }

    /// Merge this config with CLI arguments and create runtime context
    /// CLI arguments take precedence over config file settings
    #[allow(clippy::too_many_arguments)]
    pub fn merge_with_cli(
        &self,
        cli_dir: Option<PathBuf>,
        cli_target: Option<PathBuf>,
        cli_ignore: Vec<String>,
        cli_defer: Vec<String>,
        cli_verbose: bool,
        cli_dry_run: bool,
        cli_adopt: bool,
        cli_override_conflicts: bool,
    ) -> Result<StowContext> {
        // Get effective directories (use CLI, then config, then defaults)
        let stow_dir = cli_dir
            .or_else(|| self.default_dir.as_ref().map(PathBuf::from))
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let target_dir = cli_target
            .or_else(|| self.default_target.as_ref().map(PathBuf::from))
            .unwrap_or_else(|| {
                dirs::home_dir().unwrap_or_else(|| {
                    eprintln!(
                        "Warning: Could not determine home directory, using current directory"
                    );
                    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                })
            });

        let ignore = if cli_ignore.is_empty() {
            self.ignore.clone()
        } else {
            cli_ignore
        };

        let defer = if cli_defer.is_empty() {
            self.defer.clone()
        } else {
            cli_defer
        };

        let verbose = cli_verbose || self.verbose;

        StowContext::new(
            stow_dir,
            target_dir,
            ignore,
            defer,
            verbose,
            cli_dry_run,
            cli_adopt,
            cli_override_conflicts,
        )
    }
}

/// Runtime execution context after merging file config with CLI arguments
/// Contains all configuration and state needed for stow operations
#[derive(Debug, Clone)]
pub struct StowContext {
    /// Stow directory (normalized, required)
    stow_dir: PathBuf,
    /// Target directory (normalized, required)
    target_dir: PathBuf,
    /// Ignore patterns
    ignore: Vec<String>,
    /// Defer patterns
    defer: Vec<String>,
    /// Dry run mode
    dry_run: bool,
    /// Conflict resolution strategy
    conflict_strategy: crate::stow::ConflictStrategy,
}

impl StowContext {
    /// Create a new StowContext with all required runtime state
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        stow_dir: PathBuf,
        target_dir: PathBuf,
        ignore: Vec<String>,
        defer: Vec<String>,
        _verbose: bool,
        dry_run: bool,
        adopt: bool,
        override_conflicts: bool,
    ) -> crate::error::Result<Self> {
        use crate::logger;

        // Normalize paths
        let stow_dir = stow_dir.canonicalize().unwrap_or(stow_dir);
        let target_dir = target_dir.canonicalize().unwrap_or(target_dir);

        // Determine conflict strategy
        let conflict_strategy = if adopt {
            logger::verbose("  --adopt enabled: will move conflicting files into packages");
            crate::stow::ConflictStrategy::Adopt
        } else if override_conflicts {
            logger::warn("--override enabled: will remove conflicting files (destructive)");
            crate::stow::ConflictStrategy::Override
        } else {
            crate::stow::ConflictStrategy::Fail
        };

        // Log patterns if verbose
        if !ignore.is_empty() {
            logger::verbose(&format!("  Ignore patterns: {:?}", ignore));
        }
        if !defer.is_empty() {
            logger::verbose(&format!("  Defer patterns: {:?}", defer));
        }

        Ok(Self {
            stow_dir,
            target_dir,
            ignore,
            defer,
            dry_run,
            conflict_strategy,
        })
    }

    /// Get the stow directory
    pub fn stow_dir(&self) -> &Path {
        &self.stow_dir
    }

    /// Get the target directory
    pub fn target_dir(&self) -> &Path {
        &self.target_dir
    }

    /// Check if dry run mode is enabled
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Get the conflict strategy
    pub fn conflict_strategy(&self) -> crate::stow::ConflictStrategy {
        self.conflict_strategy
    }

    /// Build a pattern set from the ignore and defer patterns
    pub fn build_pattern_set(&self) -> crate::error::Result<crate::ignore::PatternSet> {
        crate::ignore::PatternSet::new(&self.ignore, &self.defer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.default_dir.is_none());
        assert!(config.default_target.is_none());
        assert!(config.ignore.is_empty());
        assert!(!config.verbose);
    }

    #[test]
    fn test_config_load_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".winstowrc");

        let mut file = fs::File::create(&config_path).unwrap();
        file.write_all(
            br#"
default-dir = "C:\\stow"
default-target = "C:\\target"
ignore = ["*.bak", ".DS_Store"]
defer = ["*.lock"]
verbose = true
"#,
        )
        .unwrap();

        let config = Config::load_from(&config_path).unwrap();
        assert_eq!(config.default_dir, Some("C:\\stow".to_string()));
        assert_eq!(config.default_target, Some("C:\\target".to_string()));
        assert_eq!(config.ignore, vec!["*.bak", ".DS_Store"]);
        assert_eq!(config.defer, vec!["*.lock"]);
        assert!(config.verbose);
    }

    #[test]
    fn test_config_invalid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".winstowrc");

        fs::write(&config_path, "invalid toml {{{").unwrap();

        let result = Config::load_from(&config_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_merge_cli_precedence() {
        let config = Config {
            default_dir: Some("C:\\config_stow".to_string()),
            default_target: Some("C:\\config_target".to_string()),
            ignore: vec!["*.config".to_string()],
            defer: vec!["*.config_defer".to_string()],
            verbose: false,
        };

        let merged = config
            .merge_with_cli(
                Some(PathBuf::from("C:\\cli_stow")),
                Some(PathBuf::from("C:\\cli_target")),
                vec!["*.cli".to_string()],
                vec!["*.cli_defer".to_string()],
                true,
                false, // dry_run
                false, // adopt
                false, // override_conflicts
            )
            .unwrap();

        // CLI should take precedence
        assert_eq!(merged.stow_dir(), Path::new("C:\\cli_stow"));
        assert_eq!(merged.target_dir(), Path::new("C:\\cli_target"));
    }

    #[test]
    fn test_config_merge_use_config_defaults() {
        let config = Config {
            default_dir: Some("C:\\config_stow".to_string()),
            default_target: Some("C:\\config_target".to_string()),
            ignore: vec!["*.config".to_string()],
            defer: vec!["*.config_defer".to_string()],
            verbose: true,
        };

        let merged = config
            .merge_with_cli(
                None,
                None,
                vec![],
                vec![],
                false,
                false, // dry_run
                false, // adopt
                false, // override_conflicts
            )
            .unwrap();

        // Should use config values
        assert_eq!(merged.stow_dir(), Path::new("C:\\config_stow"));
        assert_eq!(merged.target_dir(), Path::new("C:\\config_target"));
    }

    #[test]
    fn test_config_load_no_file() {
        // Loading from nonexistent file should return error
        let result = Config::load_from("/nonexistent/path/.winstowrc");
        assert!(result.is_err());
    }
}
