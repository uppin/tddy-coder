//! `list-actions` / `invoke-action` CLI orchestration (Session Actions PRD).

use std::path::{Path, PathBuf};

use log::{debug, info};
use serde::Serialize;
use serde_json::Value;

use tddy_core::session_actions::{
    ensure_action_architecture, invocation_record_summary_value, list_action_summaries,
    parse_action_manifest_file, parse_test_summary_from_process_output, resolve_allowlisted_path,
    run_manifest_command, validate_action_arguments_json, SessionActionsError,
};
use tddy_core::{read_changeset, WorkflowError};

#[derive(Debug, Serialize)]
pub struct ListActionsResponse {
    pub actions: Vec<tddy_core::session_actions::ActionSummary>,
}

pub fn run_list_actions(session_dir: &Path) -> anyhow::Result<()> {
    info!(
        target: "tddy_tools::session_actions_cli",
        "list-actions session_dir={}",
        session_dir.display()
    );
    let actions = list_action_summaries(session_dir).map_err(anyhow::Error::from)?;
    let out = ListActionsResponse { actions };
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}

pub fn run_invoke_action(
    session_dir: &Path,
    action_id: &str,
    data_json: &str,
) -> anyhow::Result<()> {
    debug!(
        target: "tddy_tools::session_actions_cli",
        "invoke-action begin action_id={} session_dir={}",
        action_id,
        session_dir.display()
    );

    match invoke_action_inner(session_dir, action_id, data_json) {
        Ok(v) => {
            println!("{}", serde_json::to_string(&v)?);
            Ok(())
        }
        Err(e) => {
            let code = classify_session_actions_exit(&e);
            eprintln!("{e}");
            std::process::exit(code);
        }
    }
}

fn classify_session_actions_exit(e: &SessionActionsError) -> i32 {
    match e {
        SessionActionsError::ArgumentsViolateSchema(_)
        | SessionActionsError::InvalidSchemaShape(_)
        | SessionActionsError::PathOutsideAllowlist { .. }
        | SessionActionsError::PathTraversalAttempt { .. }
        | SessionActionsError::InvalidInvokeJson(_) => 3,
        _ => 1,
    }
}

fn invoke_action_inner(
    session_dir: &Path,
    action_id: &str,
    data_json: &str,
) -> Result<Value, SessionActionsError> {
    let manifest_path = resolve_manifest_path(session_dir, action_id)?;
    let manifest = parse_action_manifest_file(&manifest_path)?;
    let args: Value = serde_json::from_str(data_json)
        .map_err(|e| SessionActionsError::InvalidInvokeJson(e.to_string()))?;

    validate_action_arguments_json(&manifest.input_schema, &args)?;

    let repo_cached = load_repo_root(session_dir)?;

    if let Some(bind) = manifest.output_path_arg.as_deref() {
        let v = args
            .get(bind)
            .and_then(|x| x.as_str())
            .ok_or_else(|| {
                SessionActionsError::ArgumentsViolateSchema(format!(
                    "missing string field `{bind}` for output path binding (required by manifest)"
                ))
            })?;
        resolve_allowlisted_path(session_dir, repo_cached.as_deref(), v, "output_binding")?;
    }

    ensure_action_architecture(&manifest.architecture)?;

    let mut record = run_manifest_command(
        session_dir,
        repo_cached.as_deref(),
        &manifest,
        &args,
    )?;

    if manifest.result_kind.as_deref() == Some("test_summary") {
        let stdout = record
            .get("stdout")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let stderr = record
            .get("stderr")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let combined = format!("{stdout}{stderr}");
        let summary = parse_test_summary_from_process_output(&combined)?;
        if let Some(obj) = record.as_object_mut() {
            obj.insert(
                "summary".to_string(),
                invocation_record_summary_value(&summary),
            );
        }
    }

    Ok(record)
}

fn load_repo_root(session_dir: &Path) -> Result<Option<PathBuf>, SessionActionsError> {
    match read_changeset(session_dir) {
        Ok(cs) => {
            let p = cs
                .repo_path
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from);
            debug!(
                target: "tddy_tools::session_actions_cli",
                "load_repo_root: repo_path={:?}",
                p.as_ref().map(|x| x.display().to_string())
            );
            Ok(p)
        }
        Err(WorkflowError::ChangesetMissing(_)) => {
            debug!(
                target: "tddy_tools::session_actions_cli",
                "load_repo_root: no changeset.yaml; repo_path unavailable"
            );
            Ok(None)
        }
        Err(e) => Err(SessionActionsError::ChangesetRead(e.to_string())),
    }
}

fn resolve_manifest_path(session_dir: &Path, action_id: &str) -> Result<PathBuf, SessionActionsError> {
    let actions_dir = session_dir.join("actions");
    for ext in ["yaml", "yml"] {
        let cand = actions_dir.join(format!("{action_id}.{ext}"));
        if cand.is_file() {
            return Ok(cand);
        }
    }
    Err(SessionActionsError::UnknownActionId(action_id.to_string()))
}
