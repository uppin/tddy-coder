//! Whether a root's crate graph is loaded, as the root's own server reports it.
//!
//! `Warm` used to answer "yes" the moment [`tddy_lsp::LspRegistry`] handed back a live server,
//! which is a weaker claim than the schema makes: measured on a real three-crate workspace, a warm
//! returned in 0.34s while the first `Anchors` on the same root — which does wait for the graph —
//! took 2.1s. So `ready` could be true while the next request still paid the whole load.
//!
//! rust-analyzer says when its graph is queryable: `experimental/serverStatus` with
//! `quiescent: true`, on the transition and **only** on the transition. That is the whole reason
//! this is a per-root latch fed by a watcher attached at spawn rather than something each request
//! reads for itself: a request arriving after the transition would wait for ever to be told
//! something the server has already said once, to somebody else.
//!
//! The watcher folds through [`ServerChatter`] rather than reading `quiescent` out of the JSON
//! itself, because there is one right way to read these two notifications and two readings of it
//! would drift.

use tddy_code_restructuring::backends::rust::ServerChatter;
use tddy_lsp::client::LspClient;
use tddy_lsp::{NotificationEvent, NotificationStream};
use tokio::sync::watch;

/// Whether one root's crate graph has been observed loaded, and a way to wait until it has.
///
/// Cloneable, and every clone reads the same latch: one watcher per server, any number of requests
/// following it.
#[derive(Clone)]
pub(crate) struct GraphLoad {
    loaded: watch::Receiver<bool>,
}

impl GraphLoad {
    /// Attach a watcher to `client` and return the latch it feeds.
    ///
    /// Called once per spawned server, from the host that spawned it — which is the only moment
    /// early enough to be sure of catching the transition. The watcher outlives every request: it
    /// ends when the client it watches is dropped, which is what makes a dropped sender mean "the
    /// server this was about is gone" rather than "nothing happened yet".
    pub(crate) fn watching(client: &LspClient) -> Self {
        let (observed, loaded) = watch::channel(false);
        let notifications = client.subscribe_notifications();
        tokio::spawn(fold_until_the_graph_is_loaded(notifications, observed));
        Self { loaded }
    }

    /// Whether the graph has been observed loaded.
    pub(crate) fn is_loaded(&self) -> bool {
        *self.loaded.borrow()
    }

    /// Whether the watcher feeding this latch is still attached.
    ///
    /// False once the server it watched is gone, which is what tells the host to attach a fresh
    /// watcher to a respawned server rather than reading a latch about a dead one.
    pub(crate) fn still_watching(&self) -> bool {
        self.loaded.has_changed().is_ok()
    }

    /// Wait until the graph is observed loaded, or until the server it belonged to is gone.
    ///
    /// Returns whether the graph is loaded. `false` is the server having gone away mid-load: the
    /// wait ends either way, because a wait that cannot end is worse than one that ends badly.
    pub(crate) async fn loaded(&mut self) -> bool {
        while !self.is_loaded() {
            if self.loaded.changed().await.is_err() {
                return false;
            }
        }
        true
    }
}

/// Fold `notifications` until the server reports its graph queryable, then stay attached.
///
/// A latch, never unset: a graph that has loaded does not unload, and a server going
/// non-quiescent to answer an edit is not a root that has to be warmed again. The fold stops
/// there, because there is nothing left to learn — but the task does **not**, because dropping
/// the sender is how this crate says "the server this was about is gone". A watcher that returned
/// on latching would make every later reader see a dead server and attach a fresh watcher to a
/// live one, which would then wait for a transition already made.
///
/// A watcher that falls behind is told how many notifications it lost, and it cannot know whether
/// the transition was among them. It therefore keeps reading rather than latching — claiming a
/// load on the strength of a gap is exactly the silent guess this latch exists instead of. The
/// loss is recorded, because a warm that then waits for ever has to be explicable.
async fn fold_until_the_graph_is_loaded(
    mut notifications: NotificationStream,
    observed: watch::Sender<bool>,
) {
    let mut chatter = ServerChatter::default();
    let mut loaded = false;
    loop {
        match notifications.recv().await {
            NotificationEvent::Received(notification) if !loaded => {
                chatter.absorb(&notification);
                loaded = chatter.quiescent();
                if loaded {
                    let _ = observed.send(true);
                }
            }
            // Read and dropped: a subscriber that stops reading only falls behind, and this one
            // has nothing left to learn from what it reads.
            NotificationEvent::Received(_) => {}
            NotificationEvent::Lost(lost) if !loaded => {
                log::debug!(
                    target: "tddy_index_daemon::graph",
                    "lost sight of {lost} notification(s) while watching a crate graph load; \
                     still waiting to be told it is queryable"
                );
            }
            NotificationEvent::Lost(_) => {}
            // The client is gone, so nothing will ever report this graph loaded. Dropping the
            // sender is the signal: every waiter learns the server went away instead of waiting on
            // a transition that can no longer happen.
            NotificationEvent::Ended => return,
        }
    }
}
