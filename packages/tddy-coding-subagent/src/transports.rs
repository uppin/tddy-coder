//! How each coding-agent harness is spoken to, and who answers its permission requests.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use crate::catalog::ACP_HARNESSES;

/// The headless harnesses, run one CLI invocation per turn.
pub const HEADLESS_HARNESSES: &[&str] = &["claude", "cursor", "codex", "oz"];

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

/// The transport of `harness`; `None` for a harness this crate cannot run as a subagent.
pub fn transport_of(harness: &str) -> Option<Transport> {
    if ACP_HARNESSES.contains(&harness) {
        Some(Transport::Acp)
    } else if HEADLESS_HARNESSES.contains(&harness) {
        Some(Transport::Headless)
    } else {
        None
    }
}

/// The permission policy `harness` runs under as a subagent; `None` exactly where [`transport_of`]
/// is `None`.
pub fn permission_policy_of(harness: &str) -> Option<PermissionPolicy> {
    if ACP_HARNESSES.contains(&harness) {
        return Some(PermissionPolicy::AsksCaller);
    }
    match harness {
        "claude" => Some(PermissionPolicy::AsksCaller),
        "cursor" => Some(PermissionPolicy::Fixed(
            "read-only; the CLI cannot ask mid-turn".to_string(),
        )),
        "codex" => Some(PermissionPolicy::Fixed(
            "workspace-write sandbox; the CLI cannot ask mid-turn".to_string(),
        )),
        "oz" => Some(PermissionPolicy::Fixed(
            "the agent profile decides; its default profile auto-approves commands and edits"
                .to_string(),
        )),
        _ => None,
    }
}

/// The `(transport, permission_policy)` wire spellings of `harness`, both empty for a harness this
/// crate cannot run as a subagent. Every coding-agent catalog row calls this, so they cannot disagree.
pub fn transport_and_policy_wire(harness: &str) -> (String, String) {
    match (transport_of(harness), permission_policy_of(harness)) {
        (Some(transport), Some(policy)) => (transport.as_str().to_string(), policy.as_wire()),
        _ => (String::new(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_harness_with_a_transport_has_a_permission_policy() {
        for harness in ACP_HARNESSES.iter().chain(HEADLESS_HARNESSES) {
            assert!(transport_of(harness).is_some(), "{harness}");
            assert!(permission_policy_of(harness).is_some(), "{harness}");
        }
    }

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
