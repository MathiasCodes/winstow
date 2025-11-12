mod adopt;
mod cli;
mod config;
mod error;
mod fs_ops;
mod ignore;
mod logger;
mod path_utils;
mod planner;
mod stow;
mod unstow;

use clap::Parser;
use cli::{Action, Cli};
use error::Result;

fn main() {
    // Parse command-line arguments
    let cli = Cli::parse();

    // Initialize logger
    logger::init(cli.verbose, cli.dry_run);

    // Validate flags before proceeding
    if let Err(e) = cli.validate_flags() {
        logger::error(&e);
        std::process::exit(2);
    }

    // Run the application and handle errors
    if let Err(e) = run(cli) {
        logger::error(&e.to_string());

        // Exit with code 2 for usage errors, 1 for other errors
        let exit_code = match e {
            error::StowError::InvalidPath(_) => 2,
            _ => 1,
        };
        std::process::exit(exit_code);
    }
}

fn run(cli: Cli) -> Result<()> {
    // Determine which action to perform (before moving cli fields)
    let action = cli.action().map_err(error::StowError::invalid_path)?;

    // Load configuration file
    let file_config = match config::Config::load() {
        Ok(config) => {
            logger::verbose("Loaded configuration file");
            config
        }
        Err(e) => {
            eprintln!("Error loading configuration file: {}", e);
            eprintln!("Cannot continue without valid configuration.");
            return Err(e);
        }
    };

    // Merge config file with CLI arguments to create complete runtime context
    let context = file_config.merge_with_cli(
        cli.stow_dir,
        cli.target_dir,
        cli.ignore,
        cli.defer,
        cli.verbose,
        cli.dry_run,
        cli.adopt,
        cli.override_conflicts,
    )?;

    logger::verbose(&format!("Stow directory: {}", context.stow_dir().display()));
    logger::verbose(&format!(
        "Target directory: {}",
        context.target_dir().display()
    ));

    if context.is_dry_run() {
        logger::info("=== DRY RUN MODE - No changes will be made ===");
    }

    // Build pattern set from context
    let patterns = context.build_pattern_set()?;

    match action {
        Action::Stow => {
            logger::verbose(&format!("Stowing {} package(s)", cli.packages.len()));

            let stower = stow::Stower::from_context(&context, patterns);

            for package in &cli.packages {
                logger::operation("Stow", package);
                stower.stow_package(package)?;
            }

            if !context.is_dry_run() {
                logger::success(&format!("Stowed {} package(s)", cli.packages.len()));
            } else {
                logger::info(&format!("Would stow {} package(s)", cli.packages.len()));
            }
        }

        Action::Delete => {
            logger::verbose(&format!("Unstowing {} package(s)", cli.packages.len()));

            let unstower = unstow::Unstower::from_context(&context);

            for package in &cli.packages {
                logger::operation("Unstow", package);
                unstower.unstow_package(package)?;
            }

            if !context.is_dry_run() {
                logger::success(&format!("Unstowed {} package(s)", cli.packages.len()));
            } else {
                logger::info(&format!("Would unstow {} package(s)", cli.packages.len()));
            }
        }

        Action::Restow => {
            logger::verbose(&format!("Restowing {} package(s)", cli.packages.len()));

            let unstower = unstow::Unstower::from_context(&context);
            let stower = stow::Stower::from_context(&context, patterns);

            for package in &cli.packages {
                logger::operation("Restow", package);
                // Unstow first
                unstower.unstow_package(package)?;
                // Then stow
                stower.stow_package(package)?;
            }

            if !context.is_dry_run() {
                logger::success(&format!("Restowed {} package(s)", cli.packages.len()));
            } else {
                logger::info(&format!("Would restow {} package(s)", cli.packages.len()));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_run_with_stow_action() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a test package
        let package_dir = stow_dir.join("test");
        fs::create_dir(&package_dir).unwrap();
        fs::File::create(package_dir.join("file.txt")).unwrap();

        let cli = Cli {
            stow: false,
            delete: false,
            restow: false,
            stow_dir: Some(stow_dir),
            target_dir: Some(target_dir),
            verbose: false,
            dry_run: true, // Dry run
            adopt: false,
            override_conflicts: false,
            ignore: vec![],
            defer: vec![],
            packages: vec!["test".to_string()],
        };

        // Default action should be stow
        let result = run(cli);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_with_delete_action() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a test package
        let package_dir = stow_dir.join("test");
        fs::create_dir(&package_dir).unwrap();
        fs::File::create(package_dir.join("file.txt")).unwrap();

        let cli = Cli {
            stow: false,
            delete: true,
            restow: false,
            stow_dir: Some(stow_dir),
            target_dir: Some(target_dir),
            verbose: false,
            dry_run: true, // Dry run
            adopt: false,
            override_conflicts: false,
            ignore: vec![],
            defer: vec![],
            packages: vec!["test".to_string()],
        };

        let result = run(cli);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_with_restow_action() {
        let temp_dir = TempDir::new().unwrap();
        let stow_dir = temp_dir.path().join("stow");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&stow_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();

        // Create a test package
        let package_dir = stow_dir.join("test");
        fs::create_dir(&package_dir).unwrap();
        fs::File::create(package_dir.join("file.txt")).unwrap();

        let cli = Cli {
            stow: false,
            delete: false,
            restow: true,
            stow_dir: Some(stow_dir),
            target_dir: Some(target_dir),
            verbose: false,
            dry_run: true, // Dry run
            adopt: false,
            override_conflicts: false,
            ignore: vec![],
            defer: vec![],
            packages: vec!["test".to_string()],
        };

        let result = run(cli);
        assert!(result.is_ok());
    }
}
