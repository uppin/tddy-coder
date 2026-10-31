//! Whether an ACP agent can be confined, read from its spec's deny slot (`#agent-tools 11/12`):
//! `claude-acp` (`acp_meta`) and `opencode` (`config_env`) withdraw their native tools, `codex-acp`
//! (`unsupported`) cannot — the same answers `confinement_for` gives by harness name today — and a
//! managed session refuses an agent that cannot, naming its declaration.
//!
//! The specs are read straight from the embedded files (no registry, no host resolution).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-acp-agents-from-specs.md`.

use pretty_assertions::assert_eq;
use tddy_actions::tool::RunSection;
use tddy_coding_agent_specs::{
    CodingAgentSection, CodingAgentSpec, CLAUDE_ACP_SPEC_YAML, CODEX_ACP_SPEC_YAML,
    OPENCODE_SPEC_YAML,
};
use tddy_coding_subagent::{confinement_of_spec, refuse_unless_spec_confinable, Confinement};

/// The ACP agents that can be confined, as a launcher names them from its registry.
const CONFINABLE: &[&str] = &["claude-acp", "opencode"];

/// The coding-agent view of a built-in spec, read from its embedded YAML.
fn the_builtin_spec(id: &str, yaml: &str) -> CodingAgentSpec {
    let file: serde_yaml::Value = serde_yaml::from_str(yaml).expect("the spec is YAML");
    let run: RunSection = serde_yaml::from_value(file["run"].clone()).expect("its run section");
    let agent: CodingAgentSection =
        serde_yaml::from_value(file["extensions"]["coding_agent"].clone())
            .expect("its coding_agent section");
    CodingAgentSpec {
        id: id.to_string(),
        run,
        agent,
    }
}

#[test]
fn an_acp_spec_whose_deny_slot_is_meta_or_config_env_withdraws_and_one_declaring_unsupported_cannot(
) {
    // Given
    let claude_acp = the_builtin_spec("claude-acp", CLAUDE_ACP_SPEC_YAML);
    let opencode = the_builtin_spec("opencode", OPENCODE_SPEC_YAML);
    let codex_acp = the_builtin_spec("codex-acp", CODEX_ACP_SPEC_YAML);

    // When
    let confinements = (
        confinement_of_spec(&claude_acp),
        confinement_of_spec(&opencode),
        confinement_of_spec(&codex_acp),
    );

    // Then
    assert_eq!(
        confinements,
        (
            Confinement::WithdrawsNativeTools,
            Confinement::WithdrawsNativeTools,
            Confinement::CannotWithdraw,
        )
    );
}

#[test]
fn a_managed_session_refuses_codex_acp_naming_its_unsupported_deny_slot_and_admits_claude_acp() {
    // Given
    let codex_acp = the_builtin_spec("codex-acp", CODEX_ACP_SPEC_YAML);
    let claude_acp = the_builtin_spec("claude-acp", CLAUDE_ACP_SPEC_YAML);

    // When
    let answers = (
        refuse_unless_spec_confinable(&codex_acp, true, CONFINABLE),
        refuse_unless_spec_confinable(&claude_acp, true, CONFINABLE),
        refuse_unless_spec_confinable(&codex_acp, false, CONFINABLE),
    );

    // Then
    assert_eq!(
        answers,
        (
            Err(
                "the 'codex-acp' agent cannot withdraw its native file-system and shell tools \
                 (its spec declares tool_deny: unsupported), and a managed session's main agent \
                 has none, so its subagent must not either: attach a coding agent that can be \
                 confined (claude-acp, opencode) or a model agent"
                    .to_string()
            ),
            Ok(()),
            Ok(()),
        )
    );
}
