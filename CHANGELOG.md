# Changelog

All notable changes to winstow will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `--no-folding` option and `no-folding` configuration key to create real directories instead of directory symlinks
- Unstowing refolds directories that only contain links into a single package directory, as GNU Stow does

### Fixed
- Stowing a package whose directory is already folded no longer fails with `WIN32_ERROR(183)`
- Unstowing and restowing folded directories no longer fails with "access denied"; directory symlinks are now removed with `remove_dir`
- Unstowing no longer fails with "Directory is not empty" when a parent directory contains other files; such directories are kept
- Unfolding a directory linked by another package now links both packages' contents instead of reporting a conflict
- `--override` no longer deletes files of another package through a folded directory link while unfolding
- `--adopt` and `--override` no longer move or remove files before all conflicts of a package are known; a package with an unresolvable conflict is left unchanged
- `--override` now replaces file links of other packages instead of failing with `WIN32_ERROR(183)`
- `--adopt` no longer tries to adopt a file in place of a package directory; this is reported as a conflict
- Stowing over a dangling symlink into the stow directory replaces it instead of failing with `WIN32_ERROR(183)`; dangling links pointing elsewhere are reported as conflicts
- Unstowing and restowing remove dangling links into the package left behind by renamed or deleted package files

### Changed
- Directory symlinks pointing outside the stow directory are reported as conflicts instead of being unfolded
- All conflicts of a package are reported at once instead of only the first one
- Dry runs list the files `--adopt` would move and `--override` would remove
- `--override` never removes symlinks pointing outside the stow directory

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

