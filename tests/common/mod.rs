// Helpers shared by the integration tests that run the winstow binary
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

/// Temporary stow and target directories for one test
pub struct Sandbox {
    pub root: TempDir,
}

impl Sandbox {
    pub fn new() -> Self {
        let root = TempDir::new().unwrap();
        fs::create_dir(root.path().join("stow")).unwrap();
        fs::create_dir(root.path().join("target")).unwrap();
        Self { root }
    }

    pub fn stow_dir(&self) -> PathBuf {
        self.root.path().join("stow")
    }

    pub fn target(&self, relative: &str) -> PathBuf {
        self.root.path().join("target").join(relative)
    }

    /// Create a file inside a package
    pub fn package_file(&self, package: &str, relative: &str, content: &str) -> PathBuf {
        let path = self.stow_dir().join(package).join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        path
    }

    /// Run winstow with the sandbox's stow and target directories
    pub fn winstow(&self, args: &[&str]) -> Output {
        let binary = std::env::var_os("WINSTOW_TEST_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_winstow")));

        Command::new(binary)
            .current_dir(self.root.path())
            // Keep configuration files of the machine out of the test
            .env("USERPROFILE", self.root.path())
            .env("APPDATA", self.root.path())
            .arg("-d")
            .arg(self.stow_dir())
            .arg("-t")
            .arg(self.root.path().join("target"))
            .args(args)
            .output()
            .unwrap()
    }
}

pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "winstow failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn is_symlink(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
}

pub fn exists_at_all(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// Combined stdout and stderr of a winstow run
pub fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Create a directory outside the stow and target directories with one file in it
pub fn foreign_dir(sandbox: &Sandbox) -> PathBuf {
    let foreign = sandbox.root.path().join("foreign");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join("keep.txt"), "foreign").unwrap();
    foreign
}
