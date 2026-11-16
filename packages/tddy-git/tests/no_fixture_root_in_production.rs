//! No production source of this crate names a path a test host happened to have
//! (`#remote-shell` 10/11).
//!
//! `/home/dev/<repo>` was the default remote checkout root for every SSH host and every target for
//! five months, because a fixture's default was the only default. A test module may still spell
//! the path; nothing outside one may.

use std::fs;
use std::path::Path;

const FIXTURE_PATH: &str = "/home/dev";

/// The lines of `source` before its first `#[cfg(test)]` — the crate's convention keeps tests at
/// the tail of a file.
fn production_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source
        .lines()
        .enumerate()
        .take_while(|(_, line)| !line.contains("#[cfg(test)]"))
}

#[test]
fn no_production_source_names_home_dev() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for entry in fs::read_dir(&src).expect("src is listed") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("source is read");
        for (index, line) in production_lines(&source) {
            if line.contains(FIXTURE_PATH) {
                offenders.push(format!("{}:{}: {}", path.display(), index + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "production code names {FIXTURE_PATH}:\n{}",
        offenders.join("\n")
    );
}
