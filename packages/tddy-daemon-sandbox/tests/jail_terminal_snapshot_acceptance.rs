//! Acceptance: a jailed terminal's snapshot is serialized at the size the jail reported.
//!
//! The daemon cannot resize a jail's PTY or see its size; the runner reports the size it applied,
//! in order with the output. These tests feed those events through the daemon's forwarder and check
//! the capture a snapshot is taken from, rendering the snapshot in an independent VT.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use tddy_daemon_sandbox::sandbox_session::forward_jail_terminal_events;
use tddy_sandbox_runner::HostTerminalEvent;
use tddy_task::TerminalCapture;
use tokio::sync::{broadcast, mpsc};

const STARTUP: &[u8] = b"\x1b[?1049h\x1b[2J\x1b[H\x1b[1mjailed\x1b[0m";
const REPAINT: &[u8] = b"\x1b[2J\x1b[H\x1b[1mresized\x1b[0m\x1b[40;140Hx";

/// A jailed terminal's capture, fed by the forwarder from the events the runner reports.
struct JailedTerminal {
    capture: Arc<Mutex<TerminalCapture>>,
    events: mpsc::UnboundedSender<HostTerminalEvent>,
    _live: broadcast::Receiver<Bytes>,
}

impl JailedTerminal {
    fn started() -> Self {
        let capture = Arc::new(Mutex::new(TerminalCapture::new()));
        let (stdout_tx, live) = broadcast::channel(64);
        let (events, rx) = mpsc::unbounded_channel();
        forward_jail_terminal_events(rx, Arc::clone(&capture), stdout_tx);
        JailedTerminal {
            capture,
            events,
            _live: live,
        }
    }

    fn reports(&self, event: HostTerminalEvent) {
        self.events.send(event).expect("the forwarder is running");
    }

    /// Wait until the forwarder has taken `total` output bytes into the capture.
    async fn has_captured(&self, total: usize) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while self.capture.lock().unwrap().end_offset() < total as u64 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the forwarder fell behind"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

#[tokio::test]
async fn a_jailed_terminal_snapshots_at_the_size_the_jail_reported() {
    // Given a jail that opened at 100x30, drew, was resized to 140x40 and repainted
    let jailed = JailedTerminal::started();
    jailed.reports(HostTerminalEvent::Resized {
        cols: 100,
        rows: 30,
    });
    jailed.reports(HostTerminalEvent::Output(Bytes::from_static(STARTUP)));
    jailed.reports(HostTerminalEvent::Resized {
        cols: 140,
        rows: 40,
    });
    jailed.reports(HostTerminalEvent::Output(Bytes::from_static(REPAINT)));
    jailed.has_captured(STARTUP.len() + REPAINT.len()).await;

    // When the daemon snapshots it
    let snapshot = jailed
        .capture
        .lock()
        .unwrap()
        .snapshot()
        .expect("a jail that reported its size has a snapshot");

    // Then the snapshot is at the jail's current grid and renders what the jail's screen shows
    assert_eq!((snapshot.rows, snapshot.cols), (40, 140));
    let mut shown = vt100::Parser::new(snapshot.rows, snapshot.cols, 0);
    shown.process(&snapshot.data);
    let mut jail_screen = vt100::Parser::new(40, 140, 0);
    jail_screen.process(STARTUP);
    jail_screen.process(REPAINT);
    assert_eq!(shown.screen().contents(), jail_screen.screen().contents());
}

#[tokio::test]
async fn a_jailed_terminal_that_never_reported_its_size_has_no_snapshot() {
    // Given a jail whose runner never reported a size
    let jailed = JailedTerminal::started();
    jailed.reports(HostTerminalEvent::Output(Bytes::from_static(STARTUP)));
    jailed.has_captured(STARTUP.len()).await;

    // When the daemon is asked for a snapshot / Then there is none to give
    assert_eq!(jailed.capture.lock().unwrap().snapshot(), None);
}
