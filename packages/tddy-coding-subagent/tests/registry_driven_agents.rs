//! A coding subagent's transport, permission policy, catalog and confinement come from the host's
//! coding-agent registry, never from the agent's name (`#agent-tools 27/28`).
//!
//! Every registry here is a real `CodingAgentRegistry::load` over a `<tddyhome>/tools` directory:
//! the built-in specs, plus the override or dropped-in agent file each test writes. An override
//! that changes what a built-in agent declares is what tells "read from the spec" from "known by
//! name": the name stays, the declaration moves.
//!
//! Changeset: `docs/dev/1-WIP/2026-11-06-acp-subagent-leftovers.md`.

use pretty_assertions::assert_eq;
use tddy_coding_agent_specs::{
    CodingAgentRegistry, CodingAgentSpec, CLAUDE_ACP_SPEC_YAML, CURSOR_SPEC_YAML,
    OPENCODE_SPEC_YAML,
};
use tddy_coding_subagent::{
    coding_agent_defs_from, permission_policy_of_spec, refuse_unless_registry_confinable,
    transport_of_spec, CodingAgentDef, PermissionPolicy, Transport,
};
use tddy_discovery::harness_profile::HarnessProfile;

/// A `<tddyhome>/tools` directory and the registry loaded over it.
struct AHost {
    tools: tempfile::TempDir,
}

fn a_host() -> AHost {
    AHost {
        tools: tempfile::tempdir().expect("tools dir"),
    }
}

impl AHost {
    /// With `<tools>/<id>.yaml` holding `yaml`.
    fn with_tool(self, id: &str, yaml: &str) -> Self {
        std::fs::write(self.tools.path().join(format!("{id}.yaml")), yaml).expect("write tool");
        self
    }

    fn registry(&self) -> CodingAgentRegistry {
        CodingAgentRegistry::load(self.tools.path()).expect("the registry loads")
    }
}

/// `spec_yaml` as the spec of another agent, `id`.
fn renamed(spec_yaml: &str, from: &str, id: &str) -> String {
    spec_yaml.replace(&format!("id: {from}\n"), &format!("id: {id}\n"))
}

fn the_spec<'a>(registry: &'a CodingAgentRegistry, id: &str) -> &'a CodingAgentSpec {
    registry
        .get(id)
        .unwrap_or_else(|| panic!("the registry declares {id}"))
}

fn a_profile(name: &str, harness: &str) -> HarnessProfile {
    HarnessProfile {
        name: name.to_string(),
        label: None,
        harness: harness.to_string(),
        provider_account: String::new(),
        model: "some-model".to_string(),
    }
}

fn names(defs: &[CodingAgentDef]) -> Vec<&str> {
    defs.iter().map(|def| def.name.as_str()).collect()
}

/// `tools/cursor.yaml`'s fixed policy text, replaced.
const OPERATOR_CURSOR_POLICY: &str = "read-only; this host's operator says so";

fn the_builtin_cursor_policy_text() -> &'static str {
    "read-only; the CLI cannot ask mid-turn"
}

// ── Permission policy ────────────────────────────────────────────────────────────────────────

#[test]
fn a_headless_agents_permission_policy_is_the_text_its_spec_declares() {
    // Given — an operator's `tools/cursor.yaml` declaring its own fixed policy
    let host = a_host().with_tool(
        "cursor",
        &CURSOR_SPEC_YAML.replace(the_builtin_cursor_policy_text(), OPERATOR_CURSOR_POLICY),
    );
    let registry = host.registry();

    // When
    let policy = permission_policy_of_spec(the_spec(&registry, "cursor"));

    // Then
    assert_eq!(
        policy,
        Some(PermissionPolicy::Fixed(OPERATOR_CURSOR_POLICY.to_string()))
    );
}

#[test]
fn the_built_in_agents_declare_the_permission_policies_they_run_under_today() {
    // Given
    let registry = a_host().registry();

    // When
    let policies: Vec<(&str, Option<PermissionPolicy>)> = [
        "claude",
        "cursor",
        "codex",
        "oz",
        "claude-acp",
        "codex-acp",
        "opencode",
    ]
    .into_iter()
    .map(|id| (id, permission_policy_of_spec(the_spec(&registry, id))))
    .collect();

    // Then
    let fixed = |text: &str| Some(PermissionPolicy::Fixed(text.to_string()));
    assert_eq!(
        policies,
        vec![
            ("claude", Some(PermissionPolicy::AsksCaller)),
            ("cursor", fixed(the_builtin_cursor_policy_text())),
            (
                "codex",
                fixed("workspace-write sandbox; the CLI cannot ask mid-turn")
            ),
            (
                "oz",
                fixed(
                    "the agent profile decides; its default profile auto-approves commands and edits"
                )
            ),
            ("claude-acp", Some(PermissionPolicy::AsksCaller)),
            ("codex-acp", Some(PermissionPolicy::AsksCaller)),
            ("opencode", Some(PermissionPolicy::AsksCaller)),
        ]
    );
}

// ── Transport ────────────────────────────────────────────────────────────────────────────────

#[test]
fn a_dropped_in_agent_with_a_headless_section_is_spoken_to_headless() {
    // Given — `tools/acme.yaml`: cursor's declaration under an id no table names
    let host = a_host().with_tool("acme", &renamed(CURSOR_SPEC_YAML, "cursor", "acme"));
    let registry = host.registry();

    // When
    let transport = (
        transport_of_spec(the_spec(&registry, "acme")),
        permission_policy_of_spec(the_spec(&registry, "acme")),
    );

    // Then
    assert_eq!(
        transport,
        (
            Some(Transport::Headless),
            Some(PermissionPolicy::Fixed(
                the_builtin_cursor_policy_text().to_string()
            ))
        )
    );
}

#[test]
fn the_built_in_agents_keep_the_transports_they_have_today_and_a_pty_only_agent_has_none() {
    // Given
    let registry = a_host().registry();

    // When
    let transports: Vec<(&str, Option<Transport>)> = [
        "claude-acp",
        "codex-acp",
        "opencode",
        "claude",
        "cursor",
        "codex",
        "oz",
        "warp",
    ]
    .into_iter()
    .map(|id| (id, transport_of_spec(the_spec(&registry, id))))
    .collect();

    // Then
    assert_eq!(
        transports,
        vec![
            ("claude-acp", Some(Transport::Acp)),
            ("codex-acp", Some(Transport::Acp)),
            ("opencode", Some(Transport::Acp)),
            ("claude", Some(Transport::Headless)),
            ("cursor", Some(Transport::Headless)),
            ("codex", Some(Transport::Headless)),
            ("oz", Some(Transport::Headless)),
            ("warp", None),
        ]
    );
}

// ── Catalog ──────────────────────────────────────────────────────────────────────────────────

#[test]
fn a_profile_on_a_dropped_in_headless_agent_is_offered() {
    // Given
    let host = a_host().with_tool("acme", &renamed(CURSOR_SPEC_YAML, "cursor", "acme"));
    let registry = host.registry();

    // When
    let defs = coding_agent_defs_from(&registry, &[a_profile("acme-reviewer", "acme")], &[]);

    // Then
    assert_eq!(names(&defs), vec!["acme-reviewer"]);
}

#[test]
fn an_allowed_acp_agent_runs_the_default_model_its_hosts_spec_declares() {
    // Given — an operator's `tools/claude-acp.yaml` moving its default model
    let builtin_default = the_spec(&a_host().registry(), "claude-acp")
        .agent
        .models
        .default
        .clone();
    let host = a_host().with_tool(
        "claude-acp",
        &CLAUDE_ACP_SPEC_YAML.replacen(
            &format!("default: {builtin_default}"),
            "default: operator-model",
            1,
        ),
    );
    let registry = host.registry();

    // When
    let defs = coding_agent_defs_from(&registry, &[], &[("claude-acp".to_string(), None)]);

    // Then
    assert_eq!(
        defs.iter()
            .map(|def| def.model.as_str())
            .collect::<Vec<_>>(),
        vec!["operator-model"]
    );
}

#[test]
fn an_allowed_id_is_offered_only_when_the_registry_declares_it_an_acp_agent() {
    // Given — two ACP agents, a headless one and an id no spec declares
    let registry = a_host().registry();
    let allowed: Vec<(String, Option<String>)> = ["claude-acp", "opencode", "cursor", "nobody"]
        .into_iter()
        .map(|id| (id.to_string(), None))
        .collect();

    // When
    let defs = coding_agent_defs_from(&registry, &[], &allowed);

    // Then
    assert_eq!(names(&defs), vec!["claude-acp", "opencode"]);
}

// ── Attach-time confinement ──────────────────────────────────────────────────────────────────

#[test]
fn a_managed_session_refuses_an_acp_agent_whose_hosts_spec_cannot_withdraw_its_tools() {
    // Given — an operator's `tools/opencode.yaml` whose ACP deny slot is `unsupported`
    let host = a_host().with_tool(
        "opencode",
        &OPENCODE_SPEC_YAML.replace(
            "tool_deny: { config_env: OPENCODE_CONFIG_CONTENT }",
            "tool_deny: unsupported",
        ),
    );
    let registry = host.registry();

    // When
    let refused = refuse_unless_registry_confinable(&registry, "opencode", true);

    // Then — refused, naming its declaration and the one agent left that can be confined
    assert_eq!(
        refused,
        Err(
            "the 'opencode' agent cannot withdraw its native file-system and shell tools (its \
             spec declares tool_deny: unsupported), and a managed session's main agent has none, \
             so its subagent must not either: attach a coding agent that can be confined \
             (claude-acp) or a model agent"
                .to_string()
        )
    );
}

#[test]
fn the_built_in_acp_agents_keep_the_confinement_they_have_today() {
    // Given
    let registry = a_host().registry();

    // When
    let admitted = [
        refuse_unless_registry_confinable(&registry, "claude-acp", true).is_ok(),
        refuse_unless_registry_confinable(&registry, "opencode", true).is_ok(),
        refuse_unless_registry_confinable(&registry, "codex-acp", true).is_ok(),
        refuse_unless_registry_confinable(&registry, "codex-acp", false).is_ok(),
    ];

    // Then
    assert_eq!(admitted, [true, true, false, true]);
}

#[test]
fn a_managed_session_still_refuses_a_headless_agent_and_an_agent_the_host_does_not_declare() {
    // Given
    let registry = a_host().registry();

    // When
    let refused = (
        refuse_unless_registry_confinable(&registry, "claude", true).is_err(),
        refuse_unless_registry_confinable(&registry, "nobody", true).is_err(),
    );

    // Then
    assert_eq!(refused, (true, true));
}
