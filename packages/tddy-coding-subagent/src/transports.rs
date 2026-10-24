//! How each coding-agent harness is spoken to, and who answers its permission requests.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-headless-driver.md

/// The headless harnesses, run one CLI invocation per turn.
pub const HEADLESS_HARNESSES: &[&str] = &["claude", "cursor", "codex"];

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

/// The transport of `harness`; `None` for a harness this crate cannot run as a subagent.
pub fn transport_of(_harness: &str) -> Option<Transport> {
    // TODO(headless-driver): implement
    None
}

/// The permission policy `harness` runs under as a subagent.
pub fn permission_policy_of(_harness: &str) -> Option<PermissionPolicy> {
    // TODO(headless-driver): implement
    None
}
