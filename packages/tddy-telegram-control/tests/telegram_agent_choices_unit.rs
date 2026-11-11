//! Unit: what Telegram offers to start is read from the host's coding-agent registry — its CLI
//! agents, each agent's models from its spec, the branch name a spec requires — and every pick is
//! named by an index that fits Telegram's 64-byte `callback_data` (`#agent-tools 18/19`).

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_coding_agent_specs::{CodingAgentRegistry, CodingAgentSpec};
use tddy_telegram_control::telegram_session_control::{
    parse_start_agent_prompt, parse_telegram_agent_pick_callback,
    parse_telegram_model_pick_callback, telegram_agent_pick_callback, telegram_model_at,
    telegram_model_choices, telegram_model_pick_callback, telegram_new_branch_name,
    TelegramAgentChoice, TelegramAgentChoices, TelegramModelChoice, TelegramPickRefusal,
};

const SESSION: &str = "019d6392-3cff-7018-8000-000000000018";

/// `<tddyhome>/tools/<id>.yaml` declaring a PTY agent `id` with `capabilities` and `curated`
/// models (its `models.default` the first of them, or empty).
fn an_agent_yaml(id: &str, capabilities: &str, curated: &[&str]) -> String {
    let default = curated.first().copied().unwrap_or("");
    format!(
        r#"schema_version: 1
id: {id}
summary: {id} — a fixture agent
binary:
  command: {id}
  env: TDDY_{upper}_CLI
deps: [tddy-tools]
reads: []
run:
  transport: pty
  argv:
    - "{{binary}}"
    - {{ if: model, then: ["--model", "{{model}}"] }}
  env: {{}}
extensions:
  coding_agent:
    mcp: {{ kind: unsupported }}
    tool_allow: unsupported
    tool_deny: unsupported
    permission_prompt: unsupported
    instructions:
      files: []
    hooks: {{ adapter: unsupported }}
    capabilities: [{capabilities}]
    models:
      default: "{default}"
      curated: [{curated}]
    placements: [host]
"#,
        upper = id.to_uppercase(),
        curated = curated.join(", "),
    )
}

/// The registry of the built-ins plus `files` dropped into a `<tddyhome>/tools`.
fn a_registry_with(files: &[(&str, String)]) -> CodingAgentRegistry {
    let tools = tempfile::tempdir().unwrap();
    for (name, yaml) in files {
        std::fs::write(tools.path().join(name), yaml).unwrap();
    }
    CodingAgentRegistry::load(tools.path()).expect("the fixture registry loads")
}

fn the_builtins() -> CodingAgentRegistry {
    CodingAgentRegistry::load(Path::new("/nonexistent-tddy-home/tools"))
        .expect("the built-in agents load")
}

fn spec<'r>(registry: &'r CodingAgentRegistry, id: &str) -> &'r CodingAgentSpec {
    registry
        .get(id)
        .unwrap_or_else(|| panic!("{id} is in the registry"))
}

fn models(ids: &[&str]) -> Vec<TelegramModelChoice> {
    ids.iter()
        .map(|id| TelegramModelChoice {
            model_id: id.to_string(),
            label: id.to_string(),
        })
        .collect()
}

#[test]
fn the_offered_agents_are_the_registrys_pty_agents_by_id_with_their_cli_session_types() {
    // Given
    let registry = a_registry_with(&[("acme.yaml", an_agent_yaml("acme", "", &["acme-1"]))]);

    // When
    let offered = TelegramAgentChoices::of(&registry);

    // Then — the ACP-only built-ins are not CLI agents
    assert_eq!(
        offered.choices().to_vec(),
        ["acme", "claude", "codex", "cursor", "muse", "opencode", "oz", "warp"]
            .map(|id| TelegramAgentChoice {
                agent_id: id.to_string(),
                session_type: format!("{id}-cli"),
            })
            .to_vec()
    );
}

#[test]
fn an_agent_index_round_trips_through_its_callback() {
    // Given
    let offered = TelegramAgentChoices::of(&the_builtins());

    // When
    let (agent_idx, session_id) =
        parse_telegram_agent_pick_callback(&telegram_agent_pick_callback(2, SESSION))
            .expect("the agent callback decodes");

    // Then
    assert_eq!(
        (
            offered.at(agent_idx).map(|choice| choice.agent_id.clone()),
            session_id
        ),
        (Ok("cursor".to_string()), SESSION.to_string())
    );
}

#[test]
fn an_agent_index_past_the_offered_agents_is_refused() {
    // Given
    let offered = TelegramAgentChoices::of(&the_builtins());

    // When
    let refused = offered.at(7).map(|choice| choice.agent_id.clone());

    // Then
    assert_eq!(
        refused,
        Err(TelegramPickRefusal::AgentIndexOutOfRange {
            agent_idx: 7,
            offered: 7
        })
    );
}

#[test]
fn a_model_and_project_index_round_trip_through_the_model_callback() {
    // When
    let decoded = parse_telegram_model_pick_callback(&telegram_model_pick_callback(1, 3, SESSION));

    // Then
    assert_eq!(decoded, Some((1, 3, SESSION.to_string())));
}

#[test]
fn the_model_choices_are_the_specs_default_then_its_other_curated_models_labelled_by_id() {
    // Given
    let registry = the_builtins();

    // When
    let claude = telegram_model_choices(spec(&registry, "claude"));

    // Then
    assert_eq!(
        claude,
        Ok(models(&[
            "opus",
            "fable",
            "sonnet",
            "haiku",
            "claude-opus-5",
            "claude-fable-5-1",
            "claude-sonnet-5",
            "claude-haiku-4-5-20251001"
        ]))
    );
}

#[test]
fn an_agent_offering_no_models_and_requiring_none_has_no_model_choices() {
    // Given
    let registry = the_builtins();

    // When
    let warp = telegram_model_choices(spec(&registry, "warp"));

    // Then
    assert_eq!(warp, Ok(Vec::new()));
}

#[test]
fn a_requires_model_agent_whose_spec_offers_none_is_refused_naming_it() {
    // Given
    let registry = a_registry_with(&[("bare.yaml", an_agent_yaml("bare", "requires_model", &[]))]);

    // When
    let refused =
        telegram_model_choices(spec(&registry, "bare")).map_err(|refusal| refusal.to_string());

    // Then
    assert_eq!(
        refused,
        Err("bare requires a model, and its tool definition offers none".to_string())
    );
}

#[test]
fn a_model_index_past_the_specs_models_is_refused_naming_the_agent() {
    // Given
    let registry = the_builtins();

    // When
    let refused = telegram_model_at(spec(&registry, "cursor"), 3);

    // Then
    assert_eq!(
        refused,
        Err(TelegramPickRefusal::ModelIndexOutOfRange {
            agent_id: "cursor".to_string(),
            model_idx: 3,
            offered: 3,
        })
    );
}

#[test]
fn a_branch_name_required_agent_names_its_branch_from_the_changeset_name() {
    // Given
    let registry = the_builtins();

    // When
    let branch = telegram_new_branch_name(spec(&registry, "claude"), "Auth Feature");

    // Then
    assert_eq!(branch, Some("feature/auth-feature".to_string()));
}

#[test]
fn an_agent_without_branch_name_required_leaves_the_branch_name_to_the_start() {
    // Given
    let registry = the_builtins();

    // When
    let branch = telegram_new_branch_name(spec(&registry, "cursor"), "Auth Feature");

    // Then
    assert_eq!(branch, None);
}

#[test]
fn the_start_agent_command_carries_its_prompt() {
    // When
    let prompt = parse_start_agent_prompt("/start-agent  fix the flaky test ");

    // Then
    assert_eq!(prompt, Some("fix the flaky test".to_string()));
}
