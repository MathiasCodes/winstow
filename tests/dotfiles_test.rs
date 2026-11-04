mod common;

use common::{Sandbox, assert_success, is_symlink, output_text};
use std::fs;

#[test]
fn dotfiles_stow_restow_and_unstow_use_translated_names() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "dot-bashrc", "bash");
    sandbox.package_file("pkg", "dot-config/init", "config");

    assert_success(&sandbox.winstow(&["--dotfiles", "pkg"]));
    assert!(is_symlink(&sandbox.target(".bashrc")));
    assert!(is_symlink(&sandbox.target(".config")));
    assert!(!sandbox.target("dot-bashrc").exists());

    assert_success(&sandbox.winstow(&["--dotfiles", "-R", "pkg"]));
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/init")).unwrap(),
        "config"
    );

    assert_success(&sandbox.winstow(&["--dotfiles", "-D", "pkg"]));
    assert!(!sandbox.target(".bashrc").exists());
    assert!(!sandbox.target(".config").exists());
    assert!(sandbox.stow_dir().join("pkg/dot-bashrc").exists());
}

#[test]
fn dotfiles_unfolding_matches_translated_directory_names() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkgA", ".config/a.txt", "a");
    sandbox.package_file("pkgB", "dot-config/dot-b.txt", "b");

    assert_success(&sandbox.winstow(&["--dotfiles", "pkgA"]));
    assert!(is_symlink(&sandbox.target(".config")));
    assert_success(&sandbox.winstow(&["--dotfiles", "pkgB"]));

    assert!(!is_symlink(&sandbox.target(".config")));
    assert!(is_symlink(&sandbox.target(".config/a.txt")));
    assert!(is_symlink(&sandbox.target(".config/.b.txt")));
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/.b.txt")).unwrap(),
        "b"
    );
    assert!(!sandbox.target(".config/dot-b.txt").exists());
}

#[test]
fn dotfiles_unfolding_translates_nested_names() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkgA", ".config/nvim/a.txt", "a");
    sandbox.package_file("pkgB", "dot-config/nvim/dot-b.txt", "b");

    assert_success(&sandbox.winstow(&["--dotfiles", "pkgA"]));
    assert_success(&sandbox.winstow(&["--dotfiles", "pkgB"]));

    assert!(!is_symlink(&sandbox.target(".config")));
    assert!(!is_symlink(&sandbox.target(".config/nvim")));
    assert!(is_symlink(&sandbox.target(".config/nvim/a.txt")));
    assert!(is_symlink(&sandbox.target(".config/nvim/.b.txt")));
}

#[test]
fn dotfiles_directory_with_translated_descendant_is_not_folded() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "config/nvim/dot-init", "init");

    assert_success(&sandbox.winstow(&["--dotfiles", "pkg"]));

    assert!(!is_symlink(&sandbox.target("config")));
    assert!(!is_symlink(&sandbox.target("config/nvim")));
    assert!(is_symlink(&sandbox.target("config/nvim/.init")));
    assert!(!sandbox.target("config/nvim/dot-init").exists());
}

#[test]
fn ignored_descendant_stays_hidden_after_stow_and_refold() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkgA", "config/visible", "a");
    sandbox.package_file("pkgA", "config/dot-secret", "secret");
    sandbox.package_file("pkgB", "config/other", "b");

    assert_success(&sandbox.winstow(&["--dotfiles", "--ignore", "dot-secret", "pkgA"]));
    assert!(!is_symlink(&sandbox.target("config")));
    assert!(!sandbox.target("config/.secret").exists());
    assert!(!sandbox.target("config/dot-secret").exists());

    assert_success(&sandbox.winstow(&["--dotfiles", "pkgB"]));
    assert_success(&sandbox.winstow(&["--dotfiles", "-D", "pkgB"]));
    assert!(!is_symlink(&sandbox.target("config")));
    assert!(is_symlink(&sandbox.target("config/visible")));
    assert!(!sandbox.target("config/dot-secret").exists());
}

#[test]
fn translated_child_prevents_refolding() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkgA", "config/dot-hidden", "a");
    sandbox.package_file("pkgB", "config/other", "b");

    assert_success(&sandbox.winstow(&["--dotfiles", "pkgA"]));
    assert_success(&sandbox.winstow(&["--dotfiles", "pkgB"]));
    assert_success(&sandbox.winstow(&["--dotfiles", "-D", "pkgB"]));

    assert!(!is_symlink(&sandbox.target("config")));
    assert!(is_symlink(&sandbox.target("config/.hidden")));
}

#[test]
fn translated_directory_can_refold_when_children_need_no_translation() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkgA", "dot-config/a.txt", "a");
    sandbox.package_file("pkgB", ".config/b.txt", "b");

    assert_success(&sandbox.winstow(&["--dotfiles", "pkgA"]));
    assert_success(&sandbox.winstow(&["--dotfiles", "pkgB"]));
    assert_success(&sandbox.winstow(&["--dotfiles", "-D", "pkgB"]));

    assert!(is_symlink(&sandbox.target(".config")));
    assert_eq!(
        fs::read_to_string(sandbox.target(".config/a.txt")).unwrap(),
        "a"
    );
    assert!(!sandbox.target(".config/b.txt").exists());
}

#[test]
fn translated_name_collision_reports_conflict_before_override() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", ".bashrc", "first");
    sandbox.package_file("pkg", "dot-bashrc", "second");
    fs::write(sandbox.target(".bashrc"), "keep").unwrap();

    let output = sandbox.winstow(&["--dotfiles", "--override", "pkg"]);

    assert!(!output.status.success());
    assert!(output_text(&output).to_lowercase().contains("conflict"));
    assert_eq!(
        fs::read_to_string(sandbox.target(".bashrc")).unwrap(),
        "keep"
    );
    assert!(!is_symlink(&sandbox.target(".bashrc")));
}

#[test]
fn translated_name_collision_during_unfolding_keeps_original_link() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkgA", "config/one", "a");
    sandbox.package_file("pkgB", "config/.same", "first");
    sandbox.package_file("pkgB", "config/dot-same", "second");

    assert_success(&sandbox.winstow(&["--dotfiles", "pkgA"]));
    let output = sandbox.winstow(&["--dotfiles", "pkgB"]);

    assert!(!output.status.success());
    assert!(output_text(&output).to_lowercase().contains("conflict"));
    assert!(is_symlink(&sandbox.target("config")));
    assert_eq!(
        fs::read_to_string(sandbox.target("config/one")).unwrap(),
        "a"
    );
    assert!(!sandbox.target("config/.same").exists());
}

#[test]
fn restowing_already_folded_package_applies_dotfiles_to_children() {
    let sandbox = Sandbox::new();
    sandbox.package_file("pkg", "config/dot-init", "init");

    assert_success(&sandbox.winstow(&["pkg"]));
    assert!(is_symlink(&sandbox.target("config")));
    assert!(sandbox.target("config/dot-init").exists());

    assert_success(&sandbox.winstow(&["--dotfiles", "pkg"]));
    assert!(!is_symlink(&sandbox.target("config")));
    assert!(is_symlink(&sandbox.target("config/.init")));
    assert!(!sandbox.target("config/dot-init").exists());

    assert_success(&sandbox.winstow(&["--dotfiles", "-D", "pkg"]));
    assert!(!sandbox.target("config").exists());
}
