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
            .next_goal_for_state(&cs.state.current)
            .unwrap_or_else(|| start.clone());
    }
    let mut tdd_skipped_trailing_planning = false;
    for transition in cs.state.history.iter().rev() {
        if transition.state.as_str() == "Failed" {
            continue;
        }
        match recipe.next_goal_for_state(&transition.state) {
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
