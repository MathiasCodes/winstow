// Integration tests for --no-folding
// With folding disabled, directories are created as real directories and only files are linked

mod common;

use common::{Sandbox, assert_success, exists_at_all, is_symlink};
use std::fs;

#[test]
fn stow_no_folding_creates_real_dirs_and_file_links() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "config/sub/a.txt", "pkg");

    let output = sandbox.winstow(&["--no-folding", "pkg"]);

    assert_success(&output);
    assert!(sandbox.target("config/sub").is_dir());
    assert!(!is_symlink(&sandbox.target("config")));
    assert!(!is_symlink(&sandbox.target("config/sub")));
    assert!(is_symlink(&sandbox.target("config/sub/a.txt")));
    assert_eq!(
        fs::read_to_string(sandbox.target("config/sub/a.txt")).unwrap(),
        "pkg"
    );
}

#[test]
fn stow_no_folding_from_config_file_creates_real_dirs() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "config/a.txt", "pkg");
    fs::write(
        sandbox.root.path().join(".winstowrc"),
        "no-folding = true\n",
    )
    .unwrap();

    let output = sandbox.winstow(&["pkg"]);

    assert_success(&output);
    assert!(!is_symlink(&sandbox.target("config")));
    assert!(is_symlink(&sandbox.target("config/a.txt")));
}

#[test]
fn stow_no_folding_into_folded_dir_unfolds_without_dir_links() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkgA", ".config/foo/a.txt", "a");
    sandbox.package_file("pkgA", ".config/only_a/x.txt", "x");
    sandbox.package_file("pkgB", ".config/foo/b.txt", "b");
    assert_success(&sandbox.winstow(&["pkgA"]));

    let output = sandbox.winstow(&["--no-folding", "pkgB"]);

    assert_success(&output);
    assert!(!is_symlink(&sandbox.target(".config")));
    assert!(!is_symlink(&sandbox.target(".config/foo")));
    assert!(!is_symlink(&sandbox.target(".config/only_a")));
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
fn unstow_after_no_folding_stow_removes_created_dirs() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "config/sub/a.txt", "pkg");
    assert_success(&sandbox.winstow(&["--no-folding", "pkg"]));

    let output = sandbox.winstow(&["-D", "pkg"]);

    assert_success(&output);
    assert!(!exists_at_all(&sandbox.target("config")));
}
