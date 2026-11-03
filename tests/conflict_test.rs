// Integration tests for conflict detection, --adopt and --override
// Conflicts must be detected before anything is changed

mod common;

use common::{Sandbox, assert_success, foreign_dir, is_symlink, output_text};
use std::fs;

#[test]
fn stow_with_two_conflicts_reports_both_and_changes_nothing() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "first.txt", "pkg");
    sandbox.package_file("pkg", "second.txt", "pkg");
    sandbox.package_file("pkg", "free.txt", "pkg");
    fs::write(sandbox.target("first.txt"), "user").unwrap();
    fs::write(sandbox.target("second.txt"), "user").unwrap();

    let output = sandbox.winstow(&["pkg"]);

    assert!(!output.status.success());
    let text = output_text(&output);
    assert!(text.contains("first.txt"), "{text}");
    assert!(text.contains("second.txt"), "{text}");
    assert!(!sandbox.target("free.txt").exists());
}

#[test]
fn stow_adopt_moves_file_into_package_and_links_it() {
    let sandbox = Sandbox::new();
    let package_file = sandbox.package_file("pkg", "config.toml", "pkg");
    fs::write(sandbox.target("config.toml"), "user").unwrap();

    let output = sandbox.winstow(&["--adopt", "pkg"]);

    assert_success(&output);
    assert!(is_symlink(&sandbox.target("config.toml")));
    assert_eq!(fs::read_to_string(&package_file).unwrap(), "user");
}

#[test]
fn stow_adopt_with_unresolvable_conflict_adopts_nothing() {
    let sandbox = Sandbox::new();
    let package_file = sandbox.package_file("pkg", "a.txt", "pkg");
    sandbox.package_file("pkg", "config/b.txt", "pkg");
    fs::write(sandbox.target("a.txt"), "user").unwrap();
    std::os::windows::fs::symlink_dir(foreign_dir(&sandbox), sandbox.target("config")).unwrap();

    let output = sandbox.winstow(&["--adopt", "pkg"]);

    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(sandbox.target("a.txt")).unwrap(), "user");
    assert!(!is_symlink(&sandbox.target("a.txt")));
    assert_eq!(fs::read_to_string(&package_file).unwrap(), "pkg");
}

#[test]
fn stow_adopt_file_in_place_of_package_dir_conflicts() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "config/a.txt", "pkg");
    fs::write(sandbox.target("config"), "user").unwrap();

    let output = sandbox.winstow(&["--adopt", "pkg"]);

    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(sandbox.target("config")).unwrap(),
        "user"
    );
}

#[test]
fn stow_override_dry_run_removes_nothing() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "config.toml", "pkg");
    fs::write(sandbox.target("config.toml"), "user").unwrap();

    let output = sandbox.winstow(&["-n", "--override", "pkg"]);

    assert_success(&output);
    assert!(output_text(&output).contains("Override (remove)"));
    assert_eq!(
        fs::read_to_string(sandbox.target("config.toml")).unwrap(),
        "user"
    );
}

#[test]
fn stow_override_replaces_file_in_place_of_package_dir() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "config/a.txt", "pkg");
    fs::write(sandbox.target("config"), "user").unwrap();

    let output = sandbox.winstow(&["--override", "pkg"]);

    assert_success(&output);
    assert!(is_symlink(&sandbox.target("config")));
    assert_eq!(
        fs::read_to_string(sandbox.target("config/a.txt")).unwrap(),
        "pkg"
    );
}

#[test]
fn stow_override_replaces_file_link_of_other_package() {
    let sandbox = Sandbox::new();
    let file_a = sandbox.package_file("pkgA", "same.txt", "a");
    sandbox.package_file("pkgB", "same.txt", "b");
    assert_success(&sandbox.winstow(&["pkgA"]));

    let output = sandbox.winstow(&["--override", "pkgB"]);

    assert_success(&output);
    assert_eq!(fs::read_to_string(sandbox.target("same.txt")).unwrap(), "b");
    assert_eq!(fs::read_to_string(&file_a).unwrap(), "a");
}

#[test]
fn stow_override_with_link_outside_stow_dir_removes_nothing() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "a.txt", "pkg");
    sandbox.package_file("pkg", "config/b.txt", "pkg");
    fs::write(sandbox.target("a.txt"), "user").unwrap();
    let foreign = foreign_dir(&sandbox);
    std::os::windows::fs::symlink_dir(&foreign, sandbox.target("config")).unwrap();

    let output = sandbox.winstow(&["--override", "pkg"]);

    assert!(!output.status.success());
    assert!(output_text(&output).contains("config"));
    assert_eq!(fs::read_to_string(sandbox.target("a.txt")).unwrap(), "user");
    assert!(is_symlink(&sandbox.target("config")));
    assert!(foreign.join("keep.txt").exists());
}
