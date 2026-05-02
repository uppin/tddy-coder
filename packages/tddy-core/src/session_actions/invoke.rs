//! Run manifest command templates with sandboxed cwd / argv.

use std::path::Path;
use std::process::Command;

use log::{debug, info};
use serde_json::{json, Value};

use super::error::SessionActionsError;
use super::manifest::ActionManifest;
use super::summary::{invocation_record_summary_value, parse_test_summary_from_process_output};

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

    let program = manifest
        .command
        .first()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or(SessionActionsError::EmptyCommand)?;

    let mut cmd = Command::new(program);
    if manifest.command.len() > 1 {
        cmd.args(&manifest.command[1..]);
    }
    cmd.current_dir(&cwd);

    let output = cmd
        .output()
        .map_err(|e| SessionActionsError::CommandSpawn {
            program: program.to_string(),
            detail: e.to_string(),
        })?;

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    info!(
        target: "tddy_core::session_actions::invoke",
        "run_manifest_command finished: id={} exit_code={} stdout_len={} stderr_len={}",
        manifest.id,
        code,
        stdout.len(),
        stderr.len()
    );

    let mut record = json!({
        "exit_code": code,
        "stdout": stdout,
        "stderr": stderr,
    });
    finalize_invocation_record(manifest, &mut record)?;
    Ok(record)
}
