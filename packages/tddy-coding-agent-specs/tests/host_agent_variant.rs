//! Acceptance: the agent a split session runs on this host is a declared variant of `claude.yaml`
//! (`#agent-tools 10/12`), not a hand-built argv.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-tool-agnostic-jail.md`. The spec is read with serde
//! straight from the embedded file, so these tests exercise the variant's own surface and nothing
//! the registry still has to deliver.

use pretty_assertions::assert_eq;
use tddy_actions::tool::RunSection;
use tddy_coding_agent_specs::{
    builtin_specs, CodingAgentSection, CodingAgentSpec, VariantError, HOST_AGENT_VARIANT,
};

/// The embedded spec with id `id`, read as its coding-agent view.
fn the_embedded_spec(id: &str) -> CodingAgentSpec {
    let (_, yaml) = builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with(&format!("/{id}.yaml")))
        .expect("an embedded spec");
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml).expect("the spec is YAML");
    let run: RunSection =
        serde_yaml::from_value(doc["run"].clone()).expect("the spec has a run section");
    let agent: CodingAgentSection =
        serde_yaml::from_value(doc["extensions"]["coding_agent"].clone())
            .expect("the spec's coding_agent section deserializes");
    CodingAgentSpec {
        id: id.to_string(),
        run,
        agent,
    }
}

/// `--disallowedTools <tool>` for each of `tools`.
fn disallowed(tools: &[&str]) -> Vec<String> {
    tools
        .iter()
        .flat_map(|tool| ["--disallowedTools".to_string(), (*tool).to_string()])
        .collect()
}

#[test]
fn the_claude_host_agent_variant_loads_no_checkout_config_and_withdraws_every_native_tool() {
    // Given
    let claude = the_embedded_spec("claude");

    // When
    let args = claude
        .variant_args(HOST_AGENT_VARIANT)
        .expect("claude declares a host_agent variant");

    // Then
    let mut expected: Vec<String> = ["--strict-mcp-config", "--setting-sources", "user"]
        .map(str::to_string)
        .to_vec();
    expected.extend(disallowed(&[
        "Read",
        "Write",
        "Edit",
        "MultiEdit",
        "NotebookEdit",
        "Bash",
        "BashOutput",
        "KillShell",
        "Glob",
        "Grep",
        "LS",
    ]));
    assert_eq!(args, expected);
}

#[test]
fn a_variant_the_agent_does_not_declare_is_refused_naming_the_ones_it_does() {
    // Given
    let claude = the_embedded_spec("claude");

    // When
    let refused = claude.variant_args("headless_host");

    // Then
    assert_eq!(
        refused,
        Err(VariantError::Unknown {
            agent: "claude".to_string(),
            variant: "headless_host".to_string(),
            declared: vec![HOST_AGENT_VARIANT.to_string()],
        })
    );
}

#[test]
fn a_variant_that_withdraws_tools_through_an_unsupported_deny_slot_is_refused() {
    // Given — Cursor cannot withdraw a native tool, and is handed a variant that tries to
    let mut cursor = the_embedded_spec("cursor");
    cursor.agent.variants.insert(
        HOST_AGENT_VARIANT.to_string(),
        tddy_coding_agent_specs::AgentVariant {
            args: vec![],
            withdraw_native: vec!["Shell".to_string()],
        },
    );

    // When
    let refused = cursor.variant_args(HOST_AGENT_VARIANT);

    // Then
    assert_eq!(
        refused,
        Err(VariantError::DenyUnsupported {
            agent: "cursor".to_string(),
            variant: HOST_AGENT_VARIANT.to_string(),
        })
    );
}
