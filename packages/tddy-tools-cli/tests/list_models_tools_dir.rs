//! `tddy-tools list-models --tools-dir <dir>` lists from the host's coding-agent registry: the
//! built-in specs with that directory's overrides applied, so an operator's model list is the one
//! the daemon's model probe reports.
//!
//! Product: `docs/ft/daemon/1-WIP/PRD-2026-11-06-one-host-coding-agent-registry.md`.

use std::path::Path;

use assert_cmd::Command;
use serde_json::{json, Value};
use tddy_coding_agent_specs::{builtin_specs, CodingAgentRegistry};

#[test]
fn list_models_honours_a_tools_dir_override_of_the_curated_models() {
    // Given an operator override of `claude.yaml` whose curated models are its own
    let tools = tempfile::tempdir().expect("a tools directory");
    an_override_of_claudes_curated_models(
        tools.path(),
        "claude-house-model",
        &[
            ("claude-house-model", "House Model"),
            ("claude-review-model", "Review Model"),
        ],
    );

    // When the catalogue of `claude-cli` is listed with that tools directory, as the probe asks
    let listed = list_models("claude-cli", tools.path());

    // Then it is the override's list, not the built-in one
    assert_eq!(
        listed,
        json!({
            "models": [
                { "id": "claude-house-model", "label": "House Model" },
                { "id": "claude-review-model", "label": "Review Model" },
            ],
            "default_model": "claude-house-model",
        })
    );
}

#[test]
fn the_override_fixture_is_a_claude_spec_with_the_given_models() {
    // Given
    let tools = tempfile::tempdir().expect("a tools directory");
    an_override_of_claudes_curated_models(
        tools.path(),
        "claude-house-model",
        &[("claude-house-model", "House Model")],
    );

    // When the host registry is loaded over it
    let registry = CodingAgentRegistry::load(tools.path()).expect("the override loads");

    // Then claude's curated models are the override's — so the listing test above fails only on
    // what `list-models` reads, never on its fixture
    assert_eq!(
        registry
            .get("claude")
            .expect("claude is an agent")
            .curated_models(),
        (
            vec![("claude-house-model".to_string(), "House Model".to_string())],
            "claude-house-model".to_string()
        )
    );
}

#[test]
fn list_models_refuses_a_tools_dir_that_does_not_load_naming_the_file() {
    // Given an operator override of `claude.yaml` that is not YAML
    let tools = tempfile::tempdir().expect("a tools directory");
    let the_file = tools.path().join("claude.yaml");
    std::fs::write(&the_file, "id: claude\nrun: [unterminated\n").expect("the broken override");

    // When
    let output = run_list_models("claude-cli", tools.path());

    // Then the listing fails rather than answering from the built-ins, and names the file.
    // Containment for the file: the rest of the message is the YAML parser's.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success() && stderr.contains(&the_file.display().to_string()),
        "list-models must refuse naming {}; it exited {:?} with stderr: {stderr}",
        the_file.display(),
        output.status.code()
    );
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The lines of the built-in `claude.yaml` its `coding_agent.models` block spans.
const MODELS_BLOCK_START: &str = "    models:\n";
const MODELS_BLOCK_END: &str = "    context_globs:";

/// Write `<tools>/claude.yaml`: the built-in spec with its `models` block replaced by `curated`
/// and `default`, as an operator overrides the file whole.
fn an_override_of_claudes_curated_models(tools: &Path, default: &str, curated: &[(&str, &str)]) {
    let builtin = the_builtin_yaml("claude");
    let start = builtin
        .find(MODELS_BLOCK_START)
        .expect("the built-in claude.yaml has a models block");
    let end = builtin
        .find(MODELS_BLOCK_END)
        .expect("the built-in claude.yaml's models block is followed by context_globs");
    let entries: String = curated
        .iter()
        .map(|(id, label)| format!("        - {{ id: {id}, label: \"{label}\" }}\n"))
        .collect();
    let models = format!("{MODELS_BLOCK_START}      default: {default}\n      curated:\n{entries}");
    let overridden = format!("{}{models}{}", &builtin[..start], &builtin[end..]);
    std::fs::write(tools.join("claude.yaml"), overridden).expect("the override is written");
}

fn the_builtin_yaml(id: &str) -> &'static str {
    let file_name = format!("/{id}.yaml");
    builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with(&file_name))
        .map(|(_, yaml)| yaml)
        .unwrap_or_else(|| panic!("{id}.yaml is built in"))
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Run `tddy-tools list-models --agent <agent> --tools-dir <tools>`.
fn run_list_models(agent: &str, tools: &Path) -> std::process::Output {
    Command::cargo_bin("tddy-tools")
        .expect("the tddy-tools binary")
        .args(["list-models", "--agent", agent, "--tools-dir"])
        .arg(tools)
        .output()
        .expect("list-models runs")
}

/// [`run_list_models`], read as the JSON it prints on success.
fn list_models(agent: &str, tools: &Path) -> Value {
    let output = run_list_models(agent, tools);
    assert!(
        output.status.success(),
        "list-models failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("list-models prints JSON")
}
