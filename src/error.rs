use std::path::PathBuf;
use thiserror::Error;

/// Custom result type for winstow operations
pub type Result<T> = std::result::Result<T, StowError>;

/// Errors that can occur during stow operations
#[derive(Debug, Error)]
pub enum StowError {
    /// Permission denied when creating symlinks
    #[error(
        "Permission denied: {0}\n\nTo create symbolic links on Windows, you need to either:\n  - Enable Developer Mode (Settings > Update & Security > For developers)\n  - Run this program as Administrator"
    )]
    PermissionDenied(String),

    /// Conflicts with existing files or directories; nothing has been changed
    #[error(
        "Conflict: the following paths already exist and are not symlinks pointing to the package:\n{}\nNo changes have been made. To resolve the conflicts, you can:\n  - Use --adopt to move existing files into the package\n  - Use --override to replace existing files and directories (destructive)\n  - Manually remove or relocate the conflicting files",
        format_paths(.paths)
    )]
    Conflict { paths: Vec<PathBuf> },

    /// Package directory not found
    #[error("Package not found: '{package}' does not exist in stow directory '{stow_dir}'")]
    PackageNotFound { package: String, stow_dir: PathBuf },

    /// Invalid path provided
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    /// Symlink operation failed
    #[error("Symlink operation failed at {path}: {message}")]
    SymlinkError { path: PathBuf, message: String },

    /// General I/O error
    #[error("I/O error at {path}: {source}")]
    IoError {
        path: PathBuf,
        source: std::io::Error,
    },

    /// Directory is not empty and cannot be removed
    #[error("Directory is not empty: {0}")]
    DirectoryNotEmpty(PathBuf),

    /// Configuration file error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Pattern matching error
    #[error("Invalid pattern: {0}")]
    PatternError(String),
}

impl StowError {
    /// Create a new PermissionDenied error
    pub fn permission_denied(message: impl Into<String>) -> Self {
        StowError::PermissionDenied(message.into())
    }

    /// Create a new Conflict error for a single path
    #[cfg(test)]
    pub fn conflict(path: impl Into<PathBuf>) -> Self {
        StowError::Conflict {
            paths: vec![path.into()],
        }
    }

    /// Create a new Conflict error for several paths
    pub fn conflicts(paths: Vec<PathBuf>) -> Self {
        StowError::Conflict { paths }
    }

    /// Create a new PackageNotFound error
    pub fn package_not_found(package: impl Into<String>, stow_dir: impl Into<PathBuf>) -> Self {
        StowError::PackageNotFound {
            package: package.into(),
            stow_dir: stow_dir.into(),
        }
    }

    /// Create a new InvalidPath error
    pub fn invalid_path(message: impl Into<String>) -> Self {
        StowError::InvalidPath(message.into())
    }

    /// Create a new SymlinkError
    pub fn symlink_error(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        StowError::SymlinkError {
            path: path.into(),
            message: message.into(),
        }
    }

    /// Create a new IoError from an std::io::Error
    pub fn io_error(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        StowError::IoError {
            path: path.into(),
            source,
        }
    }

    /// Create a new DirectoryNotEmpty error
    pub fn directory_not_empty(path: impl Into<PathBuf>) -> Self {
        StowError::DirectoryNotEmpty(path.into())
    }

    /// Create a new ConfigError
    pub fn config_error(message: impl Into<String>) -> Self {
        StowError::ConfigError(message.into())
    }

    /// Create a new PatternError
    pub fn pattern_error(message: impl Into<String>) -> Self {
        StowError::PatternError(message.into())
    }
}

/// Format paths as an indented list, one per line
fn format_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| format!("  - {}\n", path.display()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_denied_error() {
        let err = StowError::permission_denied("test error");
        assert!(err.to_string().contains("Permission denied"));
        assert!(err.to_string().contains("Developer Mode"));
    }

    #[test]
    fn test_conflict_error() {
        let err = StowError::conflict(PathBuf::from("test/path"));
        assert!(err.to_string().contains("Conflict"));
        assert!(err.to_string().contains("test/path"));
    }

    #[test]
    fn conflicts_error_lists_every_path() {
        let err = StowError::conflicts(vec![PathBuf::from("first"), PathBuf::from("second")]);

        let message = err.to_string();

        assert!(message.contains("  - first\n"));
        assert!(message.contains("  - second\n"));
        assert!(message.contains("No changes have been made"));
    }

    #[test]
    fn test_package_not_found_error() {
        let err = StowError::package_not_found("mypackage", PathBuf::from("/stow"));
        assert!(err.to_string().contains("mypackage"));
        assert!(err.to_string().contains("/stow"));
    }

    #[test]
    fn test_invalid_path_error() {
        let err = StowError::invalid_path("invalid path");
        assert!(err.to_string().contains("Invalid path"));
    }
}
