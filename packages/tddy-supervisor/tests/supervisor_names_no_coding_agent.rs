//! Guard: the supervisor names no coding agent (`#agent-tools` 32/32).
//!
//! It runs the daemon, and what the daemon asks for, as the right OS user, in cgroups, on the right
//! sockets. Which coding agents exist, what they are called and what they need is the daemon's YAML.
//! This scan keeps it that way: no agent name in `tddy-supervisor`'s non-test code, its proto, or
//! the two supervisor templates.
//!
//! The names come from the coding-agent spec files themselves, so an agent added there is guarded
//! here without touching this test. A Rust file is scanned up to its first `#[cfg(test)]` line —
//! where this crate keeps its test modules — and `src/test_util.rs` (test-only) not at all.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_root() -> PathBuf {
    crate_dir().join("..").join("..")
}

/// Every word an agent's spec file name is made of (`claude-acp` → `claude`, `acp`).
fn coding_agent_names() -> BTreeSet<String> {
    let specs = repo_root().join("packages/tddy-coding-agent-specs/specs");
    let names: BTreeSet<String> = std::fs::read_dir(&specs)
        .unwrap_or_else(|error| panic!("read {}: {error}", specs.display()))
        .map(|entry| entry.expect("a spec directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
        .filter_map(|path| Some(path.file_stem()?.to_str()?.to_string()))
        // The shared env base every agent starts from, not an agent.
        .filter(|stem| stem != "agent-env")
        .flat_map(|stem| stem.split('-').map(str::to_string).collect::<Vec<_>>())
        .collect();
    assert!(
        names.contains("claude") && names.contains("opencode"),
        "the spec directory names the agents this guard is about: {names:?}"
    );
    names
}

fn rust_sources_under(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("read a source directory") {
        let path = entry.expect("a source directory entry").path();
        if path.is_dir() {
            rust_sources_under(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && path.file_name().is_some_and(|name| name != "test_util.rs")
        {
            found.push(path);
        }
    }
}

/// What of `path` ships: a Rust file up to its first `#[cfg(test)]`, anything else whole.
fn shipped_text_of(path: &Path) -> String {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    if path.extension().is_some_and(|extension| extension == "rs") {
        text.lines()
            .take_while(|line| line.trim() != "#[cfg(test)]")
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        text
    }
}

/// `file:line: word` for every line of `path` holding a word in `names`.
fn agent_names_in(path: &Path, names: &BTreeSet<String>) -> Vec<String> {
    shipped_text_of(path)
        .lines()
        .enumerate()
        .flat_map(|(index, line)| {
            line.to_lowercase()
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|word| names.contains(*word))
                .map(|word| format!("{}:{}: {word}", path.display(), index + 1))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn the_supervisor_names_no_coding_agent() {
    // Given the agents the daemon's YAML knows, and everything the supervisor ships
    let names = coding_agent_names();
    let mut shipped = Vec::new();
    rust_sources_under(&crate_dir().join("src"), &mut shipped);
    shipped.push(crate_dir().join("proto/supervisor.proto"));
    shipped.push(repo_root().join("supervisor.yaml.production"));
    shipped.push(repo_root().join("dev.supervisor.yaml"));

    // When
    let mentions: Vec<String> = shipped
        .iter()
        .flat_map(|path| agent_names_in(path, &names))
        .collect();

    // Then
    assert!(
        mentions.is_empty(),
        "the supervisor names coding agents — that is the daemon's YAML:\n{}",
        mentions.join("\n")
    );
}
