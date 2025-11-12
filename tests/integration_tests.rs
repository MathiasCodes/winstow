// Integration tests for winstow
// These tests verify end-to-end workflows using real filesystem operations
// All tests use temporary directories that are automatically cleaned up

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

// Helper to create a test package structure
fn create_test_package(stow_dir: &PathBuf, package_name: &str) -> PathBuf {
    let package_dir = stow_dir.join(package_name);
    fs::create_dir_all(&package_dir).unwrap();
    package_dir
}

// Helper to create a file with content
fn create_file_with_content(path: &PathBuf, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut file = File::create(path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
}

#[test]
fn test_full_stow_unstow_cycle() {
    let temp = TempDir::new().unwrap();
    let stow_dir = temp.path().join("stow");
    let target_dir = temp.path().join("target");
    fs::create_dir(&stow_dir).unwrap();
    fs::create_dir(&target_dir).unwrap();

    // Create a package with files
    let package = create_test_package(&stow_dir, "mypackage");
    create_file_with_content(&package.join("file1.txt"), "content1");
    create_file_with_content(&package.join("file2.txt"), "content2");

    // TODO: Stow the package
    // For now, just verify the structure was created
    assert!(package.join("file1.txt").exists());
    assert!(package.join("file2.txt").exists());

    // TODO: Verify symlinks were created
    // TODO: Unstow the package
    // TODO: Verify symlinks were removed
}

#[test]
fn test_nested_directory_structure() {
    let temp = TempDir::new().unwrap();
    let stow_dir = temp.path().join("stow");
    let target_dir = temp.path().join("target");
    fs::create_dir(&stow_dir).unwrap();
    fs::create_dir(&target_dir).unwrap();

    // Create a package with nested directories
    let package = create_test_package(&stow_dir, "nested");
    create_file_with_content(&package.join("a/b/c/deep.txt"), "deep content");
    create_file_with_content(&package.join("a/b/shallow.txt"), "shallow content");
    create_file_with_content(&package.join("a/top.txt"), "top content");

    assert!(package.join("a/b/c/deep.txt").exists());
    assert!(package.join("a/b/shallow.txt").exists());
    assert!(package.join("a/top.txt").exists());
}

#[test]
fn test_multiple_packages() {
    let temp = TempDir::new().unwrap();
    let stow_dir = temp.path().join("stow");
    let target_dir = temp.path().join("target");
    fs::create_dir(&stow_dir).unwrap();
    fs::create_dir(&target_dir).unwrap();

    // Create multiple packages
    let pkg1 = create_test_package(&stow_dir, "package1");
    create_file_with_content(&pkg1.join("file1.txt"), "pkg1 content");

    let pkg2 = create_test_package(&stow_dir, "package2");
    create_file_with_content(&pkg2.join("file2.txt"), "pkg2 content");

    let pkg3 = create_test_package(&stow_dir, "package3");
    create_file_with_content(&pkg3.join("file3.txt"), "pkg3 content");

    assert!(pkg1.join("file1.txt").exists());
    assert!(pkg2.join("file2.txt").exists());
    assert!(pkg3.join("file3.txt").exists());
}

#[test]
fn test_conflicting_files() {
    let temp = TempDir::new().unwrap();
    let stow_dir = temp.path().join("stow");
    let target_dir = temp.path().join("target");
    fs::create_dir(&stow_dir).unwrap();
    fs::create_dir(&target_dir).unwrap();

    // Create a package
    let package = create_test_package(&stow_dir, "mypackage");
    create_file_with_content(&package.join("conflict.txt"), "package content");

    // Create a conflicting file in target
    create_file_with_content(&target_dir.join("conflict.txt"), "existing content");

    assert!(target_dir.join("conflict.txt").exists());

    // TODO: Test that stowing without --adopt or --override fails
    // TODO: Test that stowing with --adopt works
    // TODO: Test that stowing with --override works
}

#[test]
fn test_ignore_patterns() {
    let temp = TempDir::new().unwrap();
    let stow_dir = temp.path().join("stow");
    let target_dir = temp.path().join("target");
    fs::create_dir(&stow_dir).unwrap();
    fs::create_dir(&target_dir).unwrap();

    // Create a package with files that should be ignored
    let package = create_test_package(&stow_dir, "mypackage");
    create_file_with_content(&package.join("important.txt"), "keep this");
    create_file_with_content(&package.join("backup.bak"), "ignore this");
    create_file_with_content(&package.join(".DS_Store"), "ignore this too");

    assert!(package.join("important.txt").exists());
    assert!(package.join("backup.bak").exists());
    assert!(package.join(".DS_Store").exists());

    // TODO: Stow with --ignore "*.bak" --ignore ".DS_Store"
    // TODO: Verify only important.txt was linked
}

#[test]
fn test_empty_directory_pruning() {
    let temp = TempDir::new().unwrap();
    let stow_dir = temp.path().join("stow");
    let target_dir = temp.path().join("target");
    fs::create_dir(&stow_dir).unwrap();
    fs::create_dir(&target_dir).unwrap();

    // Create nested empty directories
    let package = create_test_package(&stow_dir, "mypackage");
    fs::create_dir_all(package.join("empty/nested/deep")).unwrap();
    create_file_with_content(&package.join("empty/nested/deep/file.txt"), "content");

    assert!(package.join("empty/nested/deep/file.txt").exists());

    // TODO: Stow the package
    // TODO: Unstow the package
    // TODO: Verify all empty directories were pruned
}

#[test]
fn test_directory_folding() {
    let temp = TempDir::new().unwrap();
    let stow_dir = temp.path().join("stow");
    let target_dir = temp.path().join("target");
    fs::create_dir(&stow_dir).unwrap();
    fs::create_dir(&target_dir).unwrap();

    // Create a package with a directory that should be folded
    let package = create_test_package(&stow_dir, "mypackage");
    fs::create_dir(package.join("mydir")).unwrap();
    create_file_with_content(&package.join("mydir/file1.txt"), "content1");
    create_file_with_content(&package.join("mydir/file2.txt"), "content2");

    assert!(package.join("mydir").exists());

    // TODO: Stow the package
    // TODO: Verify the entire directory was folded (single symlink, not individual files)
}

#[test]
fn test_directory_unfolding() {
    let temp = TempDir::new().unwrap();
    let stow_dir = temp.path().join("stow");
    let target_dir = temp.path().join("target");
    fs::create_dir(&stow_dir).unwrap();
    fs::create_dir(&target_dir).unwrap();

    // Create two packages that will cause unfolding
    let pkg1 = create_test_package(&stow_dir, "package1");
    fs::create_dir(pkg1.join("shared")).unwrap();
    create_file_with_content(&pkg1.join("shared/file1.txt"), "pkg1");

    let pkg2 = create_test_package(&stow_dir, "package2");
    fs::create_dir(pkg2.join("shared")).unwrap();
    create_file_with_content(&pkg2.join("shared/file2.txt"), "pkg2");

    assert!(pkg1.join("shared/file1.txt").exists());
    assert!(pkg2.join("shared/file2.txt").exists());

    // TODO: Stow package1 (should fold)
    // TODO: Stow package2 (should unfold, then link both files)
    // TODO: Verify both files are linked in target/shared/
}

#[test]
fn test_config_file_integration() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join(".winstowrc");

    // Create a config file
    let mut config_file = File::create(&config_path).unwrap();
    config_file
        .write_all(
            br#"
default-dir = "my_stow"
default-target = "my_target"
ignore = ["*.bak", ".DS_Store"]
verbose = true
"#,
        )
        .unwrap();

    assert!(config_path.exists());

    // TODO: Load config and verify values
    // TODO: Test that CLI args override config values
}

#[test]
fn test_restow_updates_links() {
    let temp = TempDir::new().unwrap();
    let stow_dir = temp.path().join("stow");
    let target_dir = temp.path().join("target");
    fs::create_dir(&stow_dir).unwrap();
    fs::create_dir(&target_dir).unwrap();

    // Create a package
    let package = create_test_package(&stow_dir, "mypackage");
    create_file_with_content(&package.join("file.txt"), "original");

    // TODO: Stow the package
    // TODO: Modify the package (add new file)
    // TODO: Restow the package
    // TODO: Verify new file is linked
}
