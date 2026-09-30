//! Announcing a changed tool list to an MCP client that may not be connected yet.
//!
//! The session's roster is followed — and its first snapshot awaited — **before** the server
//! answers anything, so the first `tools/list` already reflects it. That means roster changes can
//! land before there is a client to tell: during the wait, and in the moment between the handshake
//! finishing and the peer being handed over. A change before serving starts needs no announcement —
//! the first `tools/list` reads the roster as it is by then. A change after serving starts but
//! before the peer is attached must not be lost, so it is remembered and announced on attach.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use rmcp::{Peer, RoleServer};

/// Sends `notifications/tools/list_changed` to the client once it is attached, and remembers a
/// change that arrived while it was not.
#[derive(Default)]
pub struct ToolListAnnouncer {
    peer: OnceLock<Peer<RoleServer>>,
    missed: AtomicBool,
}

impl ToolListAnnouncer {
    /// The tool list changed: tell the client, or remember to once there is one.
    pub fn announce(&self) {
        if let Some(peer) = self.peer.get() {
            notify(peer.clone());
            return;
        }
        self.missed.store(true, Ordering::SeqCst);
        // The peer may have been attached between the check above and the store: `attach` then
        // read `missed` before it was set, so the announcement is this call's to make.
        if let Some(peer) = self.peer.get() {
            if self.missed.swap(false, Ordering::SeqCst) {
                notify(peer.clone());
            }
        }
    }

    /// Serving is about to start: every change so far is visible to the client's first
    /// `tools/list`, so none of them is announced.
    pub fn serving_starts(&self) {
        self.missed.store(false, Ordering::SeqCst);
    }

    /// The handshake is done and `peer` can be notified. A change since serving started is
    /// announced now — the client may have listed its tools before it landed.
    pub fn attach(&self, peer: Peer<RoleServer>) {
        let _ = self.peer.set(peer);
        if self.missed.swap(false, Ordering::SeqCst) {
            if let Some(peer) = self.peer.get() {
                notify(peer.clone());
            }
        }
    }
}

fn notify(peer: Peer<RoleServer>) {
    tokio::spawn(async move {
        if let Err(e) = peer.notify_tool_list_changed().await {
            log::warn!(
                target: "tddy_tools::session_agents",
                "the roster changed but tools/list_changed could not be sent: {e}"
            );
        }
    });
}
