//! The `changeset.yaml` manifest itself: what a session records about its own run.
//!
//! Data and the accessors that read it. Nothing here touches the filesystem — [`super::io`]
//! does that, and [`super::merge`] derives a session's next move from what is stored here.

use super::stack::Stack;
use std::collections::BTreeMap;
use tddy_workflow::ids::WorkflowState;

/// One Q&A pair from planning clarification (question asked + user's answer).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClarificationQa {
    pub question: ClarificationQuestionForQa,
    pub answer: String,
}

/// Question structure for changeset storage (serializable).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClarificationQuestionForQa {
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub options: Vec<QuestionOptionForQa>,
    #[serde(default)]
    pub multi_select: bool,
}

/// Option for a clarification question (serializable).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuestionOptionForQa {
    pub label: String,
    #[serde(default)]
    pub description: String,
}

/// Changeset manifest stored in plan directory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Changeset {
    /// Human-readable feature title from the planning step (e.g. "Auth Feature").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Initial user prompt (goal/feature description from stdin).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_prompt: Option<String>,
    /// Questions asked during planning and user's answers (empty if no clarification).
    #[serde(default)]
    pub clarification_qa: Vec<ClarificationQa>,
    pub version: u32,
    pub models: BTreeMap<String, String>,
    pub sessions: Vec<SessionEntry>,
    pub state: ChangesetState,
    #[serde(default)]
    pub artifacts: BTreeMap<String, String>,
    pub discovery: Option<DiscoveryData>,
    /// Git worktree path for this session (e.g. .worktrees/feature-auth).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    /// Branch name for this session (set after worktree creation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Suggested branch name from plan agent (e.g. "feature/auth"). Used for worktree creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_suggestion: Option<String>,
    /// Suggested worktree directory name from plan agent (e.g. "feature-auth"). Used for worktree creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_suggestion: Option<String>,
    /// Whether changes were pushed to remote.
    #[serde(default)]
    pub remote_pushed: bool,
    /// Canonical absolute path to the code repository. Persisted for resume from any directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Active workflow recipe name (e.g. "tdd", "bugfix"). Omitted in legacy changesets → resolved using the same default as new sessions (**`free-prompting`**) at read time when no explicit recipe is supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub recipe: Option<String>,
    /// Demo routing and options for the TDD graph (merged into session Context at bootstrap / resume).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<ChangesetWorkflow>,
    /// Effective remote-tracking ref used to create the session worktree (default or resolved base).
    /// Persisted for observability and resume parity (chain PRs).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effective_worktree_integration_base_ref: Option<String>,
    /// User-selected chain-PR base ref (`origin/...`) when opted in; omitted when using default resolution only.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub worktree_integration_base_ref: Option<String>,
    /// PR-stack DAG; present only on an orchestrator session. Omitted for normal sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<Stack>,
    /// Back-reference from a CHILD session to its orchestrating session.
    /// Distinct from `previous_session_id` in `.session.yaml` (the base-branch source, which
    /// in a DAG may be a sibling node, not the orchestrator).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestrator_session_id: Option<String>,
}

/// A single session entry (plan, acceptance-tests, or impl).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionEntry {
    pub id: String,
    pub agent: String,
    pub tag: String,
    pub created_at: String,
    /// Path to system prompt file for this session (e.g. system-prompt-plan.md).
    #[serde(default)]
    pub system_prompt_file: Option<String>,
}

/// Workflow state persisted in changeset.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChangesetState {
    pub current: WorkflowState,
    /// Currently active agent session ID. Updated when a step starts or when SessionStarted is received.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub updated_at: String,
    #[serde(default)]
    pub history: Vec<StateTransition>,
}

/// State transition for history.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StateTransition {
    pub state: WorkflowState,
    pub at: String,
}

/// Discovery data from plan goal (toolchain, scripts, doc locations).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveryData {
    #[serde(default)]
    pub toolchain: BTreeMap<String, String>,
    #[serde(default)]
    pub scripts: BTreeMap<String, String>,
    #[serde(default)]
    pub doc_locations: Vec<String>,
    #[serde(default)]
    pub relevant_code: Vec<RelevantCode>,
    pub test_infrastructure: Option<TestInfrastructure>,
}

/// Relevant code path for discovery.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelevantCode {
    pub path: String,
    pub reason: String,
}

/// Test infrastructure info.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestInfrastructure {
    pub runner: String,
    pub conventions: String,
}

/// Branch vs worktree intent after base selection (persisted under `workflow` in `changeset.yaml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchWorktreeIntent {
    NewBranchFromBase,
    WorkOnSelectedBranch,
}

impl BranchWorktreeIntent {
    /// Stable string for [`tddy_graph::context::Context`] keys and RPC (matches serde `snake_case`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NewBranchFromBase => "new_branch_from_base",
            Self::WorkOnSelectedBranch => "work_on_selected_branch",
        }
    }
}

/// Workflow routing flags and demo options persisted in `changeset.yaml` (PRD: graph predicates, resume).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct ChangesetWorkflow {
    /// Canonical boolean for the post-green conditional edge (`run_optional_step_x` in Context / graph).
    #[serde(default)]
    pub run_optional_step_x: Option<bool>,
    #[serde(default)]
    pub demo_options: Vec<String>,
    /// Schema id for `tddy-tools` validation when writing this block (`goals.json` / JSON Schema).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_schema_id: Option<String>,
    /// Explicit branch/worktree mode for setup and post-green routing (PRD).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_worktree_intent: Option<BranchWorktreeIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_integration_base_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_branch_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_branch_to_work_on: Option<String>,
    /// Post-workflow: whether the operator opted into GitHub PR creation for the session branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_workflow_open_github_pr: Option<bool>,
    /// Populated only after a successful PR publish path when elicitation runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_workflow_remove_session_worktree: Option<bool>,
    /// Machine-readable PR automation lifecycle for resume and remote clients (`changeset.yaml` / Context).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_pr_status: Option<GithubPrStatus>,
    /// Operator answered "remove session worktree?" at post-workflow elicitation (`None` = not asked yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator_remove_session_worktree: Option<bool>,
}

/// Persisted GitHub PR automation status (phase, outcome URL, fatal error message).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct GithubPrStatus {
    pub phase: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Action-needed signal for a stack node, orthogonal to [`GithubPrStatus`].
///
/// `kind` is one of `up-to-date`, `needs-repoint`, `has-conflicts`, `ready-to-merge`,
/// `blocked`, `merged`. `source` is `derived` (auto-computed from git + GitHub) or `override`
/// (set by the agent — never clobbered by derivation while the override stands).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct PrInternalStatus {
    pub kind: String,
    #[serde(default)]
    pub note: Option<String>,
    pub source: String,
}

impl Changeset {
    /// Directory basename under `.worktrees/<basename>/` (matches [`crate::worktree`] conventions).
    ///
    /// When neither [`Changeset::worktree_suggestion`] nor [`Changeset::name`] is set, derives a
    /// stable folder name from [`ChangesetWorkflow::selected_branch_to_work_on`] or
    /// [`ChangesetWorkflow::new_branch_name`] so worktree setup can proceed (e.g. merge-pr after
    /// Telegram branch pick with only workflow fields populated).
    pub fn worktree_directory_basename(&self) -> Option<String> {
        self.worktree_suggestion
            .clone()
            .or_else(|| {
                self.name
                    .as_ref()
                    .map(|n| slugify_changeset_segment_for_worktree(n))
            })
            .or_else(|| {
                self.workflow.as_ref().and_then(|w| {
                    w.selected_branch_to_work_on
                        .as_ref()
                        .filter(|s| !s.trim().is_empty())
                        .map(|b| slugify_changeset_segment_for_worktree(b))
                })
            })
            .or_else(|| {
                self.workflow.as_ref().and_then(|w| {
                    w.new_branch_name
                        .as_ref()
                        .filter(|s| !s.trim().is_empty())
                        .map(|b| slugify_changeset_segment_for_worktree(b))
                })
            })
    }
}

fn slugify_changeset_segment_for_worktree(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

impl Default for Changeset {
    fn default() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            name: None,
            initial_prompt: None,
            clarification_qa: Vec::new(),
            version: 1,
            models: BTreeMap::new(),
            sessions: Vec::new(),
            state: ChangesetState {
                current: WorkflowState::new("Init"),
                session_id: None,
                updated_at: now.clone(),
                history: vec![StateTransition {
                    state: WorkflowState::new("Init"),
                    at: now,
                }],
            },
            artifacts: BTreeMap::new(),
            discovery: None,
            worktree: None,
            branch: None,
            branch_suggestion: None,
            worktree_suggestion: None,
            remote_pushed: false,
            repo_path: None,
            recipe: None,
            workflow: None,
            effective_worktree_integration_base_ref: None,
            worktree_integration_base_ref: None,
            stack: None,
            orchestrator_session_id: None,
        }
    }
}

#[cfg(test)]
mod worktree_directory_basename_tests {
    use super::*;

    #[test]
    fn derives_from_selected_branch_when_name_missing() {
        let cs = Changeset {
            workflow: Some(ChangesetWorkflow {
                selected_branch_to_work_on: Some(
                    "origin/feature/codex-oauth-web-relay".to_string(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            cs.worktree_directory_basename().as_deref(),
            Some("origin-feature-codex-oauth-web-relay")
        );
    }

    #[test]
    fn derives_from_new_branch_name_when_name_missing() {
        let cs = Changeset {
            workflow: Some(ChangesetWorkflow {
                new_branch_name: Some("feature/foo-bar".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            cs.worktree_directory_basename().as_deref(),
            Some("feature-foo-bar")
        );
    }

    #[test]
    fn prefers_worktree_suggestion_over_workflow() {
        let cs = Changeset {
            worktree_suggestion: Some("my-wt".to_string()),
            workflow: Some(ChangesetWorkflow {
                selected_branch_to_work_on: Some("origin/other".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(cs.worktree_directory_basename().as_deref(), Some("my-wt"));
    }
}
