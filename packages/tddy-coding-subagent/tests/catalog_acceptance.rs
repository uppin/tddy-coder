//! Which coding agents a daemon offers as subagents, and which of them a managed session can run.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use tddy_coding_subagent::{coding_agent_defs, confinement_for, CodingAgentDef, Confinement};
use tddy_discovery::harness_profile::HarnessProfile;

fn a_profile(name: &str, harness: &str, model: &str) -> HarnessProfile {
    HarnessProfile {
        name: name.to_string(),
        label: Some(format!("{name} label")),
        harness: harness.to_string(),
        provider_account: "meta".to_string(),
        model: model.to_string(),
    }
}

#[test]
fn an_acp_harness_profile_is_a_coding_agent() {
    // Given
    let profiles = [a_profile("codex-fast", "codex-acp", "gpt-5.2")];

    // When
    let defs = coding_agent_defs(&profiles, &[]);

    // Then
    assert_eq!(
        defs,
        [CodingAgentDef {
            name: "codex-fast".to_string(),
            label: Some("codex-fast label".to_string()),
            harness: "codex-acp".to_string(),
            model: "gpt-5.2".to_string(),
            provider_account: Some("meta".to_string()),
        }]
    );
}

#[test]
fn a_pty_only_harness_profile_is_not_offered() {
    // Given
    let profiles = [a_profile("tui-claude", "claude-cli", "opus")];

    // When
    let defs = coding_agent_defs(&profiles, &[]);

    // Then
    assert!(defs.is_empty(), "a PTY-only harness is not offered: {defs:?}");
}

#[test]
fn an_acp_allowed_agent_is_a_coding_agent_on_its_default_model() {
    // Given
    let allowed = [
        ("claude-acp".to_string(), Some("Claude (ACP)".to_string())),
        ("cursor".to_string(), None),
    ];

    // When
    let defs = coding_agent_defs(&[], &allowed);

    // Then
    assert_eq!(defs.len(), 1, "only the ACP id: {defs:?}");
    assert_eq!(defs[0].name, "claude-acp");
    assert_eq!(defs[0].harness, "claude-acp");
    assert_eq!(defs[0].label.as_deref(), Some("Claude (ACP)"));
    assert_eq!(
        defs[0].model,
        tddy_agent_backend::default_model_for_agent("claude-acp")
    );
    assert_eq!(defs[0].provider_account, None);
}

#[test]
fn profiles_come_before_allowed_agents() {
    // Given
    let profiles = [a_profile("opencode-kimi", "opencode", "fireworks/kimi-k2")];
    let allowed = [("codex-acp".to_string(), None)];

    // When
    let names: Vec<String> = coding_agent_defs(&profiles, &allowed)
        .into_iter()
        .map(|def| def.name)
        .collect();

    // Then
    assert_eq!(names, ["opencode-kimi", "codex-acp"]);
}

#[test]
fn a_profile_shadows_an_allowed_agent_of_the_same_name() {
    // Given
    let profiles = [a_profile("claude-acp", "claude-acp", "opus")];
    let allowed = [("claude-acp".to_string(), Some("Claude (ACP)".to_string()))];

    // When
    let defs = coding_agent_defs(&profiles, &allowed);

    // Then
    assert_eq!(defs.len(), 1, "one def per name: {defs:?}");
    assert_eq!(defs[0].label.as_deref(), Some("claude-acp label"));
}

#[test]
fn claude_can_withdraw_its_native_tools() {
    // When
    let confinement = confinement_for("claude-acp");

    // Then
    assert_eq!(confinement, Confinement::WithdrawsNativeTools);
}

#[test]
fn opencode_can_withdraw_its_native_tools() {
    // When
    let confinement = confinement_for("opencode");

    // Then
    assert_eq!(confinement, Confinement::WithdrawsNativeTools);
}

#[test]
fn codex_cannot_withdraw_its_native_tools() {
    // When
    let confinement = confinement_for("codex-acp");

    // Then
    assert_eq!(confinement, Confinement::CannotWithdraw);
}

#[test]
fn a_harness_nobody_proved_confinable_cannot_withdraw() {
    // When
    let confinement = confinement_for("some-new-harness");

    // Then
    assert_eq!(confinement, Confinement::CannotWithdraw);
}
