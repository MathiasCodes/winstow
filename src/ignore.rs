use crate::error::{Result, StowError};
use glob::Pattern;
use std::path::Path;

/// Pattern matcher for ignore and defer rules
#[derive(Debug, Clone)]
pub struct PatternMatcher {
    patterns: Vec<Pattern>,
}

impl PatternMatcher {
    /// Create a new PatternMatcher from a list of glob patterns
    pub fn new(pattern_strings: &[String]) -> Result<Self> {
        let mut patterns = Vec::new();

        for pattern_str in pattern_strings {
            let pattern = Pattern::new(pattern_str).map_err(|e| {
                StowError::pattern_error(format!("Invalid pattern '{}': {}", pattern_str, e))
            })?;
            patterns.push(pattern);
        }

        Ok(Self { patterns })
    }

    /// Check if a path matches any of the patterns
    pub fn matches(&self, path: impl AsRef<Path>) -> bool {
        if self.patterns.is_empty() {
            return false;
        }

        let path = path.as_ref();

        // Pre-compute string representations once
        let path_str = path.to_string_lossy();
        let filename_str = path.file_name().map(|f| f.to_string_lossy());

        // Iterate patterns once and check all variants for each pattern
        for pattern in &self.patterns {
            // Check full path
            if pattern.matches(&path_str) {
                return true;
            }

            // Check filename
            if let Some(ref fname) = filename_str
                && pattern.matches(fname)
            {
                return true;
            }

            // Check components
            for component in path.components() {
                let component_str = component.as_os_str().to_string_lossy();
                if pattern.matches(&component_str) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if the matcher is empty (no patterns)
    ///
    /// See also: [`len`](Self::len)
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Get the number of patterns
    ///
    /// See also: [`is_empty`](Self::is_empty)
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.patterns.len()
    }
}

/// Container for both ignore and defer patterns
#[derive(Debug, Clone)]
pub struct PatternSet {
    ignore: PatternMatcher,
    defer: PatternMatcher,
}

impl PatternSet {
    /// Create a new PatternSet from ignore and defer pattern strings
    pub fn new(ignore_patterns: &[String], defer_patterns: &[String]) -> Result<Self> {
        Ok(Self {
            ignore: PatternMatcher::new(ignore_patterns)?,
            defer: PatternMatcher::new(defer_patterns)?,
        })
    }

    /// Create an empty PatternSet
    #[allow(dead_code)]
    pub fn empty() -> Self {
        Self {
            ignore: PatternMatcher {
                patterns: Vec::new(),
            },
            defer: PatternMatcher {
                patterns: Vec::new(),
            },
        }
    }

    /// Check if a path should be ignored
    pub fn should_ignore(&self, path: impl AsRef<Path>) -> bool {
        self.ignore.matches(path)
    }

    /// Check if a path should be deferred
    pub fn should_defer(&self, path: impl AsRef<Path>) -> bool {
        self.defer.matches(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_pattern_matcher_creation() {
        let patterns = vec!["*.bak".to_string(), "*.tmp".to_string()];
        let matcher = PatternMatcher::new(&patterns).unwrap();
        assert_eq!(matcher.len(), 2);
        assert!(!matcher.is_empty());
    }

    #[test]
    fn test_pattern_matcher_empty() {
        let matcher = PatternMatcher::new(&[]).unwrap();
        assert!(matcher.is_empty());
        assert_eq!(matcher.len(), 0);
    }

    #[test]
    fn test_invalid_pattern() {
        let patterns = vec!["[invalid".to_string()];
        assert!(PatternMatcher::new(&patterns).is_err());
    }

    #[test]
    fn test_matches_filename() {
        let patterns = vec!["*.bak".to_string()];
        let matcher = PatternMatcher::new(&patterns).unwrap();

        assert!(matcher.matches(PathBuf::from("file.bak")));
        assert!(matcher.matches(PathBuf::from("dir/file.bak")));
        assert!(!matcher.matches(PathBuf::from("file.txt")));
    }

    #[test]
    fn test_matches_exact_name() {
        let patterns = vec![".DS_Store".to_string()];
        let matcher = PatternMatcher::new(&patterns).unwrap();

        assert!(matcher.matches(PathBuf::from(".DS_Store")));
        assert!(matcher.matches(PathBuf::from("dir/.DS_Store")));
        assert!(!matcher.matches(PathBuf::from("DS_Store")));
    }

    #[test]
    fn test_matches_path_component() {
        let patterns = vec!["node_modules".to_string()];
        let matcher = PatternMatcher::new(&patterns).unwrap();

        assert!(matcher.matches(PathBuf::from("node_modules")));
        assert!(matcher.matches(PathBuf::from("node_modules/package")));
        assert!(matcher.matches(PathBuf::from("dir/node_modules/file")));
    }

    #[test]
    fn test_multiple_patterns() {
        let patterns = vec!["*.bak".to_string(), "*.tmp".to_string(), ".git".to_string()];
        let matcher = PatternMatcher::new(&patterns).unwrap();

        assert!(matcher.matches(PathBuf::from("file.bak")));
        assert!(matcher.matches(PathBuf::from("file.tmp")));
        assert!(matcher.matches(PathBuf::from(".git")));
        assert!(!matcher.matches(PathBuf::from("file.txt")));
    }

    #[test]
    fn test_pattern_set() {
        let ignore = vec!["*.bak".to_string()];
        let defer = vec!["*.lock".to_string()];
        let set = PatternSet::new(&ignore, &defer).unwrap();

        assert!(set.should_ignore(PathBuf::from("file.bak")));
        assert!(!set.should_ignore(PathBuf::from("file.lock")));

        assert!(set.should_defer(PathBuf::from("file.lock")));
        assert!(!set.should_defer(PathBuf::from("file.bak")));
    }

    #[test]
    fn test_pattern_set_empty() {
        let set = PatternSet::empty();

        assert!(!set.should_ignore(PathBuf::from("file.txt")));
        assert!(!set.should_defer(PathBuf::from("file.txt")));
    }
}
