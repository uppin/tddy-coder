//! Source scan: the usage engine names no coding agent (`#usage-binding 1/5`).
//!
//! An agent's usage is declared in its tool file (`tddy-coding-agent-specs/specs/*.yaml`) and
//! evaluated here as data, so `tddy-usage` has no reason to spell an agent's name. The scan reads
//! comments too: a comment naming an agent is where the next agent arm gets added. `oz` is left out
//! — two letters match too much unrelated text to be a useful guard.

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

/// Every `.rs` file under `dir`, recursively, sorted so a failure lists them stably.
fn rust_sources_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("a readable source tree") {
            let path = entry.expect("a readable entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// `line` with every `TODO(<node>)` tag removed: the tag names the stack node that owes the work
/// (a branch name, which may well contain an agent's), not anything the code says about an agent.
fn without_node_tags(line: &str) -> String {
    let mut rest = line;
    let mut kept = String::new();
    while let Some(start) = rest.find("TODO(") {
        kept.push_str(&rest[..start]);
        rest = match rest[start..].find(')') {
            Some(end) => &rest[start + end + 1..],
            None => "",
        };
    }
    kept.push_str(rest);
    kept
}

/// Every line of `text` that names one of [`AGENT_NAMES`], attributed to `file`.
fn agent_mentions_in(file: &Path, text: &str) -> Vec<AgentMention> {
    text.lines()
        .enumerate()
        .flat_map(|(index, line)| {
            let lowered = without_node_tags(line).to_lowercase();
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
fn a_node_tag_is_not_an_agent_mention_but_the_text_beside_it_is() {
    // Given
    let file = Path::new("gather.rs");
    let text = "// TODO(claude-transcript): implement\n// read the codex rollout\n";

    // When
    let mentions = agent_mentions_in(file, text);

    // Then
    assert_eq!(
        mentions,
        vec![AgentMention {
            file: file.to_path_buf(),
            line: 2,
            agent: "codex",
        }]
    );
}

#[test]
fn the_usage_engine_names_no_agent() {
    // Given
    let sources = rust_sources_under(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    assert!(
        sources.iter().any(|f| f.ends_with("src/lib.rs")),
        "the scan must cover the crate, but it read {sources:#?}"
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
        "tddy-usage must name no coding agent — what it says about one belongs in that agent's tool file"
    );
}
