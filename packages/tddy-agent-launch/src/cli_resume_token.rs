//! The resume token a CLI harness prints as it exits, recorded in the session's `.session.yaml` so a
//! later resume continues that conversation.
//!
//! Warp prints `warp --resume <TOKEN>` only as it exits, so the token exists only in the terminal's
//! output once the process has ended. The recorder is an [`ExitHook`] the session manager runs once
//! the process has exited and its output is complete, **before** the terminal is deregistered: whoever
//! sees the terminal gone, and resumes, reads the new token. It writes nothing when the run printed
//! none, and it changes no field but `harness_resume_token`.
//!
//! The hook is handed over at spawn, so a fast exit cannot slip past it, but it is held until the
//! caller [arms](ResumeTokenArm::arm) it: the start or the resume writes the whole metadata, and that
//! write would overwrite a token recorded first. A caller that never arms it (it failed before the
//! metadata was written) records nothing.
//!
//! A resume reads the token through [`settled_resume_token`], which first waits for the previous
//! run's recorder to finish: a resume that follows the exit closely still continues its conversation.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tddy_cli_sessions::cli_session_manager::{CliSessionManager, ExitHook};
use tddy_core::cli_harness_warp::resume_token_from_output;
use tddy_core::{read_session_metadata, write_session_metadata};
use tddy_rpc::Status;
use tddy_task::TerminalCapture;
use tokio::sync::oneshot;

/// Releases the recorder built by [`resume_token_recorder`] once the session's metadata is on disk.
pub(crate) struct ResumeTokenArm(oneshot::Sender<()>);

impl ResumeTokenArm {
    /// The metadata is written: the recorder may now add the token to it.
    pub(crate) fn arm(self) {
        let _ = self.0.send(());
    }
}

/// The hook recording the resume token `session_dir`'s terminal prints as it exits, and the arm that
/// releases it. A run that prints no token leaves the recorded one as it is.
pub(crate) fn resume_token_recorder(session_dir: PathBuf) -> (ExitHook, ResumeTokenArm) {
    let (arm_tx, arm_rx) = oneshot::channel();
    let hook: ExitHook = Box::new(move |capture| {
        Box::pin(async move {
            // A dropped arm (the start failed before writing the metadata) records nothing.
            if arm_rx.await.is_ok() {
                record_from_capture(&session_dir, &capture);
            }
        })
    });
    (hook, ResumeTokenArm(arm_tx))
}

/// Records the resume token `capture` ends with, if it holds one.
fn record_from_capture(session_dir: &Path, capture: &Mutex<TerminalCapture>) {
    let output = match capture.lock() {
        Ok(capture) => capture.buffered_bytes().to_vec(),
        Err(_) => {
            log::warn!(
                "resume token not read: the terminal capture of {} is poisoned",
                session_dir.display()
            );
            return;
        }
    };
    let Some(token) = resume_token_from_output(&output) else {
        return;
    };
    if let Err(e) = record_resume_token(session_dir, &token) {
        log::warn!(
            "failed to record the resume token in {}: {e}",
            session_dir.display()
        );
    }
}

/// Writes `token` as the session's resume token, every other field of its metadata as it was read.
fn record_resume_token(session_dir: &Path, token: &str) -> Result<(), tddy_core::WorkflowError> {
    let mut metadata = read_session_metadata(session_dir)?;
    metadata.harness_resume_token = Some(token.to_string());
    write_session_metadata(session_dir, &metadata)
}

/// The resume token `session_dir`'s session has recorded, once the recorder of its previous run (if
/// that run has exited) has finished writing it.
pub(crate) async fn settled_resume_token(
    cli_manager: &CliSessionManager,
    session_id: &str,
    session_dir: &Path,
) -> Result<Option<String>, Status> {
    cli_manager.exit_hook_settled(session_id).await;
    read_session_metadata(session_dir)
        .map(|metadata| metadata.harness_resume_token)
        .map_err(|e| Status::internal(format!("failed to read session metadata: {e}")))
}
