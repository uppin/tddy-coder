//! Run manifest command templates with sandboxed cwd / argv.

use std::path::Path;

use log::{debug, info};
use serde_json::Value;

use super::error::SessionActionsError;
use super::manifest::{parse_action_manifest_file, ActionManifest};
use super::paths::resolve_action_manifest_path;
use super::runtime::run_manifest_blocking;
use super::summary::{invocation_record_summary_value, parse_test_summary_from_process_output};
use super::validate::validate_action_arguments_json;
use crate::session_actions::arch::ensure_action_architecture;
use crate::session_actions::paths::resolve_allowlisted_path;

/// Apply **`result_kind: test_summary`** post-processing to an invoke record (same contract as CLI).
pub fn finalize_invocation_record(
    manifest: &ActionManifest,
    record: &mut Value,
) -> Result<(), SessionActionsError> {
    if manifest.result_kind.as_deref() != Some("test_summary") {
        return Ok(());
    }
    let stdout = record.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
    let stderr = record.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
    let combined = format!("{stdout}{stderr}");
    let summary = parse_test_summary_from_process_output(&combined)?;
    if let Some(obj) = record.as_object_mut() {
        obj.insert(
            "summary".to_string(),
            invocation_record_summary_value(&summary),
        );
    }
    Ok(())
}

/// Resolve, validate, and execute one action — the full invoke pipeline.
///
/// This consolidates the CLI `invoke_action_inner` logic so it can be called from both the
/// `tddy-tools` CLI (local fallback) and the `TDDY_SOCKET` relay listener (relay path).
///
/// - `session_dir`: session directory (for session-overlay manifest lookup and allowlist root).
/// - `store_root`: per-repo store root (e.g. `~/.tddy/actions/<repo-key>/`); `None` disables store lookup.
/// - `repo_root`: working directory for the command subprocess and second allowlist root.
/// - `action_id`: relative path identifier (e.g. `packages/foo/build` or just `run-tests`).
/// - `data_json`: JSON-encoded arguments object (validated against `input_schema`).
pub fn invoke_action_core(
    session_dir: Option<&Path>,
    store_root: Option<&Path>,
    repo_root: Option<&Path>,
    action_id: &str,
    data_json: &str,
) -> Result<serde_json::Value, SessionActionsError> {
    let manifest_path = resolve_action_manifest_path(session_dir, store_root, action_id)?;
    let manifest = parse_action_manifest_file(&manifest_path)?;
    let args: serde_json::Value = serde_json::from_str(data_json)
        .map_err(|e| SessionActionsError::InvalidInvokeJson(e.to_string()))?;

    validate_action_arguments_json(&manifest.input_schema, &args)?;

    if let Some(bind) = manifest.output_path_arg.as_deref() {
        let v = args.get(bind).and_then(|x| x.as_str()).ok_or_else(|| {
            SessionActionsError::ArgumentsViolateSchema(format!(
                "missing string field `{bind}` for output path binding (required by manifest)"
            ))
        })?;
        let session_for_allowlist = session_dir.unwrap_or_else(|| Path::new(".")).to_path_buf();
        resolve_allowlisted_path(&session_for_allowlist, repo_root, v, "output_binding")?;
    }

    ensure_action_architecture(&manifest.architecture)?;

    run_manifest_command(
        session_dir.unwrap_or_else(|| Path::new(".")),
        repo_root,
        &manifest,
        &args,
    )
}

/// Execute the manifest’s declared `command` after arguments are validated and paths resolved.
///
/// Working directory: `repo_root` when provided, otherwise `session_dir`. The command vector is taken
/// verbatim from the manifest (already security-checked as a declarative template in the YAML).
pub fn run_manifest_command(
    session_dir: &Path,
    repo_root: Option<&Path>,
    manifest: &ActionManifest,
    _validated_args: &Value,
) -> Result<Value, SessionActionsError> {
    let cwd = repo_root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| session_dir.to_path_buf());

    debug!(
        target: "tddy_core::session_actions::invoke",
        "run_manifest_command: id={} cwd={}",
        manifest.id,
        cwd.display()
    );

    let record = run_manifest_blocking(manifest, cwd)?;

    info!(
        target: "tddy_core::session_actions::invoke",
        "run_manifest_command finished: id={} exit_code={} stdout_len={} stderr_len={}",
        manifest.id,
        record.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-1),
        record
            .get("stdout")
            .and_then(|v| v.as_str())
            .map(str::len)
            .unwrap_or(0),
        record
            .get("stderr")
            .and_then(|v| v.as_str())
            .map(str::len)
            .unwrap_or(0),
    );

    Ok(record)
}
