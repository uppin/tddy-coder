//! Source scan: `tddy-coding-subagent` names no coding agent in its production code
//! (`#agent-tools 27/28`, `docs/dev/changesets/2026-11-09-agent-tools-acp-subagent-leftovers.md`).
//!
//! Which agent withdraws which native tools, how it is spoken to and which ones are ACP agents is
//! the agents' specs' to declare (`coding_agent.acp.tool_deny`, `acp.withdraw_native`, the `acp` and
//! `headless` sections), read through `tddy_coding_agent_specs::CodingAgentSpec`. So this crate has
//! no reason left to spell an agent: not to compare a harness with one, not to list them, not to
//! keep a per-agent table. A deletion claim is only durable if it is pinned, so this test reads the
//! crate's sources.
//!
//! A **finding** is a non-comment line, read up to the file's first `#[cfg(test)]` (where this
//! workspace keeps its inline test modules), that names a built-in agent's id as a string literal
//! or one of the constants and tables that held the per-agent data. A test may build a session of
//! any agent; production code may not name one.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;

/// The built-in coding agents' ids, as string literals.
const AGENT_ID_LITERALS: &[&str] = &[
    "\"claude-acp\"",
    "\"codex-acp\"",
    "\"opencode\"",
    "\"claude\"",
    "\"cursor\"",
    "\"codex\"",
    "\"oz\"",
];

/// The constants and tables that held per-agent data, matched as whole identifiers (so
/// `OPENCODE_CONFIG_CONTENT` is not `OPENCODE`). `OPENCODE_CONFIG_CONTENT` itself stays: the route
/// is still handed inline until the route-file node lands (user, 2026-11-07).
const AGENT_TABLES: &[&str] = &[
    "CLAUDE_ACP",
    "CODEX_ACP",
    "OPENCODE",
    "ACP_HARNESSES",
    "HEADLESS_HARNESSES",
    "CLAUDE_NATIVE_TOOLS",
    "OPENCODE_NATIVE_TOOLS",
];

/// One production line that names a coding agent.
#[derive(Debug, PartialEq, Eq)]
struct AgentName {
    file: PathBuf,
    line: usize,
    text: String,
}

/// Whether `line` names `identifier` as a whole identifier.
fn names_identifier(line: &str, identifier: &str) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    line.match_indices(identifier).any(|(at, _)| {
        let before = line[..at].chars().next_back();
        let after = line[at + identifier.len()..].chars().next();
        !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
}

/// Every line of `text` naming an agent, attributed to `file`, read up to its first `#[cfg(test)]`
/// and skipping comments.
fn agent_names_in(file: &Path, text: &str) -> Vec<AgentName> {
    text.lines()
        .take_while(|line| !line.trim_start().starts_with("#[cfg(test)]"))
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| {
            AGENT_ID_LITERALS
                .iter()
                .any(|literal| line.contains(literal))
                || AGENT_TABLES
                    .iter()
                    .any(|table| names_identifier(line, table))
        })
        .map(|(index, line)| AgentName {
            file: file.to_path_buf(),
            line: index + 1,
            text: line.trim().to_string(),
        })
        .collect()
}

/// Every `.rs` file under `dir`, recursively, sorted so a failure lists them stably.
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

/// `tddy-coding-subagent/src`.
fn the_crate_sources() -> Vec<PathBuf> {
    rust_sources_in(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
}

#[test]
fn the_agent_name_scan_finds_a_literal_or_a_table_in_any_shape_and_skips_comments_and_tests() {
    // Given — every way an agent was named, beside lines that do not name one
    let file = Path::new("confinement.rs");
    let text = concat!(
        "        CLAUDE_ACP | OPENCODE => Confinement::WithdrawsNativeTools,\n",
        "pub const ACP_HARNESSES: &[&str] = &[CLAUDE_ACP, CODEX_ACP, OPENCODE];\n",
        "        \"cursor\" => Some(PermissionPolicy::Fixed(\n",
        "pub const CLAUDE_NATIVE_TOOLS: &[&str] = &[\n",
        "// a comment naming \"claude-acp\" and CLAUDE_NATIVE_TOOLS\n",
        "    let id = spec.id.as_str();\n",
        "    let env = OPENCODE_CONFIG_ENV;\n",
        "#[cfg(test)]\n",
        "    let harness = \"codex-acp\";\n",
    );

    // When
    let names = agent_names_in(file, text);

    // Then
    assert_eq!(
        names.iter().map(|name| name.line).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
}

#[test]
fn coding_subagent_names_no_agent() {
    // Given — the crate's production sources
    let sources = the_crate_sources();
    assert!(
        sources.iter().any(|file| file.ends_with("src/launcher.rs"))
            && sources
                .iter()
                .any(|file| file.ends_with("src/confinement.rs"))
            && sources
                .iter()
                .any(|file| file.ends_with("src/transports.rs")),
        "the scan must read the crate's sources, but it read {sources:#?}"
    );

    // When
    let names: Vec<AgentName> = sources
        .iter()
        .flat_map(|file| {
            let text = std::fs::read_to_string(file).expect("a readable source file");
            agent_names_in(file, &text)
        })
        .collect();

    // Then
    assert_eq!(
        names,
        Vec::<AgentName>::new(),
        "a coding subagent reads its agent's withdrawal, transport and confinement from the \
         agent's spec (`CodingAgentSpec::acp`, `acp_tool_deny`, `headless`), never from its name"
    );
}
