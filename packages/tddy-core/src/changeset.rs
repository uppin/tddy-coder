//! Changeset manifest — unified workflow state, sessions, and model configuration.
//!
//! Replaces `.session` and `.impl-session` with a single `changeset.yaml` file.

use crate::backend::ClarificationQuestion;
use crate::error::WorkflowError;
use crate::workflow::context::Context;
use crate::workflow::ids::{GoalId, WorkflowState};
use crate::workflow::recipe::WorkflowRecipe;
use log::{debug, info};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

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

/// PR-stack DAG carried by the ORCHESTRATOR session's changeset.
/// Each node is a child PR session reference.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stack {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub nodes: Vec<StackNode>,
}

/// A single node in the PR-stack DAG.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StackNode {
    /// Stable planner-assigned id (e.g. "n1"). Exists before a child session is materialized.
    pub node_id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Suggested git branch name (e.g. "feature/auth-token-store").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_suggestion: Option<String>,
    /// Actual branch once the child worktree is created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Child session id once materialized; None while only planned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Parent NODE ids (not session ids). Empty = root off stack base; >1 = DAG.
    #[serde(default)]
    pub parents: Vec<String>,
    /// Reuses existing GithubPrStatus; phase values: "planned"|"open"|"merged"|"closed"|"error".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_status: Option<GithubPrStatus>,
    /// Coarse mirror of child session WorkflowState for orchestrator dashboards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_state: Option<WorkflowState>,
}

impl StackNode {
    /// A node is "skipped" when its PR has been merged.
    pub fn is_skipped(&self) -> bool {
        self.pr_status
            .as_ref()
            .map(|s| s.phase == "merged")
            .unwrap_or(false)
    }
}

impl Stack {
    /// Find a node by node_id.
    pub fn node(&self, node_id: &str) -> Option<&StackNode> {
        self.nodes.iter().find(|n| n.node_id == node_id)
    }

    /// Kahn topological sort over node_id/parents relationships.
    /// Returns ordered list of node_ids (leaves last). Cycle → WorkflowError::ChangesetInvalid.
    pub fn topo_order(&self) -> Result<Vec<String>, WorkflowError> {
        use std::collections::VecDeque;

        // In-degree = number of parents that exist as nodes in this stack.
        let known: std::collections::HashSet<&str> =
            self.nodes.iter().map(|n| n.node_id.as_str()).collect();
        let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
        // children[parent] = nodes that depend on `parent`.
        let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for node in &self.nodes {
            in_degree.entry(node.node_id.as_str()).or_insert(0);
            for parent in &node.parents {
                if known.contains(parent.as_str()) {
                    *in_degree.entry(node.node_id.as_str()).or_insert(0) += 1;
                    children
                        .entry(parent.as_str())
                        .or_default()
                        .push(node.node_id.as_str());
                }
            }
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&id, _)| id)
            .collect();
        let mut order: Vec<String> = Vec::with_capacity(self.nodes.len());
        while let Some(id) = queue.pop_front() {
            order.push(id.to_string());
            if let Some(deps) = children.get(id) {
                for &child in deps {
                    let d = in_degree.get_mut(child).expect("child has in-degree entry");
                    *d -= 1;
                    if *d == 0 {
                        queue.push_back(child);
                    }
                }
            }
        }

        if order.len() != self.nodes.len() {
            return Err(WorkflowError::ChangesetInvalid(format!(
                "cycle detected in PR-stack DAG: only {} of {} nodes orderable",
                order.len(),
                self.nodes.len()
            )));
        }
        Ok(order)
    }

    /// Effective base origin refs for a node, skipping merged ancestors.
    /// Returns `origin/<branch>` for each nearest non-skipped ancestor across all `parents`,
    /// or `[stack_bottom_base.to_string()]` when all parents are merged/absent.
    pub fn effective_base_refs(&self, node_id: &str, stack_bottom_base: &str) -> Vec<String> {
        let Some(node) = self.node(node_id) else {
            return vec![stack_bottom_base.to_string()];
        };
        let refs: Vec<String> = node
            .parents
            .iter()
            .filter_map(|parent_id| self.node(parent_id))
            .filter(|parent| !parent.is_skipped())
            .map(|parent| {
                format!(
                    "origin/{}",
                    parent.branch.as_deref().unwrap_or(parent.node_id.as_str())
                )
            })
            .collect();
        if refs.is_empty() {
            vec![stack_bottom_base.to_string()]
        } else {
            refs
        }
    }
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
    /// Stable string for [`Context`] keys and RPC (matches serde `snake_case`).
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

/// Read changeset from plan directory.
pub fn read_changeset(session_dir: &Path) -> Result<Changeset, WorkflowError> {
    let path = session_dir.join("changeset.yaml");
    let content = fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            WorkflowError::ChangesetMissing(format!(
                "changeset.yaml not found in {} — run the plan goal first",
                session_dir.display()
            ))
        } else {
            WorkflowError::ChangesetInvalid(e.to_string())
        }
    })?;
    serde_yaml::from_str(&content).map_err(|e| WorkflowError::ChangesetInvalid(e.to_string()))
}

/// Write changeset to plan directory.
pub fn write_changeset(session_dir: &Path, changeset: &Changeset) -> Result<(), WorkflowError> {
    let path = session_dir.join("changeset.yaml");
    let content =
        serde_yaml::to_string(changeset).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    fs::write(&path, content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Ensures the session's changeset carries `recipe_name` — creates changeset.yaml with this
/// recipe if none exists yet, fills in `recipe` if the existing changeset has it unset, and
/// leaves an already-set (possibly different) recipe untouched.
pub fn ensure_changeset_recipe(session_dir: &Path, recipe_name: &str) -> Result<(), WorkflowError> {
    let mut cs = match read_changeset(session_dir) {
        Ok(cs) => cs,
        Err(WorkflowError::ChangesetMissing(_)) => Changeset::default(),
        Err(e) => return Err(e),
    };
    if cs.recipe.is_some() {
        return Ok(());
    }
    cs.recipe = Some(recipe_name.to_string());
    write_changeset(session_dir, &cs)
}

/// Atomically replace `changeset.yaml` (write temp + rename) so readers never see a partial file.
pub fn write_changeset_atomic(
    session_dir: &Path,
    changeset: &Changeset,
) -> Result<(), WorkflowError> {
    info!(
        target: "tddy_core::changeset",
        "write_changeset_atomic: session_dir={}",
        session_dir.display()
    );
    let path = session_dir.join("changeset.yaml");
    let tmp = session_dir.join(".changeset.yaml.tmp");
    let content =
        serde_yaml::to_string(changeset).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    fs::write(&tmp, &content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    // Windows: rename cannot replace an existing target.
    #[cfg(windows)]
    {
        let _ = fs::remove_file(&path);
    }
    fs::rename(&tmp, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        WorkflowError::WriteFailed(e.to_string())
    })?;
    Ok(())
}

/// Merges persisted workflow/demo fields from `changeset.yaml` into session [`Context`]
/// (`run_optional_step_x`, demo options) so graph predicates match stored intent after interview/plan.
///
/// Legacy changesets without a `workflow` block leave context unchanged for these keys; the graph
/// then uses the same defaults as an empty context (e.g. post-green routing per existing graph rules).
pub fn merge_persisted_workflow_into_context(
    session_dir: &Path,
    context: &Context,
) -> Result<(), WorkflowError> {
    info!(
        target: "tddy_core::changeset",
        "merge_persisted_workflow_into_context: session_dir={}",
        session_dir.display()
    );
    let cs = read_changeset(session_dir)?;
    let Some(ref wf) = cs.workflow else {
        debug!(
            target: "tddy_core::changeset",
            "merge_persisted_workflow_into_context: no workflow block in changeset — skipping"
        );
        return Ok(());
    };
    if let Some(b) = wf.run_optional_step_x {
        debug!(
            target: "tddy_core::changeset",
            "merge_persisted_workflow_into_context: set run_optional_step_x={}",
            b
        );
        context.set_sync("run_optional_step_x", b);
    }
    if !wf.demo_options.is_empty() {
        info!(
            target: "tddy_core::changeset",
            "merge_persisted_workflow_into_context: set demo_options count={}",
            wf.demo_options.len()
        );
        context.set_sync("demo_options", wf.demo_options.clone());
    }
    if let Some(ref id) = wf.tool_schema_id {
        debug!(
            target: "tddy_core::changeset",
            "merge_persisted_workflow_into_context: tool_schema_id present (len={})",
            id.len()
        );
    }
    crate::branch_worktree_intent::merge_branch_worktree_intent_into_context(wf, context);
    merge_post_workflow_into_context(wf, context)?;
    Ok(())
}

/// Copies post-workflow PR / worktree elicitation fields from persisted [`ChangesetWorkflow`] into
/// [`Context`] so presenters, CLI, and graph predicates see the same durable state as `changeset.yaml`.
fn merge_post_workflow_into_context(
    wf: &ChangesetWorkflow,
    context: &Context,
) -> Result<(), WorkflowError> {
    if let Some(flag) = wf.post_workflow_open_github_pr {
        debug!(
            target: "tddy_core::changeset",
            "merge_post_workflow_into_context: post_workflow_open_github_pr={flag}"
        );
        context.set_sync("post_workflow_open_github_pr", flag);
    }
    if let Some(flag) = wf.post_workflow_remove_session_worktree {
        debug!(
            target: "tddy_core::changeset",
            "merge_post_workflow_into_context: post_workflow_remove_session_worktree={flag}"
        );
        context.set_sync("post_workflow_remove_session_worktree", flag);
    }
    if let Some(flag) = wf.operator_remove_session_worktree {
        debug!(
            target: "tddy_core::changeset",
            "merge_post_workflow_into_context: operator_remove_session_worktree={flag}"
        );
        context.set_sync("operator_remove_session_worktree", flag);
    }
    let Some(status) = &wf.github_pr_status else {
        return Ok(());
    };
    let v = serde_json::to_value(status).map_err(|e| {
        WorkflowError::ChangesetInvalid(format!(
            "workflow.github_pr_status is not serializable for Context merge: {e}"
        ))
    })?;
    info!(
        target: "tddy_core::changeset",
        "merge_post_workflow_into_context: github_pr_status phase={}",
        status.phase
    );
    context.set_sync("github_pr_status", v);
    Ok(())
}

/// Which workflow goal to run when continuing from an on-disk session (CLI resume, presenter).
///
/// For a normal persisted state, this is [`WorkflowRecipe::next_goal_for_state`]. When the session
/// should be treated as **failed resume** ([`ChangesetState::current`] is `Failed`, or the last
/// history entry is `Failed`), `next_goal_for_state(Failed)` is `None`; in that case we walk
/// [`ChangesetState::history`] from newest to oldest, skipping `Failed`, and use the first
/// transition whose [`WorkflowRecipe::next_goal_for_state`] is `Some` and **not** equal to
/// [`WorkflowRecipe::start_goal`].
///
/// Skipping transitions per [`WorkflowRecipe::skip_failed_resume_transition`] avoids a common bad
/// ordering: a full workflow restart writes `Planning` immediately before `Failed`, while an earlier
/// transition (e.g. `GreenImplementing`) still reflects the real phase to retry. If every candidate
/// is skipped or `None`, falls back to [`WorkflowRecipe::start_goal`], with a TDD-specific fallback
/// to **`plan`** when only trailing `Planning` → `plan` entries were skipped (failed during plan).
///
/// When the **last** history entry is `Failed`, the same walk is used even if [`ChangesetState::current`]
/// was left stale (e.g. still `Planning` after a manual `changeset.yaml` edit that fixed `history`
/// but not `current`). Otherwise `next_goal_for_state(current)` would incorrectly return `plan`.
pub fn start_goal_for_session_continue(recipe: &dyn WorkflowRecipe, cs: &Changeset) -> GoalId {
    let start = recipe.start_goal();
    let history_ends_in_failed = cs
        .state
        .history
        .last()
        .is_some_and(|t| t.state.as_str() == "Failed");
    let use_failed_resume_walk = cs.state.current.as_str() == "Failed" || history_ends_in_failed;
    if !use_failed_resume_walk {
        return recipe
            .next_goal_for_state_with_changeset(&cs.state.current, cs)
            .unwrap_or_else(|| start.clone());
    }
    let mut tdd_skipped_trailing_planning = false;
    for transition in cs.state.history.iter().rev() {
        if transition.state.as_str() == "Failed" {
            continue;
        }
        match recipe.next_goal_for_state_with_changeset(&transition.state, cs) {
            None => continue,
            Some(g) if recipe.skip_failed_resume_transition(&transition.state, &g) => {
                if recipe.name() == "tdd"
                    && transition.state.as_str() == "Planning"
                    && g.as_str() == "plan"
                {
                    tdd_skipped_trailing_planning = true;
                }
                continue;
            }
            Some(g) => return g,
        }
    }
    if recipe.name() == "tdd" && tdd_skipped_trailing_planning {
        return GoalId::new("plan");
    }
    start
}

/// Resolve model for a goal: CLI override > changeset `models` > optional recipe defaults.
pub fn resolve_model(
    changeset: Option<&Changeset>,
    goal: &str,
    cli_model: Option<&str>,
    recipe_defaults: Option<&BTreeMap<String, String>>,
) -> Option<String> {
    if let Some(m) = cli_model {
        return Some(m.to_string());
    }
    if let Some(c) = changeset {
        if let Some(m) = c.models.get(goal) {
            return Some(m.clone());
        }
    }
    recipe_defaults.and_then(|d| d.get(goal).cloned())
}

/// Get session ID for a tag (e.g. "plan" or "impl").
pub fn get_session_for_tag(changeset: &Changeset, tag: &str) -> Option<String> {
    changeset
        .sessions
        .iter()
        .rfind(|s| s.tag == tag)
        .map(|s| s.id.clone())
}

/// Backend `agent` from the latest session with `tag == preferred_tag`, else the last session entry.
pub fn resolve_agent_from_changeset(changeset: &Changeset, preferred_tag: &str) -> Option<String> {
    changeset
        .sessions
        .iter()
        .rfind(|s| s.tag == preferred_tag)
        .map(|s| s.agent.clone())
        .or_else(|| changeset.sessions.last().map(|s| s.agent.clone()))
}

/// Update workflow state only (no new session).
pub fn update_state(changeset: &mut Changeset, new_state: WorkflowState) {
    let now = chrono::Utc::now().to_rfc3339();
    changeset.state.history.push(StateTransition {
        state: new_state.clone(),
        at: now.clone(),
    });
    changeset.state.current = new_state;
    changeset.state.updated_at = now;
}

/// Build clarification_qa from backend questions and newline-separated answers.
pub fn clarification_qa_from_backend(
    questions: Vec<ClarificationQuestion>,
    answers: &str,
) -> Vec<ClarificationQa> {
    let answer_lines: Vec<String> = answers.split('\n').map(|s| s.trim().to_string()).collect();
    questions
        .into_iter()
        .enumerate()
        .map(|(i, q)| {
            let answer = answer_lines.get(i).cloned().unwrap_or_default();
            ClarificationQa {
                question: ClarificationQuestionForQa {
                    header: q.header,
                    question: q.question,
                    options: q
                        .options
                        .into_iter()
                        .map(|o| QuestionOptionForQa {
                            label: o.label,
                            description: o.description,
                        })
                        .collect(),
                    multi_select: q.multi_select,
                },
                answer,
            }
        })
        .collect()
}

/// Atomically update the stack on an orchestrator session.
pub fn update_stack_atomic(
    orchestrator_session_dir: &Path,
    f: impl FnOnce(&mut Stack),
) -> Result<(), WorkflowError> {
    let mut changeset = read_changeset(orchestrator_session_dir)?;
    let stack = changeset.stack.get_or_insert_with(Stack::default);
    f(stack);
    write_changeset_atomic(orchestrator_session_dir, &changeset)
}

/// Link a stack node to its materialized child session (set session_id + branch).
pub fn link_stack_node_to_child_session(
    orchestrator_session_dir: &Path,
    node_id: &str,
    child_session_id: &str,
    branch: Option<String>,
) -> Result<(), WorkflowError> {
    update_stack_atomic(orchestrator_session_dir, |stack| {
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            node.session_id = Some(child_session_id.to_string());
            if let Some(b) = branch {
                node.branch = Some(b);
            }
        }
    })
}

/// Sync a stack node's child_state + pr_status from the child session's changeset.
/// Reads child via `unified_session_dir_path(sessions_root, node.session_id)` + `read_changeset`.
pub fn sync_stack_node_from_child(
    orchestrator_session_dir: &Path,
    sessions_root: &Path,
    node_id: &str,
) -> Result<(), WorkflowError> {
    // Resolve the child session id from the current orchestrator stack before any write.
    let orch = read_changeset(orchestrator_session_dir)?;
    let session_id = orch
        .stack
        .as_ref()
        .and_then(|s| s.node(node_id))
        .and_then(|n| n.session_id.clone());
    let Some(session_id) = session_id else {
        return Ok(());
    };

    let child_dir = crate::session_lifecycle::unified_session_dir_path(sessions_root, &session_id);
    let child = read_changeset(&child_dir)?;
    let child_state = child.state.current.clone();
    let pr_status = child
        .workflow
        .as_ref()
        .and_then(|w| w.github_pr_status.clone());

    update_stack_atomic(orchestrator_session_dir, |stack| {
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            node.child_state = Some(child_state);
            node.pr_status = pr_status;
        }
    })
}

/// Append a session and update state.
pub fn append_session_and_update_state(
    changeset: &mut Changeset,
    session_id: String,
    tag: &str,
    new_state: WorkflowState,
    agent: &str,
    system_prompt_file: Option<String>,
) {
    let now = chrono::Utc::now().to_rfc3339();
    changeset.sessions.push(SessionEntry {
        id: session_id.clone(),
        agent: agent.to_string(),
        tag: tag.to_string(),
        created_at: now.clone(),
        system_prompt_file,
    });
    changeset.state.session_id = Some(session_id);
    changeset.state.history.push(StateTransition {
        state: new_state.clone(),
        at: now.clone(),
    });
    changeset.state.current = new_state;
    changeset.state.updated_at = now;
}

#[cfg(test)]
mod resolve_agent_tests {
    use super::*;

    #[test]
    fn resolve_agent_prefers_plan_tag_session() {
        let mut cs = Changeset::default();
        append_session_and_update_state(
            &mut cs,
            "a".into(),
            "plan",
            WorkflowState::new("Planned"),
            "cursor",
            None,
        );
        append_session_and_update_state(
            &mut cs,
            "b".into(),
            "acceptance-tests",
            WorkflowState::new("AcceptanceTestsReady"),
            "claude",
            None,
        );
        assert_eq!(
            resolve_agent_from_changeset(&cs, "plan").as_deref(),
            Some("cursor")
        );
    }

    #[test]
    fn resolve_agent_falls_back_to_last_session() {
        let mut cs = Changeset::default();
        append_session_and_update_state(
            &mut cs,
            "x".into(),
            "acceptance-tests",
            WorkflowState::new("AcceptanceTestsReady"),
            "stub",
            None,
        );
        assert_eq!(
            resolve_agent_from_changeset(&cs, "plan").as_deref(),
            Some("stub")
        );
    }
}

#[cfg(test)]
mod stack_tests {
    use super::*;

    #[test]
    fn effective_base_refs_single_parent_skip() {
        let stack = Stack {
            version: 1,
            nodes: vec![
                StackNode {
                    node_id: "n1".to_string(),
                    title: "Node 1".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n1".to_string()),
                    session_id: Some("sess-1".to_string()),
                    parents: vec![],
                    pr_status: Some(GithubPrStatus {
                        phase: "merged".to_string(),
                        url: None,
                        error: None,
                    }),
                    child_state: None,
                },
                StackNode {
                    node_id: "n2".to_string(),
                    title: "Node 2".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n2".to_string()),
                    session_id: None,
                    parents: vec!["n1".to_string()],
                    pr_status: None,
                    child_state: None,
                },
            ],
        };
        let refs = stack.effective_base_refs("n2", "origin/master");
        assert_eq!(refs, vec!["origin/master".to_string()]);
    }

    #[test]
    fn effective_base_refs_multi_parent_unmerged_set() {
        let stack = Stack {
            version: 1,
            nodes: vec![
                StackNode {
                    node_id: "n1".to_string(),
                    title: "Node 1".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n1".to_string()),
                    session_id: Some("sess-1".to_string()),
                    parents: vec![],
                    pr_status: Some(GithubPrStatus {
                        phase: "open".to_string(),
                        url: None,
                        error: None,
                    }),
                    child_state: None,
                },
                StackNode {
                    node_id: "n2".to_string(),
                    title: "Node 2".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n2".to_string()),
                    session_id: Some("sess-2".to_string()),
                    parents: vec![],
                    pr_status: Some(GithubPrStatus {
                        phase: "open".to_string(),
                        url: None,
                        error: None,
                    }),
                    child_state: None,
                },
                StackNode {
                    node_id: "n3".to_string(),
                    title: "Node 3".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n3".to_string()),
                    session_id: None,
                    parents: vec!["n1".to_string(), "n2".to_string()],
                    pr_status: None,
                    child_state: None,
                },
            ],
        };
        let refs = stack.effective_base_refs("n3", "origin/master");
        assert_eq!(
            refs,
            vec![
                "origin/feature/n1".to_string(),
                "origin/feature/n2".to_string()
            ]
        );
    }

    #[test]
    fn effective_base_refs_all_merged_returns_bottom_base() {
        let merged_status = || {
            Some(GithubPrStatus {
                phase: "merged".to_string(),
                url: None,
                error: None,
            })
        };
        let stack = Stack {
            version: 1,
            nodes: vec![
                StackNode {
                    node_id: "n1".to_string(),
                    title: "Node 1".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n1".to_string()),
                    session_id: Some("sess-1".to_string()),
                    parents: vec![],
                    pr_status: merged_status(),
                    child_state: None,
                },
                StackNode {
                    node_id: "n2".to_string(),
                    title: "Node 2".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n2".to_string()),
                    session_id: Some("sess-2".to_string()),
                    parents: vec!["n1".to_string()],
                    pr_status: merged_status(),
                    child_state: None,
                },
                StackNode {
                    node_id: "n3".to_string(),
                    title: "Node 3".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n3".to_string()),
                    session_id: None,
                    parents: vec!["n2".to_string()],
                    pr_status: None,
                    child_state: None,
                },
            ],
        };
        let refs = stack.effective_base_refs("n3", "origin/master");
        assert_eq!(refs, vec!["origin/master".to_string()]);
    }

    #[test]
    fn topo_order_linear_dag() {
        let stack = Stack {
            version: 1,
            nodes: vec![
                StackNode {
                    node_id: "n1".to_string(),
                    title: "Node 1".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec![],
                    pr_status: None,
                    child_state: None,
                },
                StackNode {
                    node_id: "n2".to_string(),
                    title: "Node 2".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec!["n1".to_string()],
                    pr_status: None,
                    child_state: None,
                },
                StackNode {
                    node_id: "n3".to_string(),
                    title: "Node 3".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec!["n2".to_string()],
                    pr_status: None,
                    child_state: None,
                },
            ],
        };
        let order = stack.topo_order().unwrap();
        assert_eq!(
            order,
            vec!["n1".to_string(), "n2".to_string(), "n3".to_string()]
        );
    }

    #[test]
    fn topo_order_cycle_returns_error() {
        let stack = Stack {
            version: 1,
            nodes: vec![
                StackNode {
                    node_id: "n1".to_string(),
                    title: "Node 1".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec!["n2".to_string()],
                    pr_status: None,
                    child_state: None,
                },
                StackNode {
                    node_id: "n2".to_string(),
                    title: "Node 2".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec!["n1".to_string()],
                    pr_status: None,
                    child_state: None,
                },
            ],
        };
        let result = stack.topo_order();
        assert!(result.is_err(), "expected Err for cycle, got Ok");
        let err = result.unwrap_err().to_string().to_lowercase();
        assert!(
            err.contains("cycle"),
            "error message should mention 'cycle', got: {err}"
        );
    }

    #[test]
    fn update_stack_atomic_reads_and_writes_back() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mut cs = Changeset::default();
        cs.stack = Some(Stack {
            version: 1,
            nodes: vec![],
        });
        write_changeset(dir, &cs).unwrap();

        update_stack_atomic(dir, |stack| {
            stack.version = 42;
        })
        .unwrap();

        let loaded = read_changeset(dir).unwrap();
        assert_eq!(loaded.stack.unwrap().version, 42);
    }

    #[test]
    fn link_stack_node_to_child_session_sets_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mut cs = Changeset::default();
        cs.stack = Some(Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".to_string(),
                title: "Node 1".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None,
                parents: vec![],
                pr_status: None,
                child_state: None,
            }],
        });
        write_changeset(dir, &cs).unwrap();

        link_stack_node_to_child_session(dir, "n1", "sess-abc", Some("feature/n1".to_string()))
            .unwrap();

        let loaded = read_changeset(dir).unwrap();
        let node = loaded.stack.unwrap();
        let n1 = node.node("n1").unwrap();
        assert_eq!(n1.session_id.as_deref(), Some("sess-abc"));
        assert_eq!(n1.branch.as_deref(), Some("feature/n1"));
    }

    #[test]
    fn sync_stack_node_from_child_propagates_state_and_pr_status() {
        use crate::workflow::ids::WorkflowState;
        let orch_tmp = tempfile::tempdir().unwrap();
        let sessions_tmp = tempfile::tempdir().unwrap();
        let orch_dir = orch_tmp.path();
        let sessions_root = sessions_tmp.path();
        let child_id = "child-session-1";

        // unified_session_dir_path = sessions_root/sessions/<id>
        let child_dir = sessions_root.join("sessions").join(child_id);
        std::fs::create_dir_all(&child_dir).unwrap();
        let mut child_cs = Changeset::default();
        child_cs.state.current = WorkflowState::new("GreenImplementing");
        child_cs.workflow = Some(ChangesetWorkflow {
            github_pr_status: Some(GithubPrStatus {
                phase: "open".to_string(),
                url: Some("https://github.com/example/pr/1".to_string()),
                error: None,
            }),
            ..Default::default()
        });
        write_changeset(&child_dir, &child_cs).unwrap();

        let mut orch_cs = Changeset::default();
        orch_cs.stack = Some(Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".to_string(),
                title: "Node 1".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/n1".to_string()),
                session_id: Some(child_id.to_string()),
                parents: vec![],
                pr_status: None,
                child_state: None,
            }],
        });
        write_changeset(orch_dir, &orch_cs).unwrap();

        sync_stack_node_from_child(orch_dir, sessions_root, "n1").unwrap();

        let loaded = read_changeset(orch_dir).unwrap();
        let stack = loaded.stack.unwrap();
        let n1 = stack.node("n1").unwrap();
        assert_eq!(
            n1.child_state,
            Some(WorkflowState::new("GreenImplementing"))
        );
        assert_eq!(n1.pr_status.as_ref().unwrap().phase, "open");
    }

    #[test]
    fn serde_back_compat_changeset_without_stack_field() {
        let yaml = r#"
version: 1
models: {}
sessions: []
state:
  current: Init
  updated_at: "2024-01-01T00:00:00Z"
  history: []
artifacts: {}
discovery: ~
"#;
        let cs: Changeset =
            serde_yaml::from_str(yaml).expect("legacy changeset should deserialize");
        assert!(
            cs.stack.is_none(),
            "stack should be None for legacy changeset"
        );
        assert!(
            cs.orchestrator_session_id.is_none(),
            "orchestrator_session_id should be None for legacy changeset"
        );
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

#[cfg(test)]
mod ensure_changeset_recipe_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_a_changeset_with_the_recipe_when_none_exists_yet() {
        // Given — a session directory with no changeset.yaml at all (the daemon has allocated
        // the directory but the workflow hasn't written anything into it yet).
        let dir = tempdir().expect("tempdir");

        // When
        ensure_changeset_recipe(dir.path(), "plan-pr-stack").expect("ensure_changeset_recipe");

        // Then
        let cs = read_changeset(dir.path()).expect("changeset.yaml must now exist");
        assert_eq!(cs.recipe, Some("plan-pr-stack".to_string()));
    }

    #[test]
    fn fills_in_the_recipe_on_an_existing_changeset_that_has_none_without_losing_other_fields() {
        // Given — a changeset already on disk (e.g. written by the daemon's session-metadata
        // step) that has no recipe set, but does carry other real data.
        let dir = tempdir().expect("tempdir");
        write_changeset(
            dir.path(),
            &Changeset {
                initial_prompt: Some("plan a 3-PR todo app stack".to_string()),
                recipe: None,
                ..Changeset::default()
            },
        )
        .expect("write_changeset");

        // When
        ensure_changeset_recipe(dir.path(), "plan-pr-stack").expect("ensure_changeset_recipe");

        // Then
        let cs = read_changeset(dir.path()).expect("read_changeset");
        assert_eq!(cs.recipe, Some("plan-pr-stack".to_string()));
        assert_eq!(
            cs.initial_prompt,
            Some("plan a 3-PR todo app stack".to_string()),
            "existing fields must survive filling in the recipe"
        );
    }

    #[test]
    fn does_not_overwrite_an_already_set_different_recipe() {
        // Given — a resumed session whose changeset already has a (different) recipe recorded.
        let dir = tempdir().expect("tempdir");
        write_changeset(
            dir.path(),
            &Changeset {
                recipe: Some("tdd".to_string()),
                ..Changeset::default()
            },
        )
        .expect("write_changeset");

        // When
        ensure_changeset_recipe(dir.path(), "plan-pr-stack").expect("ensure_changeset_recipe");

        // Then — the existing recipe is left untouched, not clobbered by the new call.
        let cs = read_changeset(dir.path()).expect("read_changeset");
        assert_eq!(cs.recipe, Some("tdd".to_string()));
    }
}
