//! Shared helpers for WorkflowEngine integration tests.
//! Each test file uses a subset; allow dead_code to avoid per-file unused warnings.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tddy_core::backend::InvokeRequest;
use tddy_core::changeset::{write_changeset, Changeset, ChangesetState};
use tddy_core::output::create_session_dir_in;
use tddy_core::workflow::graph::{ExecutionResult, ExecutionStatus};
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::{GoalId, SharedBackend, WorkflowEngine, WorkflowRecipe};
use tddy_workflow_recipes::{BugfixRecipe, SessionArtifactManifest, TddRecipe};

static IT_SESSION_BASE_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn unique_tddy_data_dir_for_test() -> PathBuf {
    let n = IT_SESSION_BASE_SEQ.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("tddy-integration-{}-{}", std::process::id(), n))
}

/// Default TDD recipe for integration tests (same behavior as tddy-coder).
pub fn tdd_recipe() -> Arc<dyn WorkflowRecipe> {
    Arc::new(TddRecipe)
}

/// Bugfix workflow recipe for integration tests (same behavior as tddy-coder).
pub fn bugfix_recipe() -> Arc<dyn WorkflowRecipe> {
    Arc::new(BugfixRecipe)
}

/// Session artifact manifest paired with [`tdd_recipe`] (same [`TddRecipe`] instance semantics).
pub fn tdd_manifest() -> Arc<dyn SessionArtifactManifest> {
    Arc::new(TddRecipe)
}

/// [`WorkflowEngine`] wired with the TDD graph and hooks from [`tdd_recipe`].
pub fn tdd_engine(backend: SharedBackend, storage_dir: PathBuf) -> WorkflowEngine {
    let recipe = tdd_recipe();
    let hooks = recipe.create_hooks(None);
    WorkflowEngine::new(recipe, backend, storage_dir, Some(hooks))
}

/// Build an [`InvokeRequest`] using TDD recipe hints (for backend integration tests).
pub fn stub_invoke_request(prompt: impl Into<String>, goal_id: &str) -> InvokeRequest {
    let recipe = tdd_recipe();
    let gid = GoalId::new(goal_id);
    let hints = recipe.goal_hints(&gid).expect("TddRecipe hints");
    let submit_key = recipe.submit_key(&gid);
    InvokeRequest {
        prompt: prompt.into(),
        system_prompt: None,
        system_prompt_path: None,
        goal_id: gid,
        submit_key,
        hints,
        model: None,
        session: None,
        working_dir: None,
        debug: false,
        agent_output: false,
        agent_output_sink: None,
        progress_sink: None,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
        socket_path: None,
        session_dir: None,
        remote: None,
    }
}

/// Build an [`InvokeRequest`] using Bugfix recipe hints (for bugfix backend integration tests).
pub fn bugfix_stub_invoke_request(prompt: impl Into<String>, goal_id: &str) -> InvokeRequest {
    let recipe = bugfix_recipe();
    let gid = GoalId::new(goal_id);
    let hints = recipe
        .goal_hints(&gid)
        .unwrap_or_else(|| panic!("BugfixRecipe must provide goal_hints for {}", goal_id));
    let submit_key = recipe.submit_key(&gid);
    InvokeRequest {
        prompt: prompt.into(),
        system_prompt: None,
        system_prompt_path: None,
        goal_id: gid,
        submit_key,
        hints,
        model: None,
        session: None,
        working_dir: None,
        debug: false,
        agent_output: false,
        agent_output_sink: None,
        progress_sink: None,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
        socket_path: None,
        session_dir: None,
        remote: None,
    }
}

/// New session directory `{unique_base}/sessions/{uuid}/` — isolated per call (no shared env, safe with parallel tests).
pub fn session_dir_for_new_session() -> PathBuf {
    let base = unique_tddy_data_dir_for_test();
    std::fs::create_dir_all(&base).expect("sessions base");
    create_session_dir_in(&base).expect("create_session_dir_in")
}

/// Create a temp directory with a git repo (init, commit, origin/master) for worktree tests.
/// Returns `(repo_root, session_dir)` where `session_dir` is under the integration test session base
/// (see [`session_dir_for_new_session`]), not under `output_dir`.
pub fn temp_dir_with_git_repo(label: &str) -> (PathBuf, PathBuf) {
    let output_dir = std::env::temp_dir().join(format!("tddy-{}-{}", label, std::process::id()));
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create repo dir");

    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&output_dir)
            .output()
            .expect("git command");
    };
    run(&["init"]);
    run(&["config", "user.email", "test@test.com"]);
    run(&["config", "user.name", "Test"]);
    std::fs::write(output_dir.join("README"), "initial").expect("write README");
    run(&["add", "README"]);
    run(&["commit", "-m", "initial"]);
    run(&["branch", "-M", "master"]);
    run(&["remote", "add", "origin", output_dir.to_str().unwrap()]);
    run(&["push", "-u", "origin", "master"]);

    let session_dir = session_dir_for_new_session();
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    (output_dir, session_dir)
}

/// Build context for plan goal.
/// `output_dir` is the repo root; agent runs there to discover `Cargo.toml`, etc.
/// `session_dir` must be the session directory for this run (e.g. from [`session_dir_for_new_session`]).
pub fn ctx_plan(
    feature_input: &str,
    output_dir: PathBuf,
    session_dir: PathBuf,
    answers: Option<&str>,
    conversation_output_path: Option<PathBuf>,
) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "feature_input".to_string(),
        serde_json::json!(feature_input),
    );
    m.insert(
        "output_dir".to_string(),
        serde_json::to_value(output_dir).unwrap(),
    );
    m.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir).unwrap(),
    );
    if let Some(a) = answers {
        m.insert("answers".to_string(), serde_json::json!(a));
    }
    if let Some(p) = conversation_output_path {
        m.insert(
            "conversation_output_path".to_string(),
            serde_json::to_value(p).unwrap(),
        );
    }
    m
}

/// Build context for acceptance-tests goal.
/// output_dir: repo root (required for worktree creation in before_acceptance_tests hook).
/// run_optional_step_x: when true, green will transition to demo (used when running full chain).
pub fn ctx_acceptance_tests(
    session_dir: PathBuf,
    output_dir: Option<PathBuf>,
    answers: Option<&str>,
    run_optional_step_x: bool,
) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir.clone()).unwrap(),
    );
    if let Some(ref d) = output_dir {
        m.insert("output_dir".to_string(), serde_json::to_value(d).unwrap());
    }
    m.insert(
        "run_optional_step_x".to_string(),
        serde_json::json!(run_optional_step_x),
    );
    if let Some(a) = answers {
        m.insert("answers".to_string(), serde_json::json!(a));
    }
    m
}

/// Build context for red goal.
pub fn ctx_red(session_dir: PathBuf, answers: Option<&str>) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir).unwrap(),
    );
    if let Some(a) = answers {
        m.insert("answers".to_string(), serde_json::json!(a));
    }
    m
}

/// Build context for green goal.
pub fn ctx_green(
    session_dir: PathBuf,
    answers: Option<&str>,
    run_optional_step_x: bool,
) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir).unwrap(),
    );
    m.insert(
        "run_optional_step_x".to_string(),
        serde_json::json!(run_optional_step_x),
    );
    if let Some(a) = answers {
        m.insert("answers".to_string(), serde_json::json!(a));
    }
    m
}

/// Build context for evaluate goal.
/// output_dir is the project root (for Cargo.toml discovery); defaults to session_dir if None.
pub fn ctx_evaluate(
    session_dir: PathBuf,
    output_dir: Option<PathBuf>,
) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir.clone()).unwrap(),
    );
    m.insert(
        "output_dir".to_string(),
        serde_json::to_value(output_dir.unwrap_or(session_dir)).unwrap(),
    );
    m
}

/// Build context for validate goal.
pub fn ctx_validate(session_dir: PathBuf) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir).unwrap(),
    );
    m
}

/// Build context for refactor goal.
pub fn ctx_refactor(session_dir: PathBuf) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir).unwrap(),
    );
    m
}

/// Build context for update-docs goal.
pub fn ctx_update_docs(session_dir: PathBuf) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir).unwrap(),
    );
    m
}

/// Build context for demo goal.
pub fn ctx_demo(session_dir: PathBuf) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir).unwrap(),
    );
    m
}

/// Get session_dir from session context after a run. Re-exported for test use.
pub async fn get_session_dir_from_session(
    engine: &WorkflowEngine,
    session_id: &str,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let session = engine
        .get_session(session_id)
        .await?
        .ok_or("Session not found")?;
    session
        .context
        .get_sync("session_dir")
        .or_else(|| session.context.get_sync("output_dir"))
        .ok_or("session_dir not in session context".into())
}

/// Run plan and return (session_dir, session_id). Returns Err on WaitingForInput or Error.
pub async fn run_plan(
    engine: &WorkflowEngine,
    input: &str,
    output_dir: &std::path::Path,
    answers: Option<&str>,
) -> Result<(PathBuf, String), Box<dyn std::error::Error + Send + Sync>> {
    run_plan_with_conversation_output(engine, input, output_dir, answers, None).await
}

/// Run plan with optional conversation_output_path for raw agent output.
pub async fn run_plan_with_conversation_output(
    engine: &WorkflowEngine,
    input: &str,
    output_dir: &std::path::Path,
    answers: Option<&str>,
    conversation_output_path: Option<PathBuf>,
) -> Result<(PathBuf, String), Box<dyn std::error::Error + Send + Sync>> {
    let session_dir = session_dir_for_new_session();
    std::fs::create_dir_all(&session_dir)?;
    let init_cs = Changeset {
        initial_prompt: Some(input.to_string()),
        ..Changeset::default()
    };
    let _ = write_changeset(&session_dir, &init_cs);

    let context = ctx_plan(
        input,
        output_dir.to_path_buf(),
        session_dir.clone(),
        answers,
        conversation_output_path,
    );
    let plan_gid = GoalId::new("plan");
    let result = engine.run_goal(&plan_gid, context).await?;

    match &result.status {
        ExecutionStatus::WaitingForInput { .. } => return Err("ClarificationNeeded".into()),
        ExecutionStatus::Error(e) => return Err(e.clone().into()),
        _ => {}
    }

    let session_dir = get_session_dir_from_session(engine, &result.session_id).await?;
    Ok((session_dir, result.session_id))
}

/// Write a minimal changeset.yaml with Planned state for a plan session.
/// Includes branch_suggestion and worktree_suggestion for worktree creation.
pub fn write_changeset_for_session(session_dir: &std::path::Path, session_id: &str) {
    let cs = Changeset {
        name: Some("feature".to_string()),
        sessions: vec![tddy_core::changeset::SessionEntry {
            id: session_id.to_string(),
            agent: "claude".to_string(),
            tag: "plan".to_string(),
            created_at: "2026-03-07T10:00:00Z".to_string(),
            system_prompt_file: None,
        }],
        state: ChangesetState {
            current: WorkflowState::new("Planned"),
            updated_at: "2026-03-07T10:00:00Z".to_string(),
            history: vec![],
            ..Changeset::default().state
        },
        branch_suggestion: Some("feature/test".to_string()),
        worktree_suggestion: Some("feature-test".to_string()),
        ..Changeset::default()
    };
    write_changeset(session_dir, &cs).expect("write changeset");
}

/// Write a minimal changeset.yaml with custom state.
/// Sets `state.session_id` to `session_id` (persisted agent thread id for resume).
/// Includes branch_suggestion and worktree_suggestion for worktree creation.
pub fn write_changeset_with_state(session_dir: &std::path::Path, state: &str, session_id: &str) {
    let cs = Changeset {
        name: Some("feature".to_string()),
        sessions: vec![tddy_core::changeset::SessionEntry {
            id: session_id.to_string(),
            agent: "claude".to_string(),
            tag: "plan".to_string(),
            created_at: "2026-03-07T10:00:00Z".to_string(),
            system_prompt_file: None,
        }],
        state: ChangesetState {
            current: WorkflowState::new(state),
            updated_at: "2026-03-07T10:00:00Z".to_string(),
            history: vec![],
            session_id: Some(session_id.to_string()),
        },
        branch_suggestion: Some("feature/test".to_string()),
        worktree_suggestion: Some("feature-test".to_string()),
        ..Changeset::default()
    };
    write_changeset(session_dir, &cs).expect("write changeset");
}

/// Write a minimal evaluation-report.md to session_dir (for validate goal prerequisite).
pub fn write_evaluation_report_to_session_dir(session_dir: &std::path::Path) {
    let content = r#"# Evaluation Report

## Summary

Evaluated 3 changed files. Risk level: medium.

## Risk Level

medium

## Changed Files

- src/main.rs (modified, +15/-3)
- src/lib.rs (modified, +5/-0)
- tests/main_test.rs (added, +40/-0)

## Affected Tests

- tests/main_test.rs: created
- tests/integration_test.rs: updated

## Validity Assessment

The change is valid for the intended use-case.
"#;
    std::fs::write(session_dir.join("evaluation-report.md"), content)
        .expect("write evaluation-report.md");
}

/// Write refactoring-plan.md to session_dir (for refactor goal prerequisite).
pub fn write_refactoring_plan(session_dir: &std::path::Path) {
    let content = r#"# Refactoring Plan

## Priority: Critical

1. **Rename Goal::ValidateRefactor to Goal::Validate**
   - Scope: backend/mod.rs, workflow/mod.rs
   - Estimated effort: small

## Priority: High

2. **Rename internal types**
   - Scope: workflow/mod.rs, lib.rs
   - Estimated effort: medium
"#;
    std::fs::write(session_dir.join("refactoring-plan.md"), content)
        .expect("write refactoring-plan.md");
}

/// Run a goal and return ExecutionResult. Loop on Paused until Completed/Error/WaitingForInput.
pub async fn run_goal_until_done(
    engine: &WorkflowEngine,
    goal: &str,
    context: HashMap<String, serde_json::Value>,
) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
    let gid = GoalId::new(goal);
    let mut result = engine.run_goal(&gid, context.clone()).await?;
    loop {
        match &result.status {
            ExecutionStatus::Completed | ExecutionStatus::Error(_) => return Ok(result),
            ExecutionStatus::WaitingForInput { .. } => return Ok(result),
            ExecutionStatus::ElicitationNeeded { .. } => return Ok(result),
            ExecutionStatus::Paused { .. } => {
                result = engine.run_session(&result.session_id).await?;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Spec-driven headless backend over a stub binary (`#agent-tools 15/19`)
// ---------------------------------------------------------------------------

/// A host with nothing of the real machine: the env it is given, and executable files.
struct AFixtureHost {
    env: std::collections::BTreeMap<String, String>,
}

impl tddy_actions::tool::ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn is_executable_file(&self, path: &std::path::Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &std::path::Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// The file name of the binary a spec's agent runs.
fn stub_binary_name(agent: &str) -> &'static str {
    match agent {
        "claude" => "claude",
        "cursor" => "agent",
        "codex" => "codex",
        other => panic!("no stub binary is known for agent {other}"),
    }
}

/// The directory a [`a_spec_backend`] fixture lives in, canonicalised (`/var` vs `/private/var`).
pub fn spec_workspace_root(dir: &tempfile::TempDir) -> PathBuf {
    std::fs::canonicalize(dir.path()).expect("workspace root")
}

/// A [`tddy_core::SpecHeadlessBackend`] for `agent` (`claude`, `cursor` or `codex`) over a stub
/// binary. The stub first records its argv (one entry per line) to `argv.txt` and its whole
/// environment to `env.txt` in the workspace root, then runs `stub_script` (shell lines).
///
/// The workspace holds an empty `tools/`, `bin/` with the stub (and a `tddy-tools` stub), a
/// `session/` directory and `repo/`, a git checkout. [`in_spec_workspace`] points a request at them.
pub fn a_spec_backend(
    agent: &str,
    stub_script: &str,
) -> (tddy_core::SpecHeadlessBackend, tempfile::TempDir) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("workspace");
    let root = spec_workspace_root(&dir);
    for sub in ["tools", "bin", "session", "repo"] {
        std::fs::create_dir(root.join(sub)).expect("workspace dir");
    }
    let git = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root.join("repo"))
        .status()
        .expect("git runs");
    assert!(git.success(), "the repo is a git checkout");
    let stub = format!(
        "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{argv}'\nenv > '{env}'\n{stub_script}\n",
        argv = root.join("argv.txt").display(),
        env = root.join("env.txt").display(),
    );
    for (name, script) in [
        (stub_binary_name(agent), stub.as_str()),
        ("tddy-tools", "#!/bin/sh\n"),
    ] {
        let path = root.join("bin").join(name);
        std::fs::write(&path, script).expect("write stub");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let bin = |name: &str| root.join("bin").join(name).to_string_lossy().into_owned();
    let host: Arc<dyn tddy_actions::tool::ToolHost> = Arc::new(AFixtureHost {
        env: std::collections::BTreeMap::from([
            ("TDDY_CLAUDE_BINARY".to_string(), bin("claude")),
            ("TDDY_CURSOR_AGENT".to_string(), bin("agent")),
            ("TDDY_CODEX_CLI".to_string(), bin("codex")),
            ("TDDY_TOOLS_BINARY".to_string(), bin("tddy-tools")),
        ]),
    });
    let backend = tddy_coder::headless_from_spec::headless_backend_from_spec(
        agent,
        &root.join("tools"),
        host,
    )
    .unwrap_or_else(|e| panic!("the {agent} backend renders from its spec: {e:#}"));
    (backend, dir)
}

/// `request` run in the fixture's git checkout, with its session directory, unless it names its own.
pub fn in_spec_workspace(dir: &tempfile::TempDir, mut request: InvokeRequest) -> InvokeRequest {
    let root = spec_workspace_root(dir);
    request.working_dir.get_or_insert_with(|| root.join("repo"));
    request
        .session_dir
        .get_or_insert_with(|| root.join("session"));
    request
}

/// What the stub recorded: `argv.txt` (one entry per line) or `env.txt`.
pub fn spec_recorded(dir: &tempfile::TempDir, file: &str) -> String {
    std::fs::read_to_string(spec_workspace_root(dir).join(file))
        .unwrap_or_else(|e| panic!("the stub recorded {file}: {e}"))
}

/// The argv the stub saw, one entry per line (a multi-line argument spans entries).
pub fn spec_argv(dir: &tempfile::TempDir) -> Vec<String> {
    spec_recorded(dir, "argv.txt")
        .lines()
        .map(str::to_string)
        .collect()
}

/// Building the `agent` backend over a host whose binary does not exist: the error text.
pub fn a_spec_backend_over_a_missing_binary(
    agent: &str,
) -> Result<tddy_core::SpecHeadlessBackend, String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("workspace");
    let root = spec_workspace_root(&dir);
    for sub in ["tools", "bin"] {
        std::fs::create_dir(root.join(sub)).expect("workspace dir");
    }
    let tools = root.join("bin").join("tddy-tools");
    std::fs::write(&tools, "#!/bin/sh\n").expect("write stub");
    std::fs::set_permissions(&tools, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let missing = root.join("bin").join("definitely-no-such-binary-9f3a2c1b");
    let missing = missing.to_string_lossy().into_owned();
    let tools = tools.to_string_lossy().into_owned();
    let host: Arc<dyn tddy_actions::tool::ToolHost> = Arc::new(AFixtureHost {
        env: std::collections::BTreeMap::from([
            ("TDDY_CLAUDE_BINARY".to_string(), missing.clone()),
            ("TDDY_CURSOR_AGENT".to_string(), missing.clone()),
            ("TDDY_CODEX_CLI".to_string(), missing.clone()),
            ("TDDY_TOOLS_BINARY".to_string(), tools),
        ]),
    });
    tddy_coder::headless_from_spec::headless_backend_from_spec(agent, &root.join("tools"), host)
        .map_err(|e| format!("{e:#}"))
}
