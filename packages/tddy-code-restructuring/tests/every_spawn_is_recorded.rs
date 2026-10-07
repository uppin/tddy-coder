//! No production code starts a process except through the recorder.
//!
//! A record that most spawn sites write to is a record that cannot be trusted to be complete: the
//! next `Command::new` somebody adds would be a process nobody can see. The same technique as
//! `library_returns_its_results` keeps it so — read the sources, name the offenders.

use std::path::{Path, PathBuf};

/// The module that owns starting a process, and the children it keeps its parts in.
fn is_the_recorder(relative: &Path) -> bool {
    relative == Path::new("spawn_record.rs") || relative.starts_with("spawn_record")
}

/// The lines of `source` that are production code: everything above its test module.
///
/// The test module is a `#[cfg(test)]` that is followed by `mod`. A `#[cfg(test)]` over a `use` or a
/// function is one gated item in the middle of production code, and stopping there would hide every
/// line below it.
fn production_lines(source: &str) -> Vec<(usize, &str)> {
    let lines: Vec<&str> = source.lines().collect();
    let test_module = lines.iter().enumerate().position(|(index, line)| {
        line.trim() == "#[cfg(test)]"
            && lines
                .get(index + 1)
                .is_some_and(|next| next.trim_start().starts_with("mod "))
    });
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| (index + 1, line))
        .take(test_module.unwrap_or(usize::MAX))
        .collect()
}

fn starts_a_process(line: &str) -> bool {
    let code = line.trim();
    !code.starts_with("//") && (code.contains("Command::new") || code.contains(".spawn()"))
}

#[test]
fn no_production_code_starts_a_process_except_through_the_recorder() {
    // Given this crate's own sources
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    // When every production line outside the recorder is read
    let mut offenders: Vec<String> = Vec::new();
    for module in modules_under(&src) {
        let relative = module.strip_prefix(&src).expect("a path under src");
        if is_the_recorder(relative) {
            continue;
        }
        let source = std::fs::read_to_string(&module).expect("a source file");
        offenders.extend(
            production_lines(&source)
                .into_iter()
                .filter(|(_, line)| starts_a_process(line))
                .map(|(number, line)| format!("{}:{number}: {}", relative.display(), line.trim())),
        );
    }

    // Then none of them starts a process
    assert_eq!(
        offenders,
        Vec::<String>::new(),
        "these production sites start a process without telling the recorder"
    );
}

/// Every `.rs` file under `dir`, sorted, so the assertion above is deterministic.
fn modules_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).expect("a readable directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
