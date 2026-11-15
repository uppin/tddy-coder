//! Acceptance: the agent a split session runs on this host is a declared variant of `claude.yaml`
//! (`#agent-tools 10/12`), not a hand-built argv.
//!
//! Docs: `docs/ft/coder/sandbox-builder.md`. The spec is read with serde
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
            hooks_flag: None,
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

#[test]
fn claudes_host_agent_loads_the_daemons_hooks_file_with_settings() {
    // Given Claude, whose host_agent variant keeps the checkout's settings out with
    // `--setting-sources user` — which also skips the hooks file the daemon writes
    let claude = the_embedded_spec("claude");
    let hooks_file = std::path::Path::new("/sessions/s1/context/.claude/settings.local.json");

    // When the variant's hooks flags are rendered for that file
    let args = claude
        .variant_hooks_args(HOST_AGENT_VARIANT, hooks_file)
        .expect("claude declares a host_agent variant");

    // Then the file is loaded explicitly with `--settings`
    assert_eq!(
        args,
        vec![
            "--settings".to_string(),
            "/sessions/s1/context/.claude/settings.local.json".to_string()
        ]
    );
}

#[test]
fn a_variant_declaring_no_hooks_flag_loads_no_hooks_file() {
    // Given an agent whose host_agent variant declares no hooks flag
    let mut claude = the_embedded_spec("claude");
    claude.agent.variants.insert(
        HOST_AGENT_VARIANT.to_string(),
        tddy_coding_agent_specs::AgentVariant {
            args: vec!["--strict-mcp-config".to_string()],
            withdraw_native: vec![],
            hooks_flag: None,
        },
    );

    // When its hooks flags are rendered
    let args = claude
        .variant_hooks_args(
            HOST_AGENT_VARIANT,
            std::path::Path::new("/any/.claude/settings.local.json"),
        )
        .expect("the variant is declared");

    // Then there are none
    assert_eq!(args, Vec::<String>::new());
}

#[test]
fn hooks_flags_for_an_undeclared_variant_are_refused() {
    // Given Cursor, which declares no host_agent variant
    let cursor = the_embedded_spec("cursor");

    // When its host_agent hooks flags are asked for
    let refused = cursor.variant_hooks_args(
        HOST_AGENT_VARIANT,
        std::path::Path::new("/any/.claude/settings.local.json"),
    );

    // Then the variant is refused, as its args would be
    assert!(
        matches!(refused, Err(VariantError::Unknown { .. })),
        "an undeclared variant must be refused, got {refused:?}"
    );
}
