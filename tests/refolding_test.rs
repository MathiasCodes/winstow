// Integration tests for refolding directories on unstow
// A directory left with links into a single package directory becomes a directory link again

mod common;

use common::{Sandbox, assert_success, is_symlink};
use std::fs;

/// Stow pkgA and pkgB, which share `.config/foo`, so that both are unfolded
fn stow_two_packages_sharing_dir(sandbox: &Sandbox) {
    sandbox.package_file("pkgA", ".config/foo/a.txt", "a");
    sandbox.package_file("pkgB", ".config/foo/b.txt", "b");
    assert_success(&sandbox.winstow(&["pkgA"]));
    assert_success(&sandbox.winstow(&["pkgB"]));
    assert!(!is_symlink(&sandbox.target(".config")));
}

#[test]
fn unstow_second_package_refolds_dirs_to_first_package() {
    let sandbox = Sandbox::new();
    stow_two_packages_sharing_dir(&sandbox);

    let output = sandbox.winstow(&["-D", "pkgB"]);

    assert_success(&output);
    assert!(is_symlink(&sandbox.target(".config")));
    assert_eq!(
        fs::canonicalize(sandbox.target(".config")).unwrap(),
        fs::canonicalize(sandbox.stow_dir().join("pkgA").join(".config")).unwrap()
    );
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/foo/a.txt")).unwrap(),
        "a"
    );
    assert!(!sandbox.target(".config/foo/b.txt").exists());
}

#[test]
fn unstow_dir_with_real_file_left_does_not_refold() {
    let sandbox = Sandbox::new();
    stow_two_packages_sharing_dir(&sandbox);
    fs::write(sandbox.target(".config/foo/user.txt"), "user").unwrap();

    let output = sandbox.winstow(&["-D", "pkgB"]);

    assert_success(&output);
    assert!(!is_symlink(&sandbox.target(".config")));
    assert!(!is_symlink(&sandbox.target(".config/foo")));
    assert!(is_symlink(&sandbox.target(".config/foo/a.txt")));
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/foo/user.txt")).unwrap(),
        "user"
    );
}

#[test]
fn unstow_no_folding_does_not_refold() {
    let sandbox = Sandbox::new();
    stow_two_packages_sharing_dir(&sandbox);

    let output = sandbox.winstow(&["-D", "--no-folding", "pkgB"]);

    assert_success(&output);
    assert!(!is_symlink(&sandbox.target(".config")));
    assert!(!is_symlink(&sandbox.target(".config/foo")));
    assert!(is_symlink(&sandbox.target(".config/foo/a.txt")));
}

#[test]
fn unstow_dir_with_links_into_two_packages_does_not_refold() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkgA", ".config/a/x.txt", "a");
    sandbox.package_file("pkgB", ".config/b/y.txt", "b");
    sandbox.package_file("pkgC", ".config/c/z.txt", "c");
    for package in ["pkgA", "pkgB", "pkgC"] {
        assert_success(&sandbox.winstow(&[package]));
    }

    let output = sandbox.winstow(&["-D", "pkgC"]);

    assert_success(&output);
    assert!(!is_symlink(&sandbox.target(".config")));
    assert!(is_symlink(&sandbox.target(".config/a")));
    assert!(is_symlink(&sandbox.target(".config/b")));
}

#[test]
fn unstow_dry_run_with_refold_changes_nothing() {
    let sandbox = Sandbox::new();
    stow_two_packages_sharing_dir(&sandbox);

    let output = sandbox.winstow(&["-n", "-D", "pkgB"]);

    assert_success(&output);
    assert!(!is_symlink(&sandbox.target(".config")));
    assert!(is_symlink(&sandbox.target(".config/foo/b.txt")));
}

#[test]
fn restow_one_of_two_packages_keeps_both_linked() {
    let sandbox = Sandbox::new();
    stow_two_packages_sharing_dir(&sandbox);

    let output = sandbox.winstow(&["-R", "pkgB"]);

    assert_success(&output);
    assert!(!is_symlink(&sandbox.target(".config")));
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/foo/a.txt")).unwrap(),
        "a"
    );
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/foo/b.txt")).unwrap(),
        "b"
    );
}
