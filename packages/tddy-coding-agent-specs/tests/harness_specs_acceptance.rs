//! Acceptance: `specs/opencode.yaml`, `specs/muse.yaml` and `specs/warp.yaml` are built-in coding
//! agents whose `coding_agent` sections carry what the `CliHarness` descriptors held — the route a
//! jail needs, the model listing and its adapter, the resume adapter and the placements. Their
//! launches are exercised by `tddy-session-lifecycle` `harness_clis_from_spec_acceptance`.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-harness-clis-from-specs.md` (`#agent-tools 8/12`).

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ArgvItem, ToolRegistry};
use tddy_coding_agent_specs::{
    load_agent_registry, CodingAgentSpec, McpSlot, ModelsSection, Placement, ResumeSlot, RouteSlot,
};

fn the_builtin_registry() -> (tempfile::TempDir, ToolRegistry) {
    let home = tempfile::tempdir().unwrap();
    let registry = load_agent_registry(&home.path().join("tools")).expect("the built-ins load");
    (home, registry)
}

fn the_spec(registry: &ToolRegistry, id: &str) -> CodingAgentSpec {
    CodingAgentSpec::of(
        registry
            .get(id)
            .unwrap_or_else(|| panic!("{id} is built in")),
    )
    .unwrap_or_else(|e| panic!("{id} is a coding agent: {e}"))
}

#[test]
fn opencode_yaml_requires_a_route_in_a_jail_names_its_config_file_and_key_and_lists_its_models_through_opencode_models(
) {
    // Given
    let (_home, registry) = the_builtin_registry();

    // When
    let opencode = the_spec(&registry, "opencode").agent;

    // Then
    assert_eq!(
        (
            opencode.route,
            opencode.models,
            opencode.resume,
            opencode.placements,
            opencode.mcp,
        ),
        (
            Some(RouteSlot {
                required_in_jail: true,
                config_env: Some("OPENCODE_CONFIG".to_string()),
                files_dir: Some("provider-route".to_string()),
                key_env: Some("TDDY_OPENCODE_PROVIDER_KEY".to_string()),
            }),
            ModelsSection {
                default: String::new(),
                curated: Vec::new(),
                list_command: Some(vec![
                    ArgvItem::Lit("{binary}".to_string()),
                    ArgvItem::Lit("models".to_string()),
                ]),
                adapter: Some("opencode-models".to_string()),
            },
            None,
            vec![Placement::Host, Placement::Jail],
            McpSlot::Unsupported,
        )
    );
}

#[test]
fn muse_yaml_requires_a_route_in_a_jail_hands_its_key_in_meta_api_key_and_runs_without_jit() {
    // Given
    let (_home, registry) = the_builtin_registry();

    // When
    let def = registry.get("muse").expect("muse is built in");
    let muse = the_spec(&registry, "muse").agent;

    // Then
    assert_eq!(
        (
            muse.route,
            muse.models.default,
            muse.models.list_command,
            def.policy.jit,
            muse.placements,
        ),
        (
            Some(RouteSlot {
                required_in_jail: true,
                config_env: None,
                files_dir: None,
                key_env: Some("META_API_KEY".to_string()),
            }),
            "muse-spark-1.3".to_string(),
            None,
            false,
            vec![Placement::Host, Placement::Jail],
        )
    );
}

#[test]
fn warp_yaml_offers_no_model_resumes_through_warp_resume_token_and_runs_only_on_the_host() {
    // Given
    let (_home, registry) = the_builtin_registry();

    // When
    let warp = the_spec(&registry, "warp").agent;

    // Then
    assert_eq!(
        (warp.models, warp.resume, warp.placements, warp.route),
        (
            ModelsSection {
                default: String::new(),
                curated: Vec::new(),
                list_command: None,
                adapter: None,
            },
            Some(ResumeSlot {
                adapter: "warp-resume-token".to_string(),
            }),
            vec![Placement::Host],
            None,
        )
    );
}

#[test]
fn the_builtin_registry_holds_opencode_muse_and_warp_beside_claude_and_cursor() {
    // Given
    let (_home, registry) = the_builtin_registry();

    // When
    let ids: Vec<String> = ["claude", "cursor", "opencode", "muse", "warp"]
        .iter()
        .filter_map(|id| registry.get(id))
        .filter_map(|def| CodingAgentSpec::of(def).ok())
        .map(|spec| spec.id)
        .collect();

    // Then
    assert_eq!(ids, vec!["claude", "cursor", "opencode", "muse", "warp"]);
}
