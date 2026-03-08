//! Changeset manifest — unified workflow state, sessions, and model configuration.
//!
//! Replaces `.session` and `.impl-session` with a single `changeset.yaml` file.

use crate::backend::ClarificationQuestion;
use crate::error::WorkflowError;
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
    pub description: String,
}

/// Changeset manifest stored in plan directory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Changeset {
    /// PRD/feature name from plan agent (e.g. "Auth Feature").
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
    pub current: String,
    pub updated_at: String,
    #[serde(default)]
    pub history: Vec<StateTransition>,
}

/// State transition for history.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StateTransition {
    pub state: String,
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
    pub plan_dir_suggestion: Option<String>,
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

impl Default for Changeset {
    fn default() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            name: None,
            initial_prompt: None,
            clarification_qa: Vec::new(),
            version: 1,
            models: default_models(),
            sessions: Vec::new(),
            state: ChangesetState {
                current: "Init".to_string(),
                updated_at: now.clone(),
                history: vec![StateTransition {
                    state: "Init".to_string(),
                    at: now,
                }],
            },
            artifacts: default_artifacts(),
            discovery: None,
        }
    }
}

fn default_models() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("plan".to_string(), "opus".to_string());
    m.insert("acceptance-tests".to_string(), "sonnet".to_string());
    m.insert("red".to_string(), "sonnet".to_string());
    m.insert("green".to_string(), "sonnet".to_string());
    m.insert("demo".to_string(), "sonnet".to_string());
    m
}

fn default_artifacts() -> BTreeMap<String, String> {
    let mut a = BTreeMap::new();
    a.insert("prd".to_string(), "PRD.md".to_string());
    a.insert("todo".to_string(), "TODO.md".to_string());
    a.insert(
        "acceptance_tests".to_string(),
        "acceptance-tests.md".to_string(),
    );
    a.insert("red_output".to_string(), "red-output.md".to_string());
    a.insert("progress".to_string(), "progress.md".to_string());
    a.insert("demo_plan".to_string(), "demo-plan.md".to_string());
    a.insert("demo_results".to_string(), "demo-results.md".to_string());
    a.insert(
        "system_prompt_plan".to_string(),
        "system-prompt-plan.md".to_string(),
    );
    a
}

/// Read changeset from plan directory.
pub fn read_changeset(plan_dir: &Path) -> Result<Changeset, WorkflowError> {
    let path = plan_dir.join("changeset.yaml");
    let content = fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            WorkflowError::ChangesetMissing(format!(
                "changeset.yaml not found in {} — run the plan goal first",
                plan_dir.display()
            ))
        } else {
            WorkflowError::ChangesetInvalid(e.to_string())
        }
    })?;
    serde_yaml::from_str(&content).map_err(|e| WorkflowError::ChangesetInvalid(e.to_string()))
}

/// Write changeset to plan directory.
pub fn write_changeset(plan_dir: &Path, changeset: &Changeset) -> Result<(), WorkflowError> {
    let path = plan_dir.join("changeset.yaml");
    let content =
        serde_yaml::to_string(changeset).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    fs::write(&path, content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Resolve model for a goal: CLI override > changeset > None.
pub fn resolve_model(
    changeset: Option<&Changeset>,
    goal: &str,
    cli_model: Option<&str>,
) -> Option<String> {
    if let Some(m) = cli_model {
        return Some(m.to_string());
    }
    changeset.and_then(|c| c.models.get(goal)).cloned()
}

/// Map changeset state to the next goal to execute in the full workflow.
/// Returns `None` when workflow is complete (GreenComplete) or failed.
/// NOTE: validate-changes states ("Validating", "Validated") return None —
/// validate-changes is a standalone goal, not part of the auto-sequence.
pub fn next_goal_for_state(state: &str) -> Option<&'static str> {
    match state {
        "Init" => Some("plan"),
        "Planned" => Some("acceptance-tests"),
        "AcceptanceTestsReady" => Some("red"),
        "RedTestsReady" => Some("green"),
        "GreenComplete" => Some("demo"),
        "DemoComplete" | "DemoSkipped" => Some("evaluate"),
        "Evaluated" | "Failed" | "Validating" | "Validated" => None,
        _ => Some("plan"),
    }
}

/// Get session ID for a tag (e.g. "plan" or "impl").
pub fn get_session_for_tag(changeset: &Changeset, tag: &str) -> Option<String> {
    changeset
        .sessions
        .iter()
        .rfind(|s| s.tag == tag)
        .map(|s| s.id.clone())
}

/// Update workflow state only (no new session).
pub fn update_state(changeset: &mut Changeset, new_state: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    changeset.state.history.push(StateTransition {
        state: new_state.to_string(),
        at: now.clone(),
    });
    changeset.state.current = new_state.to_string();
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
    new_state: &str,
    agent: &str,
    system_prompt_file: Option<String>,
) {
    let now = chrono::Utc::now().to_rfc3339();
    changeset.sessions.push(SessionEntry {
        id: session_id,
        agent: agent.to_string(),
        tag: tag.to_string(),
        created_at: now.clone(),
        system_prompt_file,
    });
    changeset.state.history.push(StateTransition {
        state: new_state.to_string(),
        at: now.clone(),
    });
    changeset.state.current = new_state.to_string();
    changeset.state.updated_at = now;
}
