//! Source scan: the Telegram control plane names no agent (`#agent-tools 18/19`).
//!
//! Telegram starts every CLI agent through the daemon's generic start and reads what to offer from
//! the host's coding-agent registry, so its production code has no reason to read a daemon.yaml
//! agent section (`claude_cli`, `cursor_cli`), keep a per-agent model list, or compare a session
//! type with an agent's name. The same rule `tddy-session-lifecycle/tests/no_agent_name_gates.rs`
//! holds for the session crates, applied to this one. A file is read up to its first
//! `#[cfg(test)]`, where this workspace keeps its inline test modules.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;

/// What reads an agent's daemon.yaml section or its hand-kept model list.
const AGENT_CONFIG_READS: &[&str] = &[
    ".claude_cli",
    ".cursor_cli",
    "resolve_cursor_binary_path",
    "CLAUDE_CLI_MODELS",
    "CURSOR_CLI_MODELS",
    "claude_cli_models",
    "cursor_cli_models",
];

/// The session types the built-in agents stand for, as string literals.
const AGENT_SESSION_TYPE_LITERALS: &[&str] = &["\"claude-cli\"", "\"cursor-cli\""];

/// What makes a line naming a session type a branch on it.
const COMPARISONS: &[&str] = &["==", "!=", "matches!", "=>", "Some(\""];

/// One production line of this crate that names an agent.
#[derive(Debug, PartialEq, Eq)]
struct AgentNaming {
    file: PathBuf,
    line: usize,
    text: String,
}

fn rust_sources_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} is readable: {e}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .flat_map(|path| {
            if path.is_dir() {
                rust_sources_in(&path)
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                vec![path]
            } else {
                Vec::new()
            }
        })
        .collect();
    files.sort();
    files
}

/// Every line of `text` before its first `#[cfg(test)]` that is not a comment and names an agent.
fn agent_namings_in(file: &Path, text: &str) -> Vec<AgentNaming> {
    text.lines()
        .take_while(|line| !line.trim_start().starts_with("#[cfg(test)]"))
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| {
            AGENT_CONFIG_READS.iter().any(|read| line.contains(read))
                || (AGENT_SESSION_TYPE_LITERALS
                    .iter()
                    .any(|literal| line.contains(literal))
                    && COMPARISONS
                        .iter()
                        .any(|comparison| line.contains(comparison)))
        })
        .map(|(index, line)| AgentNaming {
            file: file.to_path_buf(),
            line: index + 1,
            text: line.trim().to_string(),
        })
        .collect()
}

#[test]
fn the_scan_finds_a_config_read_a_model_list_and_a_gate_and_skips_comments_and_tests() {
    // Given
    let file = Path::new("start.rs");
    let text = concat!(
        "let binary = deps.config.claude_cli.as_ref();\n",
        "for model in CURSOR_CLI_MODELS.iter() {}\n",
        "if snap.session_type.as_deref() == Some(\"claude-cli\") {}\n",
        "// config.cursor_cli was read here once\n",
        "let session_type = spec_session_type.to_string();\n",
        "#[cfg(test)]\n",
        "let c = config.claude_cli;\n",
    );

    // When
    let found = agent_namings_in(file, text);

    // Then
    assert_eq!(
        found.iter().map(|naming| naming.line).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn no_production_line_of_the_telegram_control_plane_names_an_agent() {
    // Given
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    // When
    let found: Vec<AgentNaming> = rust_sources_in(&src)
        .iter()
        .flat_map(|file| {
            let text = std::fs::read_to_string(file)
                .unwrap_or_else(|e| panic!("{} is readable: {e}", file.display()));
            agent_namings_in(file, &text)
        })
        .collect();

    // Then
    assert_eq!(found, Vec::<AgentNaming>::new());
}
