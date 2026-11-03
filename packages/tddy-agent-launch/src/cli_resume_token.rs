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
use std::sync::{Arc, Mutex};

use tddy_cli_sessions::cli_session_manager::{CliSessionManager, ExitHook};
use tddy_core::adapters::ResumeTokenAdapter;
use tddy_core::{read_session_metadata, write_session_metadata};
use tddy_rpc::Status;
use tddy_task::TerminalCapture;
use tokio::sync::oneshot;

/// Releases the recorder built by [`resume_token_recorder`] once the session's metadata is on disk.
/// Holds nothing for an agent with no recorder, so arming it does nothing.
pub(crate) struct ResumeTokenArm(Option<oneshot::Sender<()>>);

impl ResumeTokenArm {
    /// The metadata is written: the recorder may now add the token to it.
    pub(crate) fn arm(self) {
        if let Some(sender) = self.0 {
            let _ = sender.send(());
        }
    }
}

/// The hook recording the resume token `session_dir`'s terminal prints as it exits, read by
/// `adapter` (the agent's `coding_agent.resume.adapter`), and the arm that releases it. A run that
/// prints no token leaves the recorded one as it is. An agent with no resume adapter gets no hook:
/// its resume is by its worktree, so there is no token to record.
pub(crate) fn resume_token_recorder(
    session_dir: PathBuf,
    adapter: Option<Arc<dyn ResumeTokenAdapter>>,
) -> (Option<ExitHook>, ResumeTokenArm) {
    let Some(adapter) = adapter else {
        return (None, ResumeTokenArm(None));
    };
    let (arm_tx, arm_rx) = oneshot::channel();
    let hook: ExitHook = Box::new(move |capture| {
        Box::pin(async move {
            // A dropped arm (the start failed before writing the metadata) records nothing.
            if arm_rx.await.is_ok() {
                record_from_capture(&session_dir, &capture, adapter.as_ref());
            }
        })
    });
    (Some(hook), ResumeTokenArm(Some(arm_tx)))
}

/// Records the resume token `adapter` reads from `capture`, if the run printed one.
fn record_from_capture(
    session_dir: &Path,
    capture: &Mutex<TerminalCapture>,
    adapter: &dyn ResumeTokenAdapter,
) {
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
    let Some(token) = adapter.resume_token(&output) else {
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
