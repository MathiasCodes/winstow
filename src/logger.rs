use std::sync::atomic::{AtomicBool, Ordering};

/// Global logging state using atomics for lock-free reads
static VERBOSE: AtomicBool = AtomicBool::new(false);
static DRY_RUN: AtomicBool = AtomicBool::new(false);

/// Initialize the logger with configuration
pub fn init(verbose: bool, dry_run: bool) {
    VERBOSE.store(verbose, Ordering::Relaxed);
    DRY_RUN.store(dry_run, Ordering::Relaxed);
}

/// Log a verbose message (only shown when verbose mode is enabled)
pub fn verbose(message: &str) {
    if VERBOSE.load(Ordering::Relaxed) {
        println!("{}", message);
    }
}

/// Log an info message (always shown)
pub fn info(message: &str) {
    println!("{}", message);
}

/// Log a warning message to stderr
pub fn warn(message: &str) {
    eprintln!("Warning: {}", message);
}

/// Log an error message to stderr
pub fn error(message: &str) {
    eprintln!("Error: {}", message);
}

/// Log an action that will be or has been performed
/// In dry-run mode, prefixes with "[DRY RUN]"
pub fn action(message: &str) {
    let dry_run = DRY_RUN.load(Ordering::Relaxed);
    let verbose = VERBOSE.load(Ordering::Relaxed);

    if dry_run {
        println!("[DRY RUN] {}", message);
    } else if verbose {
        println!("{}", message);
    }
}

/// Log the start of an operation
pub fn operation(operation: &str, target: &str) {
    let verbose = VERBOSE.load(Ordering::Relaxed);
    let dry_run = DRY_RUN.load(Ordering::Relaxed);

    if verbose || dry_run {
        let prefix = if dry_run { "[DRY RUN] " } else { "" };
        println!("{}{}: {}", prefix, operation, target);
    }
}

/// Log a success message
pub fn success(message: &str) {
    println!("✓ {}", message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger_initialization() {
        init(true, false);
        // Test that logger can be initialized without panicking
        assert!(VERBOSE.load(Ordering::Relaxed));
        assert!(!DRY_RUN.load(Ordering::Relaxed));
    }

    #[test]
    fn test_logger_functions() {
        // Test that all logging functions can be called without panicking
        init(true, false);
        verbose("Test verbose");
        info("Test info");
        warn("Test warning");
        error("Test error");
        action("Test action");
        operation("Test", "target");
        success("Test success");
    }

    #[test]
    fn test_dry_run_mode() {
        init(false, true);
        assert!(!VERBOSE.load(Ordering::Relaxed));
        assert!(DRY_RUN.load(Ordering::Relaxed));
    }
}
