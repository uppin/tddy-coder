//! The resume token a CLI harness prints as it exits, recorded in the session's `.session.yaml` so a
//! later resume continues that conversation.
//!
//! Warp prints `warp --resume <TOKEN>` only as it exits, so the token exists only in the terminal's
//! output once the process has ended. The watcher waits for the terminal to finish, reads its
//! capture and records the token the harness parser finds. It writes nothing when the run printed
//! none, and it changes no field but `harness_resume_token`.
//!
//! It is spawned after the session's metadata is written, never before: the start or the resume
//! writes the whole metadata, and that write would overwrite a token a watcher had recorded first.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_core::cli_harness_warp::resume_token_from_output;
use tddy_core::{read_session_metadata, write_session_metadata};

/// Records the resume token `handle`'s terminal prints as it exits into `session_dir`'s metadata.
/// Runs in the background; a run that prints no token leaves the recorded one as it is.
// FIXME(warp-cli-session): two orderings are not enforced. (a) The terminal is deregistered from
// `CliSessionManager` on the same status change that releases this watcher, so a resume that lands
// in the gap reads the previous token. (b) The PTY reader is not joined before the exit status is
// reported (`tddy-pty` runtime), so the capture can trail `pty_done` and miss the final output. Both
// need a change to the PTY and session-manager runtime, outside this node.
pub(crate) fn watch_for_resume_token(handle: &PtyHandle, session_dir: PathBuf) {
    let capture = Arc::clone(&handle.capture);
    let mut pty_done = handle.pty_done.clone();
    tokio::spawn(async move {
        // The sender is dropped only once the terminal has finished, so an error here means the
        // terminal was torn down without finishing: there is no complete output to read.
        if pty_done.wait_for(|done| *done).await.is_err() {
            return;
        }
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
        if let Err(e) = record_resume_token(&session_dir, &token) {
            log::warn!(
                "failed to record the resume token in {}: {e}",
                session_dir.display()
            );
        }
    });
}

/// Writes `token` as the session's resume token, every other field of its metadata as it was read.
fn record_resume_token(session_dir: &Path, token: &str) -> Result<(), tddy_core::WorkflowError> {
    let mut metadata = read_session_metadata(session_dir)?;
    metadata.harness_resume_token = Some(token.to_string());
    write_session_metadata(session_dir, &metadata)
}
