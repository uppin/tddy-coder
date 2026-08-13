//! Helpers shared by the daemon's PTY-driving acceptance tests.
//!
//! Every test that starts a real agent process and reads its output off a PTY needs the same
//! wait: poll the terminal capture until a marker shows up. Six near-identical copies of that
//! loop had drifted to five different ceilings — 2000ms in one file and 10000ms in another for
//! the *same* stub emitting the *same* marker — and the short ones failed under load as if the
//! daemon had built the wrong command line.
//!
//! There is one ceiling here, and it is deliberately generous. Waiting longer costs nothing when
//! the marker arrives promptly; a machine slow enough to exceed it is not evidence of a bug.

use std::sync::Arc;
use std::time::Duration;

use tddy_daemon::claude_cli_session::PtyHandle;
use tddy_testing_commons::wait::eventually;

/// How long a spawned stub gets to print its first marker to the PTY.
///
/// A safety net, not a prediction: this covers fork/exec, dynamic linking, shell startup and the
/// daemon's own worktree setup, all of which stretch under a parallel test suite.
pub const PTY_STUB_OUTPUT: Duration = Duration::from_secs(10);

/// The terminal capture once it contains `needle`, or a panic naming what the PTY did show.
///
/// Returns the capture so a caller can go on to assert on the surrounding output (the argv line a
/// stub echoed, say) without re-reading it.
pub async fn a_capture_showing(handle: &Arc<PtyHandle>, needle: &str, within: Duration) -> String {
    eventually(
        &format!("the PTY capture to show {needle:?}"),
        within,
        || {
            let capture = handle.capture.lock().expect("terminal capture lock");
            let shown = String::from_utf8_lossy(capture.buffered_bytes()).to_string();
            if shown.contains(needle) {
                Ok(shown)
            } else if shown.is_empty() {
                Err("the PTY has produced no output at all".to_string())
            } else {
                Err(format!("the PTY has produced: {shown:?}"))
            }
        },
    )
    .await
}
