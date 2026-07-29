//! Acceptance test: attaching a browser to a running terminal must restore mouse tracking.
//!
//! `StreamTerminalOutput` is the path the web terminal uses. The browser always measures its
//! grid first, so it always supplies `initial_cols`/`initial_rows` — and on that branch the
//! daemon deliberately skips the capture replay (it would be drawn at the pre-resize width) and
//! relies on the SIGWINCH redraw instead. A redraw does not re-emit DECSET private modes, so the
//! browser's VT never learns that the application enabled mouse tracking, and `GhosttyTerminal`
//! drops every click, drag and scroll (it gates on `hasMouseTracking()`).
//!
//! The mode state therefore has to be sent as its own prologue, independent of the replay.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tddy_daemon::claude_cli_session::{ClaudeCliSessionManager, PtyHandle};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, StreamReplayMode, StreamTerminalOutputRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const SESSION_ID: &str = "mouse-mode-session";
/// Empty `terminal_id` resolves to the reserved main ("claude") terminal.
const MAIN_TERMINAL: &str = "";
/// Dimensions a browser measures before opening the stream.
const BROWSER_COLS: u32 = 120;
const BROWSER_ROWS: u32 = 40;
/// The stub's burst has to have pushed the mouse modes out of the ring before the client attaches.
const STUB_OUTPUT_TIMEOUT_MS: u64 = 15_000;
/// How long the attached client waits for its first frame.
const FIRST_FRAME_TIMEOUT_MS: u64 = 5_000;

/// The modes the stub application enables at startup, in the order a prologue must re-send them.
const MOUSE_TRACKING_PROLOGUE: &[u8] = b"\x1b[?1000h\x1b[?1002h\x1b[?1006h";

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn test_config() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "users:\n  - github_user: \"testuser\"\n    os_user: \"testuser\"\n";
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    (dir, config)
}

/// Build a service wired to `manager`, returning temp-dir guards that must stay alive.
fn make_service(
    manager: Arc<ClaudeCliSessionManager>,
) -> (ConnectionServiceImpl, tempfile::TempDir, tempfile::TempDir) {
    let (cfg_dir, config) = test_config();
    let sessions = tempfile::tempdir().unwrap();
    let sessions_base = sessions.path().to_path_buf();
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == VALID_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    let service = ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        manager,
    );
    (service, cfg_dir, sessions)
}

/// A stub agent CLI that behaves like a mouse-driven TUI: enable SGR mouse tracking, emit far
/// more output than the daemon's capture ring retains, then block on stdin like a real agent CLI.
fn write_mouse_tracking_stub(dir: &std::path::Path) -> PathBuf {
    let script = dir.join("stub_mouse_tui.sh");
    let line = "x".repeat(90);
    let body = format!(
        "#!/bin/sh\n\
         # ignore claude-style argv\n\
         printf '\\033[?1000h\\033[?1002h\\033[?1006h'\n\
         i=0\n\
         while [ $i -lt 900 ]; do\n\
         \x20 printf '%s\\n' '{line}'\n\
         \x20 i=$((i+1))\n\
         done\n\
         exec cat\n",
        line = line
    );
    std::fs::write(&script, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

/// Start the main terminal for `SESSION_ID` running the mouse-tracking stub, and wait until its
/// burst has pushed the startup DECSETs out of the capture ring — the state in which a late
/// client can only learn the modes from a prologue. Returns the worktree guard (keeps the dir
/// alive).
async fn start_main_terminal_whose_mouse_modes_have_been_trimmed_away(
    manager: &ClaudeCliSessionManager,
) -> tempfile::TempDir {
    let worktree = tempfile::tempdir().unwrap();
    let stub = write_mouse_tracking_stub(worktree.path());
    let handle = manager
        .start(
            SESSION_ID,
            worktree.path().to_path_buf(),
            "claude-opus-4-8",
            stub.to_str().unwrap(),
            None,
            None,
        )
        .await
        .expect("main claude terminal must start");
    wait_until_the_ring_has_trimmed_past_the_modes(&handle, STUB_OUTPUT_TIMEOUT_MS).await;
    worktree
}

/// Poll the capture until the daemon has both *seen* the startup DECSETs and *evicted* them from
/// the retained output — the only state in which a prologue is the sole way a late client can
/// learn the modes. Checking eviction alone would pass trivially before the PTY reader has
/// delivered anything at all.
async fn wait_until_the_ring_has_trimmed_past_the_modes(handle: &Arc<PtyHandle>, timeout_ms: u64) {
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        {
            let cap = handle.capture.lock().unwrap();
            let modes_are_known = !cap.mode_prologue().is_empty();
            let modes_left_the_ring = !cap
                .buffered_bytes()
                .windows(MOUSE_TRACKING_PROLOGUE.len())
                .any(|window| window == MOUSE_TRACKING_PROLOGUE);
            if modes_are_known && modes_left_the_ring {
                return;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "capture ring never trimmed past the startup mouse modes"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Attach as a browser does — supplying measured dimensions — and take the first frame.
async fn first_frame_seen_by_a_browser_client(service: &ConnectionServiceImpl) -> Vec<u8> {
    let response = service
        .stream_terminal_output(Request::new(StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            terminal_id: MAIN_TERMINAL.to_string(),
            initial_cols: BROWSER_COLS,
            initial_rows: BROWSER_ROWS,
            mode: StreamReplayMode::Tail as i32,
            from_offset: 0,
        }))
        .await
        .expect("stream_terminal_output must accept a valid token");
    let mut stream = response.into_inner();
    let frame = tokio::time::timeout(Duration::from_millis(FIRST_FRAME_TIMEOUT_MS), stream.next())
        .await
        .expect("attached client received no frame at all")
        .expect("terminal output stream ended without a frame")
        .expect("terminal output stream failed");
    frame.data
}

/// Render terminal bytes so assertion failures are readable.
fn readable(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| match byte {
            0x1b => "ESC".to_string(),
            b if b.is_ascii_graphic() || *b == b' ' => (*b as char).to_string(),
            b => format!("\\x{b:02x}"),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The browser attach path
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn sends_the_mouse_tracking_prologue_first_to_a_browser_client_that_supplies_dimensions() {
    // Given a running agent terminal whose TUI enabled SGR mouse tracking at startup and has
    // since written far more output than the capture ring retains
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let _worktree = start_main_terminal_whose_mouse_modes_have_been_trimmed_away(&manager).await;
    let (service, _cfg_dir, _sessions) = make_service(Arc::clone(&manager));

    // When a browser attaches with its measured dimensions
    let frame = first_frame_seen_by_a_browser_client(&service).await;

    // Then the very first bytes it receives put its own VT back into mouse-tracking mode
    assert_eq!(
        readable(&frame),
        readable(MOUSE_TRACKING_PROLOGUE),
        "first frame must be the mouse-tracking prologue"
    );
}
