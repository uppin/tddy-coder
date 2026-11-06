//! Unit: what a route body is told about an agent spec — its product, its route support and the
//! variable its key is handed in — and the shape words a spec may write (`#agent-tools 13/19`).
//!
//! Feature doc: `docs/ft/daemon/provider-accounts.md`. Each spec here is a tool file written in
//! the test, read straight from its `run` and `coding_agent` sections.

use pretty_assertions::assert_eq;
use tddy_actions::tool::RunSection;
use tddy_coding_agent_specs::{
    routed_agent, CodingAgentSection, CodingAgentSpec, ProviderKind, RouteShape, RouteSupport,
    RoutedAgent,
};

/// A `lark` agent whose `coding_agent` section is a minimal one plus `extra` (each line indented
/// four spaces, as that section's keys are).
fn a_lark_spec_declaring(extra: &str) -> CodingAgentSpec {
    let yaml = format!(
        r#"run:
  transport: pty
  argv: ["{{binary}}"]
  env: {{}}
coding_agent:
    mcp: {{ kind: unsupported }}
    tool_allow: unsupported
    tool_deny: unsupported
    permission_prompt: unsupported
    instructions:
      files: [AGENTS.md]
    hooks: {{ adapter: claude-settings-local }}
    models:
      default: lark-1
      curated: [lark-1]
{extra}"#
    );
    let file: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("the spec is YAML");
    let run: RunSection = serde_yaml::from_value(file["run"].clone()).expect("its run section");
    let agent: CodingAgentSection =
        serde_yaml::from_value(file["coding_agent"].clone()).expect("its coding_agent section");
    CodingAgentSpec {
        id: "lark".to_string(),
        run,
        agent,
    }
}

#[test]
fn a_spec_declaring_a_product_is_named_by_it() {
    // Given
    let spec = a_lark_spec_declaring("    product: Lark Code\n");

    // When
    let product = spec.product();

    // Then
    assert_eq!(product, "Lark Code");
}

#[test]
fn a_spec_declaring_no_product_is_named_by_its_id() {
    // Given
    let spec = a_lark_spec_declaring("");

    // When
    let product = spec.product();

    // Then
    assert_eq!(product, "lark");
}

#[test]
fn a_routed_agent_carries_the_specs_id_product_route_support_and_key_env() {
    // Given
    let spec = a_lark_spec_declaring(
        "    product: Lark Code\n    route:\n      providers: [meta]\n      shape: env\n      key_env: LARK_API_KEY\n",
    );

    // When
    let agent = routed_agent(&spec);

    // Then
    assert_eq!(
        agent,
        RoutedAgent {
            id: "lark".to_string(),
            product: "Lark Code".to_string(),
            support: RouteSupport {
                providers: vec![ProviderKind::Meta],
                shape: RouteShape::Env,
            },
            key_env: Some("LARK_API_KEY".to_string()),
        }
    );
}

#[test]
fn a_spec_without_a_route_is_a_routed_agent_with_no_providers_named_by_its_id() {
    // Given
    let spec = a_lark_spec_declaring("");

    // When
    let agent = routed_agent(&spec);

    // Then — no providers, so every body refuses it; the shape is only the parse default
    assert_eq!(
        agent,
        RoutedAgent {
            id: "lark".to_string(),
            product: "lark".to_string(),
            support: RouteSupport::default(),
            key_env: None,
        }
    );
}

#[test]
fn a_removed_shape_word_is_refused_naming_the_word_that_replaced_it() {
    // When
    let refusal_of = |word: &str| {
        serde_yaml::from_str::<RouteShape>(word)
            .expect_err("a removed shape word")
            .to_string()
    };
    let (args, config_file) = (refusal_of("args"), refusal_of("config_file"));

    // Then — the refusal may carry serde's position after the sentence
    assert_eq!(
        (
            args.starts_with("route.shape `args` was renamed to `toml_args`"),
            config_file.starts_with("route.shape `config_file` was renamed to `json_config_file`"),
        ),
        (true, true),
        "args: {args}\nconfig_file: {config_file}"
    );
}
