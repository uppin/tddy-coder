//! The x11vnc process that serves an X11 display to the VNC bridge.
//!
//! An X11 desktop channel has no VNC server until one is opened. Opening it starts
//! `x11vnc -display :N -localhost -rfbport <free> -nopw -forever` as the daemon's user, waits until
//! that port accepts a connection, and points the bridge at `127.0.0.1:<port>`. The x11vnc is
//! stopped with its bridge: on `StopHostStream`, when the bridge exits, and before a reopen starts
//! another — so a display never has two.
//!
//! Loopback-only and password-less on purpose: nothing but this daemon's own bridge can reach it,
//! and the bridge signs in with VNC security type None.
//!
//! A module of its own rather than more of `screen_sharing_service.rs`, which is due to be split
//! (`#host-desktops` node 1); the host start/stop path calls in here, wherever it ends up living.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use tddy_rpc::Status;
use tokio::sync::Mutex;

/// How long an open waits for x11vnc to accept a connection when nothing else is configured.
///
/// x11vnc attaches to the display and binds its port in well under a second on a working host;
/// a display it cannot open makes it exit, and one that never accepts is reported, not waited on.
pub const DEFAULT_X11VNC_ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);

/// The arguments x11vnc is started with to serve display `:display` on loopback `rfb_port`.
#[must_use]
pub fn x11vnc_arguments(display: u32, rfb_port: u16) -> Vec<String> {
    // TODO(x11-channels): implement
    let _ = (display, rfb_port);
    todo!()
}

/// An x11vnc that is up and accepting on loopback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X11vncStarted {
    pub pid: u32,
    /// The loopback port the bridge connects to.
    pub port: u16,
}

/// The x11vnc processes this daemon has running, one per open X11 channel, keyed like the bridges
/// they serve (`host_bridge_key`).
pub struct X11vncServers {
    running: Mutex<HashMap<String, u32>>,
    accept_timeout: Duration,
}

impl Default for X11vncServers {
    fn default() -> Self {
        Self::new(DEFAULT_X11VNC_ACCEPT_TIMEOUT)
    }
}

impl X11vncServers {
    /// No x11vnc running; an open waits at most `accept_timeout` for one to accept.
    #[must_use]
    pub fn new(accept_timeout: Duration) -> Self {
        Self {
            running: Mutex::new(HashMap::new()),
            accept_timeout,
        }
    }

    /// Start `binary` serving `display` on a free loopback port under `key`, stopping whatever x11vnc
    /// `key` already had, and return once that port accepts a connection.
    ///
    /// An x11vnc that exits, or does not accept within the accept timeout, is stopped and fails the
    /// open (`Unavailable` / `DeadlineExceeded`), so no bridge is started for it.
    pub async fn start(
        &self,
        key: &str,
        binary: &Path,
        display: u32,
    ) -> Result<X11vncStarted, Status> {
        // TODO(x11-channels): implement
        let _ = (key, binary, display, &self.running, self.accept_timeout);
        todo!()
    }

    /// Stop the x11vnc running under `key`, if any.
    pub async fn stop(&self, key: &str) {
        // TODO(x11-channels): implement
        let _ = key;
        todo!()
    }
}
