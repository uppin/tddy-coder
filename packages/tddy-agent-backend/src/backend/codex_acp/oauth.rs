//! Signing `codex-acp` in again through `codex login` when the agent reports an auth failure.

use std::path::PathBuf;

use crate::error::BackendError;

pub(super) fn acp_error_suggests_retry_oauth(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("auth")
        || m.contains("401")
        || m.contains("unauthorized")
        || m.contains("login")
        || m.contains("credential")
        || m.contains("not signed")
}

fn invoke_codex_oauth_login_blocking(
    codex_bin: PathBuf,
    session_dir: PathBuf,
) -> Result<(), BackendError> {
    let backend = crate::backend::CodexBackend::with_path(codex_bin);
    let mut child = backend.spawn_oauth_login(&session_dir)?;
    let status = child
        .wait()
        .map_err(|e| BackendError::InvocationFailed(format!("codex login wait: {e}")))?;
    if !status.success() {
        return Err(BackendError::InvocationFailed(format!(
            "codex login exited with status {:?}",
            status.code()
        )));
    }
    Ok(())
}

pub(super) async fn run_oauth_login_via_codex_cli(
    codex_cli_path: PathBuf,
    session_dir: PathBuf,
) -> Result<(), BackendError> {
    tokio::task::spawn_blocking(move || {
        invoke_codex_oauth_login_blocking(codex_cli_path, session_dir)
    })
    .await
    .map_err(|e| BackendError::InvocationFailed(format!("codex login join: {e}")))?
}
