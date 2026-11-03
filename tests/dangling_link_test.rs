// Integration tests for dangling symlinks in the target directory
// Dangling links into the stow directory are replaced or removed, others are conflicts

mod common;

use common::{Sandbox, assert_success, exists_at_all, is_symlink};
use std::fs;
use std::os::windows::fs::{symlink_dir, symlink_file};

#[test]
fn stow_over_dangling_file_link_into_stow_dir_replaces_it() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "a.txt", "pkg");
    symlink_file(
        sandbox.stow_dir().join("pkg").join("gone.txt"),
        sandbox.target("a.txt"),
    )
    .unwrap();

    let output = sandbox.winstow(&["pkg"]);

    assert_success(&output);
    assert_eq!(fs::read_to_string(sandbox.target("a.txt")).unwrap(), "pkg");
}

#[test]
fn stow_over_dangling_dir_link_into_stow_dir_replaces_it() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "config/a.txt", "pkg");
    symlink_dir(
        sandbox.stow_dir().join("pkg").join("old-config"),
        sandbox.target("config"),
    )
    .unwrap();

    let output = sandbox.winstow(&["pkg"]);

    assert_success(&output);
    assert!(is_symlink(&sandbox.target("config")));
    assert_eq!(
        fs::read_to_string(sandbox.target("config/a.txt")).unwrap(),
        "pkg"
    );
}

#[test]
fn stow_over_dangling_link_outside_stow_dir_fails_and_keeps_link() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "a.txt", "pkg");
    symlink_file(
        sandbox.root.path().join("elsewhere.txt"),
        sandbox.target("a.txt"),
    )
    .unwrap();

    let output = sandbox.winstow(&["pkg"]);

    assert!(!output.status.success());
    assert!(is_symlink(&sandbox.target("a.txt")));
}

#[test]
fn unstow_removes_dangling_link_of_renamed_package_file() {
    let sandbox = Sandbox::new();
    let old_file = sandbox.package_file("pkg", "dir/old.txt", "pkg");
    sandbox.package_file("pkg", "dir/keep.txt", "pkg");
    fs::create_dir_all(sandbox.target("dir/user")).unwrap();
    assert_success(&sandbox.winstow(&["pkg"]));
    fs::rename(&old_file, old_file.with_file_name("new.txt")).unwrap();

    let output = sandbox.winstow(&["-D", "pkg"]);

    assert_success(&output);
    assert!(!exists_at_all(&sandbox.target("dir/old.txt")));
    assert!(!exists_at_all(&sandbox.target("dir/keep.txt")));
    assert!(sandbox.target("dir/user").is_dir());
}

#[test]
fn unstow_keeps_dangling_link_into_other_package() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "dir/a.txt", "pkg");
    sandbox.package_file("other", "dir/b.txt", "other");
    fs::create_dir_all(sandbox.target("dir")).unwrap();
    assert_success(&sandbox.winstow(&["pkg"]));
    symlink_file(
        sandbox
            .stow_dir()
            .join("other")
            .join("dir")
            .join("gone.txt"),
        sandbox.target("dir/gone.txt"),
    )
    .unwrap();

    let output = sandbox.winstow(&["-D", "pkg"]);

    assert_success(&output);
    assert!(is_symlink(&sandbox.target("dir/gone.txt")));
}

#[test]
fn restow_after_rename_links_new_name_and_removes_old_link() {
    let sandbox = Sandbox::new();
    let old_file = sandbox.package_file("pkg", "dir/old.txt", "pkg");
    fs::create_dir_all(sandbox.target("dir")).unwrap();
    assert_success(&sandbox.winstow(&["pkg"]));
    fs::rename(&old_file, old_file.with_file_name("new.txt")).unwrap();

    let output = sandbox.winstow(&["-R", "pkg"]);

    assert_success(&output);
    assert!(!exists_at_all(&sandbox.target("dir/old.txt")));
    assert_eq!(
        fs::read_to_string(sandbox.target("dir/new.txt")).unwrap(),
        "pkg"
    );
}
