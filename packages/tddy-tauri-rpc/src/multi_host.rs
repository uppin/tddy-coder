//! Many concurrent, independently addressed webview connections.
//!
//! [`WebviewRpcHost`](crate::WebviewRpcHost) hosts one service for a single webview at a time: it
//! holds `Option<Connection>`, and `connect` abandons whatever was there. That was right while a
//! page had exactly one thing to reach. It is what stops a page from holding a connection to the
//! daemon *and* one per attached session.
//!
//! This host keeps a **map** instead, keyed by the client epoch the page already stamps its frames
//! with. Everything else about the design survives unchanged, which is what makes the change
//! tractable:
//!
//! - the engine is already peer-keyed (`webview-{epoch}`), and releases a peer's state as a unit;
//! - backpressure is already per connection — one bounded queue and one drain task each;
//! - the epoch already routes frames, so there is **no frame-format change** and no protocol
//!   version to negotiate.
//!
//! What narrows is the meaning of [`FrameError::StaleConnection`](crate::FrameError): from "a page
//! that was replaced" to "a connection this host does not have".

use std::sync::Arc;

use crate::host::FrameSink;
use crate::target::{ConnectError, ConnectionTarget, RosterResolver};
use crate::FrameError;

/// Hosts many webview connections at once, each reaching the roster its target names.
pub struct MultiConnectionHost<R: RosterResolver> {
    _resolver: Arc<R>,
}

impl<R: RosterResolver> MultiConnectionHost<R> {
    /// Host connections whose targets `resolver` resolves.
    pub fn new(resolver: R) -> Self {
        Self {
            _resolver: Arc::new(resolver),
        }
    }

    /// Open a connection to `target`, publishing its responses onto `sink`.
    ///
    /// `client_epoch` is the connection's identity: the page mints one per transport and stamps
    /// every frame with it, so it is what routes a frame to the right connection.
    ///
    /// Opening a connection **does not disturb any other**. Calls in flight on the page's other
    /// connections are unaffected — which is the whole point, and the opposite of what
    /// `WebviewRpcHost::connect` does.
    pub async fn connect(
        &self,
        target: ConnectionTarget,
        sink: Arc<dyn FrameSink>,
        client_epoch: u32,
    ) -> Result<(), ConnectError> {
        let _ = (&target, &sink, client_epoch);
        // TODO(multi-connection-ipc): implement
        Err(ConnectError::NoSuchTarget { target })
    }

    /// Release the connection registered under `client_epoch`.
    ///
    /// Idempotent. The engine drops that peer's state, the forwards still publishing for it abort,
    /// and the sink is closed. **Every other connection keeps serving.**
    ///
    /// Sessions come and go far more often than pages do, so without this every attach would leak a
    /// host-side peer — a leak the single-slot host never had, because there was only ever one.
    pub async fn disconnect(&self, client_epoch: u32) {
        let _ = client_epoch;
        // TODO(multi-connection-ipc): implement
        unimplemented!("MultiConnectionHost::disconnect is not implemented yet")
    }

    /// Release **every** connection a page owned.
    ///
    /// A page reload used to reap the previous page's connection automatically, because there was
    /// one slot and `connect` overwrote it. With a map that is no longer implicit, and a leaked
    /// per-session connection on every reload is exactly what this exists to prevent.
    pub async fn disconnect_all(&self) {
        // TODO(multi-connection-ipc): implement
        unimplemented!("MultiConnectionHost::disconnect_all is not implemented yet")
    }

    /// Decode and dispatch one request frame, routing it by the epoch it carries.
    pub async fn handle_request_frame(&self, frame: &[u8]) -> Result<(), FrameError> {
        let _ = frame;
        // TODO(multi-connection-ipc): implement
        Err(FrameError::NotConnected)
    }

    /// How many connections are open. Diagnostics and tests.
    pub async fn connection_count(&self) -> usize {
        // TODO(multi-connection-ipc): implement
        unimplemented!("MultiConnectionHost::connection_count is not implemented yet")
    }
}
