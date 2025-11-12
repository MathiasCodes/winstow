# Changelog

All notable changes to winstow will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v0.1.0]

### Added
- Stow operation: create symbolic links from packages to target directory
- Unstow operation: remove symbolic links and prune empty directories
- Restow operation: refresh symbolic links
- Directory folding/unfolding matching GNU Stow behavior
- Conflict detection and reporting
- `--adopt` option to move conflicting files into packages
- `--override` option to remove conflicting files
- `--ignore` and `--defer` pattern matching for file exclusion
- Configuration file support (`.winstowrc`, TOML format)
- Verbose mode (`-v`) and dry-run mode (`-n`)
- Windows-specific symlink handling with proper error messages
- Case-insensitive path handling for Windows
- Tests covering core functionality and edge cases

