//! Source scan: the jail crates name no coding agent (`#agent-tools 10/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-tool-agnostic-jail.md`. Every jail is a tool plan: what
//! an agent needs of one is declared in its tool file (`tddy-coding-agent-specs/specs/*.yaml`) and
//! lowered by `tddy_sandbox_recipes::tool_grants`, so neither `tddy-sandbox` nor
//! `tddy-sandbox-recipes` has any reason to spell an agent's name. A deletion claim is only durable
//! if it is pinned, so this test reads their sources — comments included, since a comment naming an
//! agent is where the next agent arm gets added.
//!
//! The terms are the agents the registry ships. `oz` is left out: two letters match too much
//! unrelated text to be a useful guard.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;

/// The coding agents the registry ships, as they would appear in source (matched case-blind).
const AGENT_NAMES: &[&str] = &["claude", "cursor", "opencode", "muse", "codex", "warp"];

/// One source line that names an agent.
#[derive(Debug, PartialEq, Eq)]
struct AgentMention {
    file: PathBuf,
    line: usize,
    agent: &'static str,
}

/// The crate directories under scan, relative to the workspace's `packages/`.
fn the_jail_crates() -> Vec<PathBuf> {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tddy-sandbox lives under packages/")
        .to_path_buf();
    vec![
        packages.join("tddy-sandbox").join("src"),
        packages.join("tddy-sandbox-recipes").join("src"),
    ]
}

/// Every `.rs` file under `roots`, sorted so a failure lists them stably.
fn rust_sources_under(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = roots
        .iter()
        .flat_map(|root| walkdir::WalkDir::new(root).into_iter())
        .map(|entry| entry.expect("a readable source tree").into_path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    files.sort();
    files
}

/// Every line of `text` that names one of [`AGENT_NAMES`], attributed to `file`.
fn agent_mentions_in(file: &Path, text: &str) -> Vec<AgentMention> {
    text.lines()
        .enumerate()
        .flat_map(|(index, line)| {
            let lowered = line.to_lowercase();
            AGENT_NAMES
                .iter()
                .filter(move |agent| lowered.contains(*agent))
                .map(move |agent| AgentMention {
                    file: file.to_path_buf(),
                    line: index + 1,
                    agent,
                })
        })
        .collect()
}

#[test]
fn the_scan_finds_an_agent_named_in_any_casing() {
    // Given — a line in the shape the recipes spelled their arms
    let file = Path::new("plan.rs");
    let text = "fn policy() {}\n    SandboxRecipe::ClaudeCli => muse_interactive_policy(),\n";

    // When
    let mentions = agent_mentions_in(file, text);

    // Then
    assert_eq!(
        mentions,
        vec![
            AgentMention {
                file: file.to_path_buf(),
                line: 2,
                agent: "claude",
            },
            AgentMention {
                file: file.to_path_buf(),
                line: 2,
                agent: "muse",
            },
        ]
    );
}

#[test]
fn tddy_sandbox_and_tddy_sandbox_recipes_name_no_coding_agent() {
    // Given
    let sources = rust_sources_under(&the_jail_crates());
    let builder = sources
        .iter()
        .any(|f| f.ends_with("tddy-sandbox/src/builder.rs"));
    let tool_grants = sources
        .iter()
        .any(|f| f.ends_with("tddy-sandbox-recipes/src/tool_grants.rs"));
    assert!(
        builder && tool_grants,
        "the scan must cover both crates, but it read {sources:#?}"
    );

    // When
    let mentions: Vec<AgentMention> = sources
        .iter()
        .flat_map(|file| {
            let text = std::fs::read_to_string(file).expect("a readable source file");
            agent_mentions_in(file, &text)
        })
        .collect();

    // Then
    assert_eq!(
        mentions,
        vec![],
        "the jail crates must name no coding agent — move what they say about one into its tool file"
    );
}
