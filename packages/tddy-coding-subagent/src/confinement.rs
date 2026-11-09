//! Whether a coding agent can be run under a managed session's confinement.
//!
//! In a managed-codebase session the main agent has no native file-system or shell tools; a
//! subagent of that session must not have them either. Whether an agent can have them withdrawn,
//! and how, is its spec's to declare (`coding_agent.acp.tool_deny` and `acp.withdraw_native`); an
//! agent whose spec cannot prove it withdrawable is refused in managed sessions — never launched
//! unconfined.

use tddy_coding_agent_specs::{CodingAgentRegistry, CodingAgentSpec, ToolSlot};

/// What a harness can do about its native tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confinement {
    /// Its native file-system and shell tools can be withdrawn, leaving only MCP tools.
    WithdrawsNativeTools,
    /// It cannot have them withdrawn; refused in managed sessions.
    CannotWithdraw,
}

/// The confinement an ACP launch of `spec` supports, from its ACP deny slot
/// ([`CodingAgentSpec::acp_tool_deny`], `#agent-tools 11/12`): `acp_meta` and `config_env` withdraw
/// the declared `acp.withdraw_native` — and only when that list names something to withdraw.
/// `unsupported` — and a flag, which an ACP agent cannot take, or a spec with no `acp` section — is
/// [`Confinement::CannotWithdraw`].
pub fn confinement_of_spec(spec: &CodingAgentSpec) -> Confinement {
    let withdraws = spec
        .acp()
        .is_some_and(|acp| !acp.withdraw_native.is_empty());
    match spec.acp_tool_deny() {
        Some(ToolSlot::AcpMeta { .. } | ToolSlot::ConfigEnv { .. }) if withdraws => {
            Confinement::WithdrawsNativeTools
        }
        _ => Confinement::CannotWithdraw,
    }
}

/// Refuse an ACP agent declared as `spec` for a managed session
/// when [`confinement_of_spec`] is [`Confinement::CannotWithdraw`]. `confinable` names the agents
/// that can be confined (the registry's ACP agents whose spec withdraws), in the order given. `Err`
/// is the reason, naming the spec's declaration:
///
/// `the 'codex-acp' agent cannot withdraw its native file-system and shell tools (its spec declares
/// tool_deny: unsupported), and a managed session's main agent has none, so its subagent must not
/// either: attach a coding agent that can be confined (claude-acp, opencode) or a model agent`
pub fn refuse_unless_spec_confinable(
    spec: &CodingAgentSpec,
    managed: bool,
    confinable: &[&str],
) -> Result<(), String> {
    if !managed || confinement_of_spec(spec) == Confinement::WithdrawsNativeTools {
        return Ok(());
    }
    Err(format!(
        "the '{}' agent cannot withdraw its native file-system and shell tools (its spec declares \
         {}), and a managed session's main agent has none, so its subagent must not either: attach \
         a coding agent that can be confined ({}) or a model agent",
        spec.id,
        declaration(spec),
        confinable.join(", ")
    ))
}

/// How `spec` declares its withdrawal, for the refusal to name: its deny slot, or the lack of one.
fn declaration(spec: &CodingAgentSpec) -> String {
    match spec.acp_tool_deny() {
        None => "no acp section".to_string(),
        Some(ToolSlot::Unsupported(_)) => "tool_deny: unsupported".to_string(),
        Some(ToolSlot::Flag(flag)) => format!("tool_deny: the flag {}", flag.flag),
        Some(ToolSlot::AcpMeta { .. } | ToolSlot::ConfigEnv { .. }) => {
            "an empty acp.withdraw_native".to_string()
        }
    }
}

/// The ACP agents of `registry` whose spec can withdraw their native tools ([`confinement_of_spec`]),
/// ordered by id: the ones a refusal tells the caller to attach instead.
pub fn confinable_acp_agents(registry: &CodingAgentRegistry) -> Vec<String> {
    registry
        .all()
        .into_iter()
        .filter(|spec| {
            spec.acp().is_some() && confinement_of_spec(spec) == Confinement::WithdrawsNativeTools
        })
        .map(|spec| spec.id.clone())
        .collect()
}

/// Refuse `harness` for a managed session when its spec in `registry` cannot withdraw its native
/// tools (`#agent-tools 27/28`): the session's main agent has none, so a subagent must not either.
/// An ACP agent is refused by [`refuse_unless_spec_confinable`], naming the registry's confinable ACP
/// agents; a headless agent — whatever flag its spec declares for its own tools, which is not an ACP
/// withdrawal — and a harness the registry does not declare are refused too. `Err` is the reason;
/// the caller says which agent and which session it refused.
pub fn refuse_unless_registry_confinable(
    registry: &CodingAgentRegistry,
    harness: &str,
    managed: bool,
) -> Result<(), String> {
    if !managed {
        return Ok(());
    }
    let confinable = confinable_acp_agents(registry);
    let confinable: Vec<&str> = confinable.iter().map(String::as_str).collect();
    match registry.get(harness) {
        Some(spec) => refuse_unless_spec_confinable(spec, managed, &confinable),
        None => Err(format!(
            "the '{harness}' harness is not a coding agent this host declares, so its native \
             file-system and shell tools cannot be shown to be withdrawn, and a managed session's \
             main agent has none, so its subagent must not either: attach a coding agent that can \
             be confined ({}) or a model agent",
            confinable.join(", ")
        )),
    }
}
