//! Source scan: no gate compares a session type with an agent's name (`#agent-tools 12/12`).
//!
//! Feature doc: `docs/ft/daemon/session-agent-roster.md`. What a session of an agent
//! offers is declared in its spec's `capabilities` and read through
//! `tddy_coding_agent_specs::CodingAgentSpec::has`; the agent a session type runs is
//! `CodingAgentRegistry::for_session_type`. So no production code in the crates that held the
//! gates has a reason to compare a session type with `"claude-cli"` or any other agent's session
//! type. A deletion claim is only durable if it is pinned, so this test reads their sources.
//!
//! A **gate** is a non-comment line that names an agent session type as a string literal
//! (`"claude-cli"`, `"cursor-cli"`, `"opencode-cli"`, `"muse-cli"`, `"warp-cli"`) together with a
//! comparison: `==`, `!=`, `matches!`, a match arm's `=>`, or an or-pattern's `|`. A file is read
//! up to its first `#[cfg(test)]`, which is where this workspace keeps its inline test modules: a
//! test may build a `claude-cli` session, but production code may not branch on one.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;

/// The session types the built-in agent specs stand for, as string literals.
const AGENT_SESSION_TYPE_LITERALS: &[&str] = &[
    "\"claude-cli\"",
    "\"cursor-cli\"",
    "\"opencode-cli\"",
    "\"muse-cli\"",
    "\"warp-cli\"",
];

/// What makes a line that names a session type a branch on it.
const COMPARISONS: &[&str] = &["==", "!=", "matches!", "=>", "\" |", "| \""];

/// One production line that branches on an agent's session type.
#[derive(Debug, PartialEq, Eq)]
struct AgentNameGate {
    file: PathBuf,
    line: usize,
    text: String,
}

/// The crates that held the gates (changeset § State A), relative to the workspace's `packages/`.
fn the_gate_crates() -> Vec<PathBuf> {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tddy-session-lifecycle lives under packages/")
        .to_path_buf();
    [
        "tddy-agent-backend",
        "tddy-agent-launch",
        "tddy-coder",
        "tddy-daemon-rpc",
        "tddy-session-activity",
        "tddy-session-lifecycle",
        "tddy-worktree-service",
    ]
    .iter()
    .map(|name| packages.join(name).join("src"))
    .collect()
}

/// Every `.rs` file under `dir`, recursively.
fn rust_sources_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
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
        .collect()
}

/// Every `.rs` file under `roots`, sorted so a failure lists them stably.
fn rust_sources_under(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = roots
        .iter()
        .flat_map(|root| rust_sources_in(root))
        .collect();
    files.sort();
    files
}

/// Every gate in `text`, attributed to `file`, read up to its first `#[cfg(test)]`.
fn agent_name_gates_in(file: &Path, text: &str) -> Vec<AgentNameGate> {
    text.lines()
        .take_while(|line| !line.trim_start().starts_with("#[cfg(test)]"))
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| {
            AGENT_SESSION_TYPE_LITERALS
                .iter()
                .any(|literal| line.contains(literal))
        })
        .filter(|(_, line)| {
            COMPARISONS
                .iter()
                .any(|comparison| line.contains(comparison))
        })
        .map(|(index, line)| AgentNameGate {
            file: file.to_path_buf(),
            line: index + 1,
            text: line.trim().to_string(),
        })
        .collect()
}

#[test]
fn the_scan_finds_a_gate_in_each_shape_and_skips_comments_values_and_tests() {
    // Given — the shapes the gates were written in, beside lines that are not gates
    let file = Path::new("gate.rs");
    let text = concat!(
        "if session_type != \"claude-cli\" {\n",
        "    matches!(session_type, \"cursor-cli\" | \"muse-cli\")\n",
        "// a comment naming \"claude-cli\" == nothing\n",
        "let session_type = \"opencode-cli\".to_string();\n",
        "#[cfg(test)]\n",
        "assert!(session_type == \"warp-cli\");\n",
    );

    // When
    let gates = agent_name_gates_in(file, text);

    // Then
    assert_eq!(
        gates,
        vec![
            AgentNameGate {
                file: file.to_path_buf(),
                line: 1,
                text: "if session_type != \"claude-cli\" {".to_string(),
            },
            AgentNameGate {
                file: file.to_path_buf(),
                line: 2,
                text: "matches!(session_type, \"cursor-cli\" | \"muse-cli\")".to_string(),
            },
        ]
    );
}

#[test]
fn no_gate_in_the_session_crates_compares_a_session_type_with_an_agents_name() {
    // Given
    let sources = rust_sources_under(&the_gate_crates());
    let covers = |suffix: &str| sources.iter().any(|file| file.ends_with(suffix));
    assert!(
        covers("tddy-session-activity/src/service/hook_reports.rs")
            && covers("tddy-agent-launch/src/svc_start_session_core.rs")
            && covers("tddy-worktree-service/src/branch_intent.rs"),
        "the scan must cover every gate crate, but it read {sources:#?}"
    );

    // When
    let gates: Vec<AgentNameGate> = sources
        .iter()
        .flat_map(|file| {
            let text = std::fs::read_to_string(file).expect("a readable source file");
            agent_name_gates_in(file, &text)
        })
        .collect();

    // Then
    assert_eq!(
        gates,
        vec![],
        "a gate must read the agent's capabilities (`CodingAgentSpec::has`) or the registry \
         (`CodingAgentRegistry::for_session_type`), never its name"
    );
}

// ── No agent id in tddy-agent-launch (`#agent-tools 17/19`) ───────────────────────────────────
//
// Every CLI agent starts, resumes and re-attaches through one spec-driven launch, so the launch
// crate has no reason left to spell an agent: not to compare a session type or an agent id, not to
// dispatch on one, and not to record one as a default. This scan is stricter than the gate scan
// above — **any** production line of `tddy-agent-launch` naming an agent id or its session type is
// a finding, whether or not it compares — because a launch that writes `"claude-cli"` into a record
// or a route is the same hard-coded agent as one that matches on it.

/// What names an agent in `tddy-agent-launch`: the built-in agents' ids and session types as
/// string literals, and the constants that held them.
const AGENT_ID_TOKENS: &[&str] = &[
    "\"claude\"",
    "\"cursor\"",
    "\"claude-cli\"",
    "\"cursor-cli\"",
    "CLAUDE_AGENT_ID",
    "CURSOR_AGENT_ID",
];

/// The `tddy-agent-launch` sources the scan may still find an agent id in. Empty: Telegram now
/// starts through the one launch (`#agent-tools 18/19`), so nothing is kept for it.
const STILL_NEEDED_BY_TELEGRAM: &[&str] = &[];

/// One production line of `tddy-agent-launch` that names an agent.
#[derive(Debug, PartialEq, Eq)]
struct AgentIdLiteral {
    file: PathBuf,
    line: usize,
    text: String,
}

/// Whether `file` is a test module of its own (`…_tests.rs`, `…_test.rs`), which may build a
/// session of any agent.
fn is_a_test_module(file: &Path) -> bool {
    file.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.ends_with("tests") || stem.ends_with("_test"))
}

/// Every line of `text` naming an agent, attributed to `file`, read up to its first `#[cfg(test)]`
/// and skipping comments.
fn agent_id_literals_in(file: &Path, text: &str) -> Vec<AgentIdLiteral> {
    text.lines()
        .take_while(|line| !line.trim_start().starts_with("#[cfg(test)]"))
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| AGENT_ID_TOKENS.iter().any(|token| line.contains(token)))
        .map(|(index, line)| AgentIdLiteral {
            file: file.to_path_buf(),
            line: index + 1,
            text: line.trim().to_string(),
        })
        .collect()
}

/// `tddy-agent-launch/src`.
fn the_launch_crate() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tddy-session-lifecycle lives under packages/")
        .join("tddy-agent-launch")
        .join("src")
}

#[test]
fn the_agent_id_scan_finds_a_literal_or_a_constant_in_any_shape_and_skips_comments_and_tests() {
    // Given — every way an agent was named, beside lines that do not name one
    let file = Path::new("launch.rs");
    let text = concat!(
        "    session_type: \"cursor-cli\",\n",
        "        .resolve(CLAUDE_AGENT_ID, None)?;\n",
        "        agent: \"claude\".to_string(),\n",
        "// a comment naming \"claude-cli\"\n",
        "    let id = spec.id.as_str();\n",
        "#[cfg(test)]\n",
        "    session_type: \"claude-cli\",\n",
    );

    // When
    let literals = agent_id_literals_in(file, text);

    // Then
    assert_eq!(
        literals,
        vec![
            AgentIdLiteral {
                file: file.to_path_buf(),
                line: 1,
                text: "session_type: \"cursor-cli\",".to_string(),
            },
            AgentIdLiteral {
                file: file.to_path_buf(),
                line: 2,
                text: ".resolve(CLAUDE_AGENT_ID, None)?;".to_string(),
            },
            AgentIdLiteral {
                file: file.to_path_buf(),
                line: 3,
                text: "agent: \"claude\".to_string(),".to_string(),
            },
        ]
    );
}

#[test]
fn no_agent_id_literal_is_compared_or_dispatched_on_in_agent_launch() {
    // Given — the launch crate's production sources
    let sources: Vec<PathBuf> = rust_sources_under(&[the_launch_crate()])
        .into_iter()
        .filter(|file| !is_a_test_module(file))
        .collect();
    assert!(
        sources
            .iter()
            .any(|file| file.ends_with("tddy-agent-launch/src/svc_start_session_core.rs")),
        "the scan must read tddy-agent-launch, but it read {sources:#?}"
    );

    // When
    let literals: Vec<AgentIdLiteral> = sources
        .iter()
        .flat_map(|file| {
            let text = std::fs::read_to_string(file).expect("a readable source file");
            agent_id_literals_in(file, &text)
        })
        .filter(|literal| {
            !STILL_NEEDED_BY_TELEGRAM
                .iter()
                .any(|kept| literal.file.ends_with(kept))
        })
        .collect();

    // Then
    assert_eq!(
        literals,
        vec![],
        "a CLI launch reads its agent from the installed coding agents (`cli_launcher`, \
         `CliSessionType`) and its behaviour from the spec (`CodingAgentSpec::has`), never from an \
         agent's id"
    );
}
