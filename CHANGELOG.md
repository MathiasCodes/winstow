# Changelog

All notable changes to winstow will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [v0.3.0] - 2025-01-13

### Changed
- Statically linked Visual C++ runtime (VCRUNTIME140.dll) to eliminate vcredist2022 dependency
- Binary now only depends on Universal C Runtime (UCRT) which ships with Windows 10/11

### Technical
- Added `static_vcruntime` crate for hybrid linking approach
- Reduced external dependencies for easier distribution

## [v0.2.1] - 2025-01-13

### Fixed
- Version metadata in Cargo.toml now correctly reflects the release version (v0.2.0 had incorrect version metadata)

### Note
- This is a patch release to correct the version mismatch in v0.2.0
- No functional changes from v0.2.0

## [v0.2.0] - 2025-01-12

⚠️ **Note**: This release has a version metadata issue (binary reports v0.1.0). No functional changes from v0.1.0

### Added
- GitHub Actions workflow for automated releases
- Automated SHA256 hash calculation in releases

### Changed
- Release artifacts now include both standalone `.exe` and `.zip` package
- Improved release documentation with checksums

## [v0.1.0] - 2025-01-11

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

