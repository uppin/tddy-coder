//! How each coding-agent harness is spoken to, and who answers its permission requests.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use tddy_coding_agent_specs::{CodingAgentRegistry, CodingAgentSpec, HeadlessPermissionPolicy};

/// The transport a harness's turns take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Acp,
    Headless,
}

impl Transport {
    /// The catalog's spelling (`SubagentInfo.transport`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Acp => "acp",
            Self::Headless => "headless",
        }
    }
}

/// Who answers a harness's permission requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionPolicy {
    /// The request yields to the main agent (`permission_requested`).
    AsksCaller,
    /// The CLI cannot ask mid-turn; it runs under this fixed policy, shown on the row.
    Fixed(String),
}

impl PermissionPolicy {
    /// The catalog's spelling (`SubagentInfo.permission_policy`, `SessionAgentEntry.permission_policy`).
    pub fn as_wire(&self) -> String {
        match self {
            Self::AsksCaller => "asks_caller".to_string(),
            Self::Fixed(text) => format!("fixed: {text}"),
        }
    }
}

/// The transport of `harness` among the built-in agents' specs; `None` for a harness they do not
/// declare as a subagent. For a caller with no host registry ([`transport_of_spec`] is what a host's
/// registry is read through).
pub fn transport_of(harness: &str) -> Option<Transport> {
    CodingAgentRegistry::builtin_cached()
        .get(harness)
        .and_then(transport_of_spec)
}

/// The permission policy `harness` runs under among the built-in agents' specs; `None` exactly where
/// [`transport_of`] is `None`, and for a headless agent that declares none.
pub fn permission_policy_of(harness: &str) -> Option<PermissionPolicy> {
    CodingAgentRegistry::builtin_cached()
        .get(harness)
        .and_then(permission_policy_of_spec)
}

/// The `(transport, permission_policy)` wire spellings of `harness` among the built-in agents, both
/// empty for a harness that is no subagent. A host's catalog rows read its own registry
/// ([`transport_and_policy_wire_of_spec`]).
pub fn transport_and_policy_wire(harness: &str) -> (String, String) {
    CodingAgentRegistry::builtin_cached()
        .get(harness)
        .map(transport_and_policy_wire_of_spec)
        .unwrap_or_default()
}

/// The transport `spec`'s turns take, from its declaration (`#agent-tools 27/28`): ACP for an agent
/// with a `coding_agent.acp` section, headless for one with a `coding_agent.headless` section, `None`
/// for one with neither (a PTY-only agent), which this crate cannot run as a subagent.
pub fn transport_of_spec(spec: &CodingAgentSpec) -> Option<Transport> {
    if spec.acp().is_some() {
        Some(Transport::Acp)
    } else if spec.headless().is_some() {
        Some(Transport::Headless)
    } else {
        None
    }
}

/// The permission policy `spec` runs under as a subagent, from its declaration: an ACP agent asks
/// its caller; a headless one runs under its `coding_agent.headless.permission_policy`. `None`
/// exactly where [`transport_of_spec`] is `None`, and for a headless agent that declares no policy.
pub fn permission_policy_of_spec(spec: &CodingAgentSpec) -> Option<PermissionPolicy> {
    if spec.acp().is_some() {
        return Some(PermissionPolicy::AsksCaller);
    }
    match spec.headless()?.permission_policy.as_ref()? {
        HeadlessPermissionPolicy::AsksCaller => Some(PermissionPolicy::AsksCaller),
        HeadlessPermissionPolicy::Fixed(text) => Some(PermissionPolicy::Fixed(text.clone())),
    }
}

/// The `(transport, permission_policy)` wire spellings of `spec`, both empty for an agent this crate
/// cannot run as a subagent. What every coding-agent catalog row calls once it reads the registry.
pub fn transport_and_policy_wire_of_spec(spec: &CodingAgentSpec) -> (String, String) {
    match (transport_of_spec(spec), permission_policy_of_spec(spec)) {
        (Some(transport), Some(policy)) => (transport.as_str().to_string(), policy.as_wire()),
        _ => (String::new(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_policy_spells_its_wire_form() {
        assert_eq!(PermissionPolicy::AsksCaller.as_wire(), "asks_caller");
        assert_eq!(
            PermissionPolicy::Fixed("read-only".to_string()).as_wire(),
            "fixed: read-only"
        );
    }

    #[test]
    fn a_harness_outside_the_subagent_set_has_empty_wire_fields() {
        assert_eq!(
            transport_and_policy_wire("claude-cli"),
            (String::new(), String::new())
        );
        assert_eq!(
            transport_and_policy_wire(""),
            (String::new(), String::new())
        );
    }
}
