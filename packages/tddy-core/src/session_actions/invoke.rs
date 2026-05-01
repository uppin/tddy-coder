//! Run manifest command templates with sandboxed cwd / argv.

use std::path::Path;
use std::process::Command;

use log::{debug, info};
use serde_json::{json, Value};

use super::error::SessionActionsError;
use super::manifest::ActionManifest;

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

    let output = cmd.output().map_err(|e| SessionActionsError::CommandSpawn {
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

    Ok(json!({
        "exit_code": code,
        "stdout": stdout,
        "stderr": stderr,
    }))
}
