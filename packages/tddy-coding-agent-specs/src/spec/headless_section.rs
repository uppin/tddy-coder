use crate::headless::{
    FreshSession, GoalParams, HeadlessProtocol, RouteModel, ScratchDir, SystemPromptDelivery,
};
use crate::spec::slot_shapes::McpSlot;
use crate::spec::CodingAgentSpec;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tddy_actions::tool::{ArgvItem, RunSection, Transport};

/// `coding_agent.headless`: one headless turn's argv template, its static env, the adapter its
/// output is read through, and whether a non-zero exit with a structured result is a success.
///
/// The argv is rendered with `tddy_actions::tool::render_argv` over [`Self::run_section`], the
/// slots spliced in as `slots` — the same grammar and slots as the PTY `run:` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadlessSection {
    pub argv: Vec<ArgvItem>,
    /// Set on the process on top of what the launcher passes. Values are literal.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// The stream adapter (`tddy_agent_backend::adapters::AdapterRegistry`) that reads the
    /// agent's stdout: `claude-stream-json`. A name the registry does not hold is refused at load.
    pub adapter: String,
    /// A non-zero exit whose result carries a structured response counts as success. A property of
    /// the agent, not of the goal: it replaces
    /// `GoalHints::claude_nonzero_exit_ok_if_structured_response`.
    #[serde(default)]
    pub exit_ok_if_structured: bool,
    /// Where a headless run registers its MCP server, when that differs from the agent's
    /// `coding_agent.mcp` (Cursor: a project file in the worktree headless, `{home}` for its PTY
    /// session). `None` uses `coding_agent.mcp`. Added by the 2026-10-31 amendment.
    #[serde(default)]
    pub mcp: Option<McpSlot>,
    // --- The workflow turn's protocol (`#agent-tools 15/19`): what a workflow turn of the agent
    // needs beyond its argv, read by `SpecHeadlessBackend` through [`Self::protocol`]. Each is named
    // after what it does, never after an agent; an unknown word is refused at load.
    /// How the workflow's system prompt reaches the agent: `flag` (the `append_system_prompt` slot)
    /// or `prepend` (in front of the prompt). `prepend` when absent.
    #[serde(default)]
    pub system_prompt: SystemPromptDelivery,
    /// The template params a goal's hints bind: `always`, `plan_mode`, `read_only_plan`,
    /// `accept_edits`, each a map of param name to value.
    #[serde(default)]
    pub goal_params: GoalParams,
    /// The workflow binds the approval tool (`mcp__tddy-tools__approval_prompt`) when it registers
    /// the tool server.
    #[serde(default)]
    pub approval_tool: bool,
    /// The workflow registers the agent's tool server (its `tddy-tools` dep with `--mcp`).
    #[serde(default)]
    pub workflow_mcp: bool,
    /// Whether a provider route's argv replaces the request's model: `{ kind: kept }`,
    /// `{ kind: replaced }` or `{ kind: replaced_if_route_has, flag: <flag> }`. `kept` when absent.
    #[serde(default)]
    pub route_model: RouteModel,
    /// Who names a fresh session's id: `caller` (passed as `session_id`) or `agent`. `agent` when
    /// absent.
    #[serde(default)]
    pub fresh_session: FreshSession,
    /// Where the turn's registration files go: `session_dir` or `workdir`. `session_dir` when
    /// absent.
    #[serde(default)]
    pub scratch: ScratchDir,
    /// Who answers a headless conversation's permission requests (`#agent-tools 27/28`):
    /// `asks_caller`, or `{ fixed: <text> }` for a CLI that cannot ask mid-turn and runs under a
    /// fixed policy, the text shown on its catalog row. `None` when the spec declares none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<HeadlessPermissionPolicy>,
}

/// `coding_agent.headless.permission_policy`: who answers a headless conversation's permission
/// requests. Spelled `asks_caller` or `{ fixed: <text> }` (an untagged YAML shape, since YAML enums
/// are otherwise tagged).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PermissionPolicyYaml", into = "PermissionPolicyYaml")]
pub enum HeadlessPermissionPolicy {
    /// The request yields to the caller (the main agent).
    AsksCaller,
    /// The CLI cannot ask mid-turn; it runs under this policy, described for the catalog row.
    Fixed(String),
}

/// [`HeadlessPermissionPolicy`] as its YAML is written.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum PermissionPolicyYaml {
    Word(AsksCallerWord),
    Fixed { fixed: String },
}

/// The one bare word `permission_policy` takes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AsksCallerWord {
    AsksCaller,
}

impl From<PermissionPolicyYaml> for HeadlessPermissionPolicy {
    fn from(yaml: PermissionPolicyYaml) -> Self {
        match yaml {
            PermissionPolicyYaml::Word(AsksCallerWord::AsksCaller) => Self::AsksCaller,
            PermissionPolicyYaml::Fixed { fixed } => Self::Fixed(fixed),
        }
    }
}

impl From<HeadlessPermissionPolicy> for PermissionPolicyYaml {
    fn from(policy: HeadlessPermissionPolicy) -> Self {
        match policy {
            HeadlessPermissionPolicy::AsksCaller => Self::Word(AsksCallerWord::AsksCaller),
            HeadlessPermissionPolicy::Fixed(fixed) => Self::Fixed { fixed },
        }
    }
}

impl HeadlessSection {
    /// The protocol a workflow turn of this agent runs with: this section's protocol fields.
    pub fn protocol(&self) -> HeadlessProtocol {
        HeadlessProtocol {
            system_prompt: self.system_prompt,
            goal_params: self.goal_params.clone(),
            approval_tool: self.approval_tool,
            workflow_mcp: self.workflow_mcp,
            route_model: self.route_model.clone(),
            fresh_session: self.fresh_session,
            scratch: self.scratch,
        }
    }

    /// This section as a `stdio` run section, for `render_argv`.
    pub fn run_section(&self) -> RunSection {
        RunSection {
            transport: Transport::Stdio,
            argv: self.argv.clone(),
            env: self.env.clone(),
        }
    }
}

impl CodingAgentSpec {
    /// The agent's headless declaration, if it has one.
    pub fn headless(&self) -> Option<&HeadlessSection> {
        self.agent.headless.as_ref()
    }
}
