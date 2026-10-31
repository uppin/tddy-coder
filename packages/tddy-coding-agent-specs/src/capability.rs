//! `coding_agent.capabilities`: what a session of the agent offers, declared rather than inferred
//! from its name (`#agent-tools 12/12`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`. Every gate that used to
//! read `session_type == "claude-cli"` asks the agent's spec instead —
//! [`crate::CodingAgentSpec::has`]. The set is closed: a capability is code that reads it, so a
//! word no code reads is refused when the spec loads, never ignored.

use serde::{Deserialize, Serialize};

/// One thing a session of the agent offers. Written in YAML as its snake_case word
/// (`ask_user_question`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// The agent runs per-worktree lifecycle hooks that report its status (`ReportSessionStatus`),
    /// and the daemon tails its conversation for activity.
    Hooks,
    /// The agent's questions reach the operator, and an orchestrator's answer reaches the agent.
    AskUserQuestion,
    /// The agent can run on this host against a codebase that lives elsewhere: a split session's
    /// `host_agent`, and a sandboxed-codebase placement.
    SplitHost,
    /// The agent can be a project's standing main (project-home) session.
    ProjectHomeMain,
    /// A start must name a model; the agent picks none of its own.
    RequiresModel,
    /// A `new_branch_from_base` intent must name its branch; none is generated.
    BranchNameRequired,
    /// Deleting a session leaves its worktree on disk.
    KeepWorktreeOnDelete,
    /// The session may notify its operator (`NotifyOperator`, the project-home notify tool).
    OperatorNotify,
}

impl Capability {
    /// Every capability, in declaration order.
    pub const ALL: [Capability; 8] = [
        Capability::Hooks,
        Capability::AskUserQuestion,
        Capability::SplitHost,
        Capability::ProjectHomeMain,
        Capability::RequiresModel,
        Capability::BranchNameRequired,
        Capability::KeepWorktreeOnDelete,
        Capability::OperatorNotify,
    ];

    /// The word a spec writes it as.
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Hooks => "hooks",
            Capability::AskUserQuestion => "ask_user_question",
            Capability::SplitHost => "split_host",
            Capability::ProjectHomeMain => "project_home_main",
            Capability::RequiresModel => "requires_model",
            Capability::BranchNameRequired => "branch_name_required",
            Capability::KeepWorktreeOnDelete => "keep_worktree_on_delete",
            Capability::OperatorNotify => "operator_notify",
        }
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
