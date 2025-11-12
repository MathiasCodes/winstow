# Technical Concept: winstow

## Overview

**winstow** is a Windows-native command-line utility that manages symbolic link farms, closely mirroring the behavior of GNU Stow. It allows users to maintain software packages or configuration files in separate directories (the "stow directory") and present them as a unified directory tree in a target directory through symbolic links.

The application creates and removes relative symbolic links for both files and directories, implements intelligent directory folding and unfolding, detects conflicts, and prunes empty directories. It is designed as a single, statically-linked binary optimized for small size and fast execution.

---

## Core Functionality

### Purpose

winstow manages a symlink farm where:
- **Stow directory** (`--dir`): Contains package directories (e.g., `dotfiles`, `vim`, `git`)
- **Target directory** (`--target`): Where symlinks are created to make packages appear installed
- **Packages**: Subdirectories within the stow directory that contain files/directories to be linked

Example structure:
```
stow-dir/
  dotfiles/
    .config/
      vimrc
      gitconfig
  vim/
    .vim/
      vimrc
target-dir/  (typically $HOME)
  .config/ -> ../stow-dir/dotfiles/.config
  .vim/ -> ../stow-dir/vim/.vim
```

---

## Command Behavior

### `stow` Command

The `stow` command creates symbolic links from package directories to the target directory.

#### Basic Operation

1. **Package Discovery**: For each specified package name, locate the package directory in the stow directory.
2. **Tree Traversal**: Recursively enumerate all files and directories in the package.
3. **Link Creation**: For each file/directory, create a relative symbolic link in the target directory at the corresponding path.

#### Directory Folding

**Folding** is the optimization where an entire directory is linked as a single symlink instead of linking each child individually. This reduces the number of symlinks and improves performance.

**Folding Rules** (aligned with GNU Stow):
- A directory can be folded (linked as a single symlink) if:
  - The target path does not exist, OR
  - The target path is already a symlink pointing to the same package's directory
- A directory must be unfolded (replaced with a real directory containing child links) if:
  - The target path exists and is a symlink pointing to a different package's directory
  - The target path exists and is a regular file or directory (conflict)

**Folding Algorithm**:
1. For each directory in the package tree:
   - Check if the target path exists
   - If it exists and is a symlink to another package → **unfold** the existing link first
   - If it exists and is a non-link → **conflict** (fail or report)
   - If it does not exist → **fold** (create directory symlink)
   - If it exists and is a symlink to the same package → **fold** (no-op or verify)

2. After unfolding, create individual symlinks for all children of the unfolded directory.

#### File Linking

- Files are always linked individually (no folding for files).
- Use relative symlinks: compute the relative path from the link location to the target file.
- If the target file path already exists and is not a symlink → **conflict**.

#### Conflict Detection

A conflict occurs when:
- A target path exists and is a regular file or directory (not a symlink)
- A target path exists and is a symlink pointing to a different package

**Conflict Handling**:
- By default, winstow **fails fast** with a clear error message indicating the conflicting path.
- With `--adopt`: Move conflicting files from the target directory into the stow package, then create symlinks.
- With `--override`: Remove conflicting files in the target directory and create symlinks (use with caution).
- Conflicts are reported to stderr with actionable messages.

#### Relative Symlinks

- All symlinks use **relative paths** (not absolute) for portability.
- Compute relative paths using the link's parent directory and the target file/directory.
- Example: If linking `stow-dir/dotfiles/.config/vimrc` to `target-dir/.config/vimrc`, the symlink target should be `../../stow-dir/dotfiles/.config/vimrc` (relative from `target-dir/.config/`).

#### Path Normalization

- Normalize all paths to absolute paths internally for comparison.
- Handle Windows case-insensitivity: treat paths as case-insensitive for conflict detection.
- Support long paths (Windows `\\?\` prefix if needed).
- Normalize directory separators consistently.

---

### `unstow` Command

The `unstow` command removes symbolic links created by `stow` and prunes empty directories.

#### Basic Operation

1. **Link Discovery**: For each specified package, find all symlinks in the target directory that point to files/directories within that package.
2. **Link Removal**: Remove each symlink (both file and directory symlinks).
3. **Directory Pruning**: Remove empty directories that were created during stowing, working from leaves to root.

#### Link Resolution

- Resolve symlinks to determine their actual targets.
- Only remove symlinks that point to the specified package (ignore symlinks from other packages).
- Handle both absolute and relative symlinks correctly.

#### Pruning Algorithm

1. After removing all symlinks for a package:
   - Traverse the target directory tree from leaves to root.
   - For each directory:
     - If the directory is empty → remove it
     - If the directory contains only empty subdirectories → remove recursively
     - Stop at directories that contain non-empty content or symlinks from other packages

2. **Pruning Rules**:
   - Only prune directories that were created by winstow (or are now empty after link removal).
   - Do not prune directories that existed before stowing (unless they are now empty and were only created to house symlinks).
   - Be careful with case-insensitivity: ensure we don't prune directories that appear empty but have case-different entries.

#### Conflict Detection (Unstow)

- If attempting to remove a symlink but the target path is not a symlink → error (unexpected state).
- If a directory contains both symlinks from the package being unstowed and other content → only remove the package's symlinks, leave the directory if it still has content.

---

## Windows-Specific Behavior

### Symbolic Link Requirements

- **File symlinks**: Use Windows `CreateSymbolicLink` API with `SYMBOLIC_LINK_FLAG_FILE`.
- **Directory symlinks**: Use Windows `CreateSymbolicLink` API with `SYMBOLIC_LINK_FLAG_DIRECTORY`.

### Permissions

- Creating symlinks on Windows requires:
  - **Developer Mode enabled** (Windows 10/11), OR
  - **Administrator privileges** (elevated process)

**Error Handling**:
- If symlink creation fails due to insufficient permissions:
  - Detect the error (typically `UnauthorizedAccessException` or `ERROR_PRIVILEGE_NOT_HELD`).
  - Print a clear, actionable error message to stderr.
  - Exit with a non-zero exit code.
  - **Do not fall back to junctions** (winstow only uses true symlinks).

### Reparse Points

- Detect existing reparse points (symlinks, junctions, mount points) before creating links.
- When a target directory is a symlink/junction and needs to contain new children:
  - **Unfold** the existing link first (replace with a real directory).
  - Then create child symlinks within that directory.

### Path Handling

- **Case-insensitivity**: Windows paths are case-insensitive. Treat all path comparisons as case-insensitive.
- **Long paths**: Support paths up to 32,767 characters (use `\\?\` prefix if needed).
- **Directory separators**: Accept both `\` and `/`, normalize to `\` for Windows.
- **UNC paths**: Handle UNC paths (`\\server\share`) if needed.

### Junctions

- **Do not create junctions** as a fallback for symlinks.
- **Detect junctions** in the target directory (they are reparse points) and handle them appropriately (unfold if needed).
- If symlink creation is not possible, fail with an error (do not silently create junctions).

---

## CLI Interface

### Usage

```
winstow [OPTIONS] <PACKAGE>...
```

Like GNU Stow, winstow uses action flags rather than subcommands. The default action is to stow packages.

### Action Flags

- `-S, --stow` - Stow packages (default action, can be omitted)
- `-D, --delete` - Unstow (delete) packages
- `-R, --restow` - Restow packages (unstow then stow again)

Only one action flag can be specified at a time.

### Directory Options

- `-d, --dir <DIR>` - Specify the stow directory (default: current directory)
- `-t, --target <DIR>` - Specify the target directory (default: user's home directory)

### General Options

- `-v, --verbose` - Enable verbose output (show planned and executed actions)
- `-n, --dry-run` - Perform a trial run without making changes (show what would be done)
- `-h, --help` - Show help message and exit
- `-V, --version` - Show version and exit

### Conflict Resolution Options

These options only apply to stow and restow actions:

- `--adopt` - Move conflicting files from target into stow package, then create symlinks
- `--override` - Remove conflicting files in target directory and create symlinks (destructive)

### Pattern Options

These options only apply to stow and restow actions:

- `--ignore <PATTERN>` - Skip files/directories matching the pattern (can be used multiple times)
- `--defer <PATTERN>` - Skip stowing files matching the pattern only if they already exist in target (can be used multiple times)

### Examples

```bash
# Stow a package (default action)
winstow mypackage

# Stow multiple packages
winstow vim git dotfiles

# Stow with verbose and dry-run
winstow -v -n mypackage

# Unstow a package
winstow -D mypackage

# Restow a package (refresh symlinks)
winstow -R mypackage

# Stow from specific directory to specific target
winstow -d /path/to/stow -t /path/to/target mypackage

# Stow with adopt (move existing files into package)
winstow --adopt mypackage

# Stow with ignore patterns
winstow --ignore "*.bak" --ignore ".DS_Store" mypackage
```

### Output

- **Default (quiet mode)**: Minimal output, only errors and warnings to stderr.
- **Verbose mode**: Print each planned action (e.g., "Linking file: target/.config/vimrc -> ../../stow-dir/dotfiles/.config/vimrc").
- **Dry-run mode**: Print actions that would be performed, prefixed with `[DRY RUN]` or similar.
- **Errors**: All errors go to stderr with clear, actionable messages.
- **Exit codes**: 
  - `0` - Success
  - `1` - General error (conflicts, permission errors, invalid arguments)
  - `2` - Usage error (invalid options, missing arguments)

---

## Deviations from GNU Stow

### Intentional Deviations

1. **No Junction Fallback**: GNU Stow does not apply to Windows, but if it did, winstow explicitly does not fall back to junctions when symlinks fail. It fails with a clear error instead.

2. **Windows Path Semantics**: 
   - Case-insensitive path comparisons
   - Long path support with `\\?\` prefix
   - Windows-specific error messages for permission issues

3. **Configuration File Format**: winstow uses a Windows-appropriate configuration file location (`%USERPROFILE%\.winstowrc` or `%APPDATA%\winstow\config.toml`) rather than Unix-style dotfiles in `$HOME`.

### Aligned Behaviors

1. **Folding/Unfolding**: Matches GNU Stow's directory folding algorithm.
2. **Conflict Detection**: Same conflict detection rules (fail on non-link files/dirs).
3. **Relative Symlinks**: Uses relative paths like GNU Stow.
4. **Pruning**: Same pruning behavior (remove empty directories after unstowing).
5. **CLI Structure**: Similar command structure and options.
6. **Ignore Patterns**: Supports ignore patterns like GNU Stow.
7. **Adopt and Override**: Supports `--adopt` and `--override` options like GNU Stow.

---

## Technical Decisions

### Programming Language: Rust

**Rationale**:
- **Native performance**: Compiles to native machine code, no runtime overhead.
- **Small binaries**: With size optimizations, produces binaries ~1-3 MB (vs. ~5-10 MB for C# NativeAOT).
- **Memory safety**: Ownership model prevents common bugs without garbage collection overhead.
- **Windows-focused**: Designed specifically for Windows, leveraging platform-specific APIs effectively.
- **Strong ecosystem**: Excellent crates for CLI (`clap`), Windows APIs (`windows-rs`), and filesystem operations.

**Size Optimizations**:
```toml
[profile.release]
opt-level = "z"        # Optimize for size
lto = true             # Link-time optimization
codegen-units = 1      # Better optimization
strip = true           # Remove debug symbols
panic = "abort"        # No unwind tables
```

### Windows API Access

**Decision**: Use `windows-rs` crate (modern, type-safe Windows API bindings).

**Rationale**:
- Type-safe bindings generated from Windows metadata.
- Modern Rust idioms (Result types, etc.).
- Comprehensive coverage of Windows APIs needed (file system, symlinks, reparse points).
- Alternative `winapi` is older and less ergonomic.

**Key APIs**:
- `CreateSymbolicLinkW` for symlink creation
- `GetFileAttributesW` for reparse point detection
- `DeviceIoControl` with `FSCTL_GET_REPARSE_POINT` for detailed reparse point info (if needed)

### CLI Parsing: `clap`

**Decision**: Use `clap` with derive macros.

**Rationale**:
- Industry standard for Rust CLI applications.
- Excellent help message generation.
- Supports all needed features (subcommands, options, arguments).
- Can be optimized for size (disable features like colors, suggestions if not needed).

**Example**:
```rust
#[derive(Parser)]
#[command(name = "stow")]
struct Args {
  #[arg(short, long)]
  dir: Option<PathBuf>,
  
  #[arg(short = 't', long)]
  target: Option<PathBuf>,
  
  #[arg(short, long)]
  verbose: bool,
  
  packages: Vec<String>,
}
```

### Project Structure

```
winstow/
  src/
    main.rs           # CLI entry point, argument parsing
    stower.rs         # Core stow/unstow logic
    fs_ops.rs         # Windows filesystem operations (symlinks, reparse points)
    path_utils.rs     # Path normalization, relative path computation
    planner.rs        # Folding/unfolding planning logic
    adopt.rs          # Adopt and override functionality
    ignore.rs         # Ignore pattern matching
    config.rs         # Configuration file parsing
  tests/
    integration/      # Integration tests with temp directories
      stow_tests.rs
      unstow_tests.rs
      folding_tests.rs
      conflict_tests.rs
      adopt_tests.rs
      ignore_tests.rs
  Cargo.toml
  README.md
```

### Error Handling

**Decision**: Use Rust's `Result<T, E>` types throughout, custom error types.

**Rationale**:
- Explicit error handling (no exceptions).
- Type-safe error propagation.
- Clear error messages for users.

**Error Types**:
```rust
#[derive(Debug, thiserror::Error)]
enum StowError {
  #[error("Permission denied: {0}. Enable Developer Mode or run as Administrator.")]
  PermissionDenied(String),
  
  #[error("Conflict: {0} already exists and is not a symlink")]
  Conflict(String),
  
  #[error("Package not found: {0}")]
  PackageNotFound(String),
  
  // ... more error variants
}
```

### Testing Strategy

**Decision**: Comprehensive unit and integration tests using Rust's built-in test framework.

**Approach**:
- **Unit tests**: Test individual functions (path normalization, relative path computation, folding logic).
- **Integration tests**: Test full stow/unstow operations with temporary directories.
- **Test scenarios**:
  - Basic stow/unstow of files and directories
  - Folding and unfolding behavior
  - Conflict detection
  - Pruning empty directories
  - Relative symlink creation
  - Windows-specific edge cases (case-insensitivity, long paths, reparse points)

**Test Utilities**:
- Create temporary directories for each test.
- Verify symlink targets are correct.
- Clean up after tests.

### Dependencies (Minimal)

**Core Dependencies**:
- `clap` - CLI parsing
- `windows-rs` - Windows API bindings
- `thiserror` - Error type definitions (small, compile-time only)
- `glob` or `regex` - Pattern matching for ignore/defer functionality
- `toml` - Configuration file parsing (optional, if config files are implemented)

**Development Dependencies**:
- Standard Rust test framework (built-in)
- `tempfile` - Temporary directories for tests (dev-only)

**Rationale**: Keep dependencies minimal to reduce binary size and attack surface, while providing full functionality.

### Build and Distribution

**Build Command**:
```bash
cargo build --release
```

**Output**: Single statically-linked binary (`winstow.exe` on Windows).

**Distribution**:
- Release binary can be distributed standalone (no runtime dependencies).
- GitHub Releases for distribution with pre-built binaries.
- Package managers: Provide packages for Scoop and WinGet for easy installation.
- Optional MSI installer for enterprise deployment.

---

## Additional Features

### Adopt Functionality

The `--adopt` option allows winstow to handle pre-existing files in the target directory gracefully:

**Behavior**:
1. When a conflict is detected (file exists in target and is not a symlink):
2. Move the conflicting file from the target directory into the stow package directory.
3. Create the symlink from the target to the now-updated package file.

**Use Case**: Useful when you have existing configuration files in your home directory and want to start managing them with stow.

**Safety Considerations**:
- Create backups before moving files (or provide `--no-backup` option).
- Verify file permissions are preserved during the move.
- Log all adopted files clearly.

### Override Functionality

The `--override` option removes conflicting files to allow stowing:

**Behavior**:
1. When a conflict is detected (file exists in target and is not a symlink):
2. Remove the conflicting file from the target directory.
3. Create the symlink from the target to the package file.

**Use Case**: Useful when you want to replace existing files with stowed versions.

**Safety Considerations**:
- Warn user about destructive nature of this operation.
- Recommend dry-run first (`-n --override`).
- Consider creating backups by default.

### Ignore Patterns

The `--ignore <PATTERN>` option allows excluding files/directories from stowing:

**Pattern Syntax**:
- Glob patterns (e.g., `*.bak`, `*.tmp`, `.DS_Store`)
- Applied recursively during tree traversal

**Use Cases**:
- Exclude editor backup files
- Exclude OS-specific metadata files
- Exclude build artifacts or temporary files

**Implementation**:
- Check patterns before creating symlinks
- Skip ignored files/directories entirely
- Log ignored items in verbose mode

### Defer Patterns

The `--defer <PATTERN>` option allows conditional exclusion based on target existence (matching GNU Stow behavior):

**Behavior**:
- If a file matches the defer pattern AND the target already exists → skip it
- If a file matches the defer pattern AND the target doesn't exist → stow it normally

**Use Cases**:
- Shared configuration files managed by different packages
- Allow one package to "own" a file while other packages defer to it
- Example: Multiple packages contain `.bashrc`, but only the first one stowed will manage it

**Implementation**:
- Check defer patterns only when target path exists
- If target exists and matches defer pattern, skip stowing
- If target doesn't exist, stow normally regardless of defer pattern
- Log deferred items in verbose mode

**Difference from `--ignore`**:
- `--ignore`: Unconditionally skip files (they will never be stowed)
- `--defer`: Conditionally skip files only if target already exists (allows first package to stow, subsequent packages defer)

### Configuration Files

Support for persistent configuration:

**File Locations** (in order of precedence):
1. `.winstowrc` in current directory
2. `%USERPROFILE%\.winstowrc`
3. `%APPDATA%\winstow\config.toml`

**Configuration Options**:
```toml
# Default stow directory
default-dir = "C:\\stow"

# Default target directory
default-target = "C:\\Users\\YourName"

# Default ignore patterns
ignore = ["*.bak", "*.tmp", ".DS_Store"]

# Default defer patterns
defer = ["*.lock"]

# Enable verbose mode by default
verbose = false
```

### Restow Command

The `restow` command combines unstow and stow operations:

**Behavior**:
1. Unstow the specified packages (remove existing symlinks).
2. Stow the packages again (create fresh symlinks).

**Use Cases**:
- Refresh symlinks after package updates
- Fix broken symlinks
- Re-apply stow configuration

**Implementation**:
- Atomic operation: if unstow fails, don't proceed to stow
- Log both operations clearly in verbose mode

---

## Future Enhancements (Long-term)

1. **Parallel operations**: Stow/unstow multiple packages in parallel for performance.
2. **GUI wrapper**: Optional graphical interface for users who prefer visual management.
3. **Package verification**: Verify symlink integrity and report broken links.
4. **Conflict resolution wizard**: Interactive mode for resolving conflicts.
5. **Backup management**: Built-in backup and restore functionality for safety.

---

## Success Criteria

The application is considered complete when:

1. ✅ Can stow packages (create relative symlinks for files and directories)
2. ✅ Can unstow packages (remove symlinks and prune empty directories)
3. ✅ Can restow packages (refresh symlinks)
4. ✅ Implements directory folding and unfolding correctly
5. ✅ Detects and reports conflicts clearly
6. ✅ Supports `--adopt` to move conflicting files into packages
7. ✅ Supports `--override` to remove conflicting files
8. ✅ Supports ignore patterns to exclude files from stowing
9. ✅ Supports defer patterns to skip specific files
10. ✅ Reads configuration files for persistent settings
11. ✅ Handles Windows permissions gracefully (clear error messages)
12. ✅ Produces a small, statically-linked binary (< 5 MB)
13. ✅ Has comprehensive tests covering all core behaviors
14. ✅ CLI matches GNU Stow's interface closely (where applicable)
15. ✅ Documentation is clear and complete
16. ✅ Available through package managers (Scoop, WinGet)

---

## References

- GNU Stow Manual: https://www.gnu.org/software/stow/manual/
- Windows Symbolic Links: https://learn.microsoft.com/en-us/windows/win32/fileio/symbolic-links
- Rust `windows-rs`: https://github.com/microsoft/windows-rs
- Rust `clap`: https://github.com/clap-rs/clap

