//! Every production file of the engine fits the repository's 500-line budget (`#reshape` 13/19), the
//! Rust backend's root included (`#reshape` 17/19): no file is exempt.
//!
//! Measured by the one count the budget is held to — [`production_lines_of_file`], which leaves out
//! each `#[cfg(test)]` item wherever it sits — so a file cannot slip under the budget by declaring
//! an extracted test module near its top.

use std::path::{Path, PathBuf};

use tddy_code_restructuring::production_lines_of_file;

/// The budget, in production lines. A file *at* it is within it.
const BUDGET: usize = 500;

fn package_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_files_under(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("the source directory is readable") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            rust_files_under(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

/// Every production file under `src/` with its production lines, as `(relative path, lines)`.
fn the_engines_files_measured() -> Vec<(String, usize)> {
    let root = package_root();
    let mut files = Vec::new();
    rust_files_under(&root.join("src"), &mut files);
    files
        .iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .expect("under the package")
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(path).expect("a readable source file");
            let lines = production_lines_of_file(&relative, &text);
            (relative, lines)
        })
        .collect()
}

#[test]
fn every_production_file_of_the_engine_is_within_500_lines() {
    // Given every production file of the engine, measured
    let measured = the_engines_files_measured();

    // When the ones over the budget are listed
    let over: Vec<&(String, usize)> = measured
        .iter()
        .filter(|(_, lines)| *lines > BUDGET)
        .collect();

    // Then there are none
    assert!(over.is_empty(), "over {BUDGET} production lines: {over:?}");
}
