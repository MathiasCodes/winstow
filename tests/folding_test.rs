// Integration tests for directory folding, unfolding and unstowing of directory links
// These tests run the winstow binary against temporary stow and target directories

mod common;

use common::{Sandbox, assert_success, exists_at_all, foreign_dir, is_symlink};
use std::fs;

#[test]
fn stow_missing_target_dir_creates_single_dir_link() {
    let sandbox = Sandbox::new();
    sandbox.package_file("yazi", "AppData/yazi/config/yazi.toml", "pkg");
    fs::create_dir_all(sandbox.target("AppData/yazi/state")).unwrap();

    let output = sandbox.winstow(&["yazi"]);

    assert_success(&output);
    assert!(is_symlink(&sandbox.target("AppData/yazi/config")));
    assert_eq!(
        fs::read_to_string(sandbox.target("AppData/yazi/config/yazi.toml")).unwrap(),
        "pkg"
    );
}

#[test]
fn stow_existing_real_dir_links_children_individually() {
    let sandbox = Sandbox::new();
    sandbox.package_file("yazi", "config/yazi.toml", "pkg");
    sandbox.package_file("yazi", "config/plugins/main.lua", "pkg");
    fs::create_dir_all(sandbox.target("config")).unwrap();

    let output = sandbox.winstow(&["yazi"]);

    assert_success(&output);
    assert!(!is_symlink(&sandbox.target("config")));
    assert!(is_symlink(&sandbox.target("config/yazi.toml")));
    assert!(is_symlink(&sandbox.target("config/plugins")));
}

#[test]
fn stow_already_folded_package_succeeds_without_changes() {
    let sandbox = Sandbox::new();
    sandbox.package_file("yazi", "config/yazi.toml", "pkg");
    assert_success(&sandbox.winstow(&["yazi"]));

    let output = sandbox.winstow(&["yazi"]);

    assert_success(&output);
    assert!(is_symlink(&sandbox.target("config")));
}

#[test]
fn unstow_folded_package_removes_dir_link_and_keeps_package() {
    let sandbox = Sandbox::new();
    let package_file = sandbox.package_file("yazi", "config/yazi.toml", "pkg");
    assert_success(&sandbox.winstow(&["yazi"]));

    let output = sandbox.winstow(&["-D", "yazi"]);

    assert_success(&output);
    assert!(!exists_at_all(&sandbox.target("config")));
    assert!(package_file.exists());
}

#[test]
fn restow_folded_package_recreates_dir_link() {
    let sandbox = Sandbox::new();
    sandbox.package_file("yazi", "config/yazi.toml", "pkg");
    assert_success(&sandbox.winstow(&["yazi"]));

    let output = sandbox.winstow(&["-R", "yazi"]);

    assert_success(&output);
    assert!(is_symlink(&sandbox.target("config")));
}

#[test]
fn unstow_parent_dir_with_foreign_content_keeps_parent_dir() {
    let sandbox = Sandbox::new();
    sandbox.package_file("yazi", "AppData/yazi/config/yazi.toml", "pkg");
    fs::create_dir_all(sandbox.target("AppData/yazi/state")).unwrap();
    assert_success(&sandbox.winstow(&["yazi"]));

    let output = sandbox.winstow(&["-D", "yazi"]);

    assert_success(&output);
    assert!(!exists_at_all(&sandbox.target("AppData/yazi/config")));
    assert!(sandbox.target("AppData/yazi/state").is_dir());
}

#[test]
fn unstow_dirs_left_empty_removes_them() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "a/b/file.txt", "pkg");
    fs::create_dir_all(sandbox.target("a/b")).unwrap();
    assert_success(&sandbox.winstow(&["pkg"]));

    let output = sandbox.winstow(&["-D", "pkg"]);

    assert_success(&output);
    assert!(!exists_at_all(&sandbox.target("a")));
}

#[test]
fn stow_second_package_into_folded_dir_unfolds_and_links_both() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkgA", ".config/foo/a.txt", "a");
    sandbox.package_file("pkgA", ".config/only_a/x.txt", "x");
    sandbox.package_file("pkgB", ".config/foo/b.txt", "b");
    assert_success(&sandbox.winstow(&["pkgA"]));
    assert!(is_symlink(&sandbox.target(".config")));

    let output = sandbox.winstow(&["pkgB"]);

    assert_success(&output);
    assert!(!is_symlink(&sandbox.target(".config")));
    assert!(!is_symlink(&sandbox.target(".config/foo")));
    assert!(is_symlink(&sandbox.target(".config/only_a")));
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/foo/a.txt")).unwrap(),
        "a"
    );
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/foo/b.txt")).unwrap(),
        "b"
    );
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/only_a/x.txt")).unwrap(),
        "x"
    );
}

#[test]
fn unstow_one_of_two_unfolded_packages_keeps_other_links() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkgA", ".config/foo/a.txt", "a");
    sandbox.package_file("pkgB", ".config/foo/b.txt", "b");
    assert_success(&sandbox.winstow(&["pkgA"]));
    assert_success(&sandbox.winstow(&["pkgB"]));

    let output = sandbox.winstow(&["-D", "pkgB"]);

    assert_success(&output);
    assert!(!exists_at_all(&sandbox.target(".config/foo/b.txt")));
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/foo/a.txt")).unwrap(),
        "a"
    );
}

#[test]
fn stow_same_file_into_folded_dir_fails_without_changes() {
    let sandbox = Sandbox::new();
    let file_a = sandbox.package_file("pkgA", ".config/foo/same.txt", "a");
    sandbox.package_file("pkgB", ".config/foo/same.txt", "b");
    assert_success(&sandbox.winstow(&["pkgA"]));

    let output = sandbox.winstow(&["pkgB"]);

    assert!(!output.status.success());
    assert!(is_symlink(&sandbox.target(".config")));
    assert_eq!(fs::read_to_string(file_a).unwrap(), "a");
}

#[test]
fn stow_same_file_into_folded_dir_with_override_keeps_other_package_files() {
    let sandbox = Sandbox::new();
    let file_a = sandbox.package_file("pkgA", ".config/foo/same.txt", "a");
    let other_a = sandbox.package_file("pkgA", ".config/foo/other.txt", "other");
    sandbox.package_file("pkgB", ".config/foo/same.txt", "b");
    assert_success(&sandbox.winstow(&["pkgA"]));

    let output = sandbox.winstow(&["--override", "pkgB"]);

    assert_success(&output);
    assert_eq!(fs::read_to_string(&file_a).unwrap(), "a");
    assert_eq!(fs::read_to_string(&other_a).unwrap(), "other");
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/foo/same.txt")).unwrap(),
        "b"
    );
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/foo/other.txt")).unwrap(),
        "other"
    );
}

#[test]
fn stow_over_dir_link_outside_stow_dir_fails_and_keeps_link() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "config/yazi.toml", "pkg");
    let foreign = foreign_dir(&sandbox);
    let link = sandbox.target("config");
    std::os::windows::fs::symlink_dir(&foreign, &link).unwrap();

    let output = sandbox.winstow(&["pkg"]);

    assert!(!output.status.success());
    assert!(is_symlink(&sandbox.target("config")));
    assert_eq!(
        fs::read_to_string(foreign.join("keep.txt")).unwrap(),
        "foreign"
    );
}
