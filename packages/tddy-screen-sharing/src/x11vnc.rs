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
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use log::{info, warn};
use tddy_rpc::Status;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// How long an open waits for x11vnc to accept a connection when nothing else is configured.
///
/// x11vnc attaches to the display and binds its port in well under a second on a working host;
/// a display it cannot open makes it exit, and one that never accepts is reported, not waited on.
pub const DEFAULT_X11VNC_ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);

/// The address x11vnc listens on: it is started `-localhost`, so loopback is the only place it
/// accepts a connection.
pub const X11VNC_HOST: &str = "127.0.0.1";

/// How long to wait between connect attempts while x11vnc comes up.
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// The arguments x11vnc is started with to serve display `:display` on loopback `rfb_port`.
#[must_use]
pub fn x11vnc_arguments(display: u32, rfb_port: u16) -> Vec<String> {
    vec![
        "-display".to_string(),
        format!(":{display}"),
        "-localhost".to_string(),
        "-rfbport".to_string(),
        rfb_port.to_string(),
        "-nopw".to_string(),
        "-forever".to_string(),
    ]
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
    running: Arc<Mutex<HashMap<String, u32>>>,
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
            running: Arc::new(Mutex::new(HashMap::new())),
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
        // A reopen must not strand the first x11vnc: one pid per key, and a spawn that overwrote it
        // would leave a process nothing can ever signal.
        self.stop(key).await;

        let port = a_free_loopback_port()?;
        let mut child = Command::new(binary)
            .args(x11vnc_arguments(display, port))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                Status::internal(format!("spawning x11vnc '{}': {e}", binary.display()))
            })?;
        let pid = child.id().unwrap_or(0);
        if pid > 0 {
            self.running.lock().await.insert(key.to_string(), pid);
        }
        info!("x11vnc spawned: display=:{display} pid={pid} port={port} key={key}");

        if let Err(status) = self.await_accepting(&mut child, port, display).await {
            // Killed here, so no failed open returns with its x11vnc still running.
            if let Err(e) = child.kill().await {
                warn!("x11vnc stop after a failed start: key={key} err={e}");
            }
            forget(&self.running, key, pid).await;
            return Err(status);
        }

        reap(Arc::clone(&self.running), child, key.to_string(), pid);
        Ok(X11vncStarted { pid, port })
    }

    /// Stop the x11vnc running under `key`, if any.
    pub async fn stop(&self, key: &str) {
        let Some(pid) = self.running.lock().await.remove(key) else {
            return;
        };
        info!("stopping x11vnc: pid={pid} key={key}");
        #[cfg(unix)]
        // SAFETY: `pid` is a recorded child pid, never the zero sentinel (`start` records only
        // pids above zero), so the signal reaches that process alone and not our process group.
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }

    /// Wait until `port` accepts a connection, the x11vnc exits, or the accept timeout passes.
    async fn await_accepting(
        &self,
        child: &mut Child,
        port: u16,
        display: u32,
    ) -> Result<(), Status> {
        let accepting = async {
            loop {
                if tokio::net::TcpStream::connect((X11VNC_HOST, port))
                    .await
                    .is_ok()
                {
                    return;
                }
                tokio::time::sleep(ACCEPT_POLL_INTERVAL).await;
            }
        };
        tokio::select! {
            () = accepting => Ok(()),
            exited = child.wait() => Err(Status::unavailable(match exited {
                Ok(status) => format!("x11vnc for display :{display} exited before accepting: {status}"),
                Err(e) => format!("waiting for x11vnc for display :{display}: {e}"),
            })),
            () = tokio::time::sleep(self.accept_timeout) => Err(Status::deadline_exceeded(format!(
                "x11vnc for display :{display} did not accept a connection within {:?}",
                self.accept_timeout
            ))),
        }
    }
}

/// A loopback port nothing is bound to right now.
///
/// There is a window between releasing it and x11vnc binding it; x11vnc failing to bind makes it
/// exit, which the start reports.
fn a_free_loopback_port() -> Result<u16, Status> {
    std::net::TcpListener::bind((X11VNC_HOST, 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|e| Status::internal(format!("finding a free port for x11vnc: {e}")))
}

/// Background task: wait for the process to exit (prevents a zombie) and forget it when it does.
fn reap(running: Arc<Mutex<HashMap<String, u32>>>, mut child: Child, key: String, pid: u32) {
    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) => info!("x11vnc exited: key={key} status={status}"),
            Err(e) => warn!("x11vnc wait error: key={key} err={e}"),
        }
        forget(&running, &key, pid).await;
    });
}

/// Forget `key`'s pid — only while the key still names *this* x11vnc.
async fn forget(running: &Mutex<HashMap<String, u32>>, key: &str, pid: u32) {
    let mut running = running.lock().await;
    if running.get(key) == Some(&pid) {
        running.remove(key);
    }
}
