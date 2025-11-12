# Winstow

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Winstow** is a Windows-native symlink farm manager inspired by [GNU Stow](https://www.gnu.org/software/stow/). It helps you manage your dotfiles, configuration files, and software packages by creating symbolic links from a central stow directory to a target directory (typically your home directory).

## Disclaimer

I created Winstow to replicate my GNU Stow dotfile management workflow from Linux on Windows - keeping all my configuration files in a single git repository and symlinking them as needed.

⚠️ Note: Winstow is currently in active development and may be unstable. The API is subject to change without prior notice. Please use it with caution and make backups of your files before using it. 

I'm still learning Rust, so the code may not be the most idiomatic or efficient. Suggestions for improvement are welcome.

⚠️ Winstow is not a 1:1 port of GNU Stow and does not guarantee identical range of functionality or behavior.

## Why Winstow?

- ✅ **Windows-Native**: Built specifically for Windows with proper symlink support
- ✅ **GNU Stow Compatible**: Familiar interface if you've used GNU Stow
- ✅ **Directory Folding**: Intelligent directory symlinking reduces link count
- ✅ **Conflict Resolution**: `--adopt` and `--override` for handling existing files
- ✅ **Pattern Matching**: `--ignore` and `--defer` for flexible file management
- ✅ **Configuration Files**: Set defaults with `.winstowrc`
- ✅ **Small & Fast**: Single ~600KB binary
- ✅ **Safe**: Dry-run mode to preview changes before applying

## Prerequisites

To create symbolic links on Windows, you need **one** of the following:

1. **Developer Mode enabled** (Windows 10/11) - *Recommended*
   - Settings → Update & Security → For developers → Developer Mode
   
2. **Administrator privileges**
   - Run winstow from an elevated command prompt

## Installation

### Option 1: Download Binary

Download the latest release from [GitHub Releases](https://github.com/MathiasCodes/winstow/releases) and add it to your PATH.

### Option 2: Build from Source

```bash
# Clone the repository
git clone https://github.com/MathiasCodes/winstow.git
cd winstow

# Build release binary
cargo build --release

# Binary will be at target/release/winstow.exe
```

### Option 3: Package Managers (Coming Soon)

```powershell
# Scoop
scoop install winstow

# WinGet
winget install winstow
```

## Quick Start

### Basic Usage

```bash
# Stow a package (create symlinks)
winstow mypackage

# Unstow a package (remove symlinks)
winstow -D mypackage

# Restow a package (refresh symlinks)
winstow -R mypackage

# Dry-run to see what would happen
winstow -n mypackage

# Verbose output
winstow -v mypackage
```

### Directory Structure

```
# Stow directory (contains all your dotfile packages)
C:\Users\USER\Dotfiles\
  ├── Git\
  │   └── .gitconfig
  ├── Git-Bash\
  │   ├── .bashrc
  │   └── .inputrc
  └── lazygit\
      └── AppData\
          └── Local\
              └── lazygit\
                  └── config.yaml

# Target directory (where symlinks are created, typically your home directory)
# Note: lazygit uses directory folding - a single directory symlink instead of individual file symlinks
C:\Users\USER\
  ├── .gitconfig -> C:\Users\USER\Dotfiles\Git\.gitconfig
  ├── .bashrc -> C:\Users\USER\Dotfiles\Git-Bash\.bashrc
  ├── .inputrc -> C:\Users\USER\Dotfiles\Git-Bash\.inputrc
  └── AppData\
      └── Local\
          └── lazygit -> C:\Users\USER\Dotfiles\lazygit\AppData\Local\lazygit
```

## Command-Line Interface

### Actions

| Flag | Long Form | Description |
|------|-----------|-------------|
| `-S` (optional) | `--stow` | Stow packages (default action) |
| `-D` | `--delete` | Unstow (delete) packages |
| `-R` | `--restow` | Restow packages (unstow then stow) |

### Options

| Flag | Long Form | Description |
|------|-----------|-------------|
| `-d DIR` | `--dir DIR` | Stow directory (default: current directory) |
| `-t DIR` | `--target DIR` | Target directory (default: home directory) |
| `-v` | `--verbose` | Enable verbose output |
| `-n` | `--dry-run` | Preview changes without applying them |
| | `--adopt` | Move conflicting files into package (stow/restow only) |
| | `--override` | Remove conflicting files (stow/restow only, destructive) |
| | `--ignore PATTERN` | Skip files matching pattern (stow/restow only) |
| | `--defer PATTERN` | Skip files matching pattern if they already exist in target (stow/restow only) |
| `-h` | `--help` | Show help message |
| `-V` | `--version` | Show version |

## Examples

### Managing Dotfiles

**PowerShell:**
```powershell
# Set up your stow directory
mkdir $env:USERPROFILE\Dotfiles
cd $env:USERPROFILE\Dotfiles
mkdir Git
mkdir Git-Bash

# Move your dotfiles into the packages
move $env:USERPROFILE\.gitconfig Git\
move $env:USERPROFILE\.bashrc Git-Bash\
move $env:USERPROFILE\.inputrc Git-Bash\

# Stow the packages
winstow -d $env:USERPROFILE\Dotfiles -t $env:USERPROFILE Git
winstow -d $env:USERPROFILE\Dotfiles -t $env:USERPROFILE Git-Bash

# Now .gitconfig, .bashrc, and .inputrc are symlinks to your Dotfiles packages
```

**Git Bash:**
```bash
# Set up your stow directory
mkdir -p $USERPROFILE/Dotfiles
cd $USERPROFILE/Dotfiles
mkdir Git
mkdir Git-Bash

# Move your dotfiles into the packages
mv $USERPROFILE/.gitconfig Git/
mv $USERPROFILE/.bashrc Git-Bash/
mv $USERPROFILE/.inputrc Git-Bash/

# Stow the packages
winstow -d $USERPROFILE/Dotfiles -t $USERPROFILE Git
winstow -d $USERPROFILE/Dotfiles -t $USERPROFILE Git-Bash

# Now .gitconfig, .bashrc, and .inputrc are symlinks to your Dotfiles packages
```

### Handling Conflicts

```bash
# If files already exist in target, you'll get an error
winstow mypackage
# Error: Conflict: C:\Users\You\.gitconfig already exists

# Option 1: Adopt the existing file into the package
winstow --adopt mypackage
# Moves .gitconfig into mypackage/, then creates symlink

# Option 2: Override (remove) the existing file
winstow --override mypackage
# Removes .gitconfig, then creates symlink (destructive!)
```

### Using Ignore and Defer Patterns

```bash
# Ignore backup files and OS metadata (always skip these)
winstow --ignore "*.bak" --ignore ".DS_Store" --ignore "Thumbs.db" mypackage

# Ignore entire directories
winstow --ignore "node_modules" mypackage

# Defer allows another package to manage shared files
# If .bashrc already exists, skip it; otherwise, stow it
winstow --defer ".bashrc" package-a

# Later, package-b can manage .bashrc
winstow package-b  # This will stow .bashrc from package-b
```

**Difference between `--ignore` and `--defer`:**
- `--ignore`: Always skip files matching the pattern (e.g., temporary files, build artifacts)
- `--defer`: Skip files matching the pattern only if they already exist in the target directory (useful for shared configuration files managed by different packages)

### Restowing After Updates

**PowerShell:**
```powershell
# After updating your dotfiles repository
cd $env:USERPROFILE\Dotfiles
git pull

# Refresh symlinks
winstow -R -d $env:USERPROFILE\Dotfiles -t $env:USERPROFILE Git
```

**Git Bash:**
```bash
# After updating your dotfiles repository
cd $USERPROFILE/Dotfiles
git pull

# Refresh symlinks
winstow -R -d $USERPROFILE/Dotfiles -t $USERPROFILE Git
```

### Dry-Run Mode

**PowerShell:**
```powershell
# See what would happen without making changes
winstow -n -v Git

# Output:
# === DRY RUN MODE - No changes will be made ===
# [DRY RUN] Create file link: .gitconfig -> ..\Dotfiles\Git\.gitconfig
# Would stow 1 package(s)
```

**Git Bash:**
```bash
# See what would happen without making changes
winstow -n -v Git

# Output:
# === DRY RUN MODE - No changes will be made ===
# [DRY RUN] Create file link: .gitconfig -> ..\Dotfiles\Git\.gitconfig
# Would stow 1 package(s)
```

## Configuration File

Create a `.winstowrc` file in one of these locations:

1. Current directory: `./.winstowrc`
2. Home directory: `%USERPROFILE%\.winstowrc`
3. AppData: `%APPDATA%\winstow\config.toml`

### Example Configuration

```toml
# Default stow directory
default-dir = "C:\\stow"

# Default target directory  
default-target = "C:\\Users\\YourName"

# Default ignore patterns
ignore = ["*.bak", ".DS_Store", "Thumbs.db", "desktop.ini"]

# Default defer patterns
defer = ["*.lock"]

# Enable verbose mode by default
verbose = false
```

CLI arguments always override config file settings.

## Directory Folding

winstow implements directory folding (inspired by GNU Stow) for efficiency:

### Folding (Creating a Single Directory Symlink)

```
# Package structure:
C:\Users\USER\Dotfiles\mypackage\AppData\Local\app\settings.json

# If C:\Users\USER\AppData\Local\app\ doesn't exist, winstow creates:
C:\Users\USER\AppData\Local\app -> C:\Users\USER\Dotfiles\mypackage\AppData\Local\app
```

### Unfolding (Expanding When Needed)

```
# First package creates a directory symlink:
C:\Users\USER\AppData\Local -> C:\Users\USER\Dotfiles\package1\AppData\Local

# Second package needs different AppData\Local contents:
# winstow automatically unfolds:
C:\Users\USER\AppData\Local\
  ├── app1 -> C:\Users\USER\Dotfiles\package1\AppData\Local\app1
  └── app2 -> C:\Users\USER\Dotfiles\package2\AppData\Local\app2
```

## Best Practices

1. **Always use `-n` first** to preview changes before applying them
2. **Use `-v` for debugging** to see detailed operations and understand what's happening
3. **Set up `.winstowrc`** for common settings to avoid repeating command-line options
4. **Keep packages focused** - one purpose per package (e.g., separate vim, git, bash packages)
5. **Test in a temp directory** before applying to your real dotfiles
6. **Use version control** (git) in your stow directory to track changes
7. **Document your packages** with README files explaining what each package contains
8. **Backup before `--override`** - this option is destructive and removes existing files!
9. **Backup before `--adopt`** - this option is destructive and removes existing files!

## Troubleshooting

### "Permission denied" Error

**Problem**: Cannot create symbolic links

**Solutions**:
1. Enable Developer Mode (Settings → For developers)
2. Run as Administrator
3. Check Windows version (requires Windows Vista+)

### Symlinks Not Working

**Problem**: Symlinks appear as files or don't work

**Check**:
- Developer Mode is enabled OR running as Administrator
- Using proper Windows symlink support (not shortcuts)
- Target files/directories exist

### "Conflict" Error

**Problem**: `Conflict: path already exists`

**Solutions**:
```bash
# Option 1: Adopt existing file
winstow --adopt mypackage

# Option 2: Override existing file
winstow --override mypackage

# Option 3: Manually remove/backup the file
move conflicting-file conflicting-file.backup
winstow mypackage
```

### Dry-Run Shows Different Results

**Problem**: Dry-run output doesn't match actual execution

**Note**: Dry-run simulates operations but doesn't account for all dynamic conditions. Always make sure you have a backup of your files.

## Development

### Building

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run tests including ignored ones (requires Developer Mode)
cargo test -- --ignored
```

### Project Structure

```
winstow/
├── src/
│   ├── main.rs          # Entry point and CLI routing
│   ├── cli.rs           # Command-line argument parsing
│   ├── config.rs        # Configuration file handling
│   ├── error.rs         # Error types
│   ├── logger.rs        # Logging infrastructure
│   ├── path_utils.rs    # Path manipulation utilities
│   ├── fs_ops.rs        # Windows filesystem operations
│   ├── planner.rs       # Action planning and execution
│   ├── stow.rs          # Stow operation logic
│   ├── unstow.rs        # Unstow operation logic
│   ├── adopt.rs         # Adopt/override functionality
│   └── ignore.rs        # Pattern matching
├── tests/
│   └── integration_tests.rs  # Integration tests
├── docs/
│   └── TECHNICAL_CONCEPT.md  # Technical documentation
├── Cargo.toml
├── README.md
├── LICENSE
├── CHANGELOG.md
└── DISTRIBUTION.md
```

### Testing

All tests use temporary directories and should be safe to run:

```bash
# All tests (unit + integration)
cargo test

# Just integration tests
cargo test --test integration_tests

# Test with output
cargo test -- --nocapture

# Tests requiring symlink creation (needs Developer Mode)
cargo test -- --ignored
```

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Acknowledgments

- Inspired by [GNU Stow](https://www.gnu.org/software/stow/) by Bob Glickstein
- Built with [Rust](https://www.rust-lang.org/)
- Uses [clap](https://github.com/clap-rs/clap) for CLI parsing
- Uses [windows-rs](https://github.com/microsoft/windows-rs) for Windows API access
- Uses [thiserror](https://github.com/dtolnay/thiserror) for error handling
- Uses [serde](https://github.com/serde-rs/serde) for serialization/deserialization
- Uses [toml](https://github.com/toml-rs/toml) for configuration file parsing
- Uses [glob](https://github.com/rust-lang/glob) for pattern matching
- Uses [dirs](https://github.com/soc/dirs-rs) for platform-specific directory paths

## Support

- 📖 [Documentation](README.md)
- 🐛 [Issue Tracker](https://github.com/MathiasCodes/winstow/issues)
- 💬 [Discussions](https://github.com/MathiasCodes/winstow/discussions)
