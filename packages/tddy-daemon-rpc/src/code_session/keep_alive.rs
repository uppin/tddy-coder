//! What keeps an open code session's index in use: the index-daemon process and the root's
//! language server are each let go after sitting unused for a while, and an open pane is use of
//! both — even one nobody has hovered in an hour (a forgotten tab keeps rust-analyzer resident
//! until it is closed; accepted for the pane never paying a cold reload under the reader).
//!
//! A heartbeat, at the period the channel's source asks for
//! ([`IndexChannelSource::keep_alive_every`]), tells the source the process is in use and warms the
//! root again when it holds a server and no warm of this session is under way. `code_index.Warm` is
//! idempotent — a loaded root is answered at once — and reaching the root's server is what the
//! index daemon counts as its use, so no new RPC is needed. A root that is idle or failed is not
//! warmed here: starting a load is [`Session::warm_again`]'s decision, not the heartbeat's.
//!
//! [`IndexChannelSource::keep_alive_every`]: crate::code_navigation::IndexChannelSource::keep_alive_every

use tddy_index_daemon::proto::code_index as index;
use tokio::time::{interval_at, Instant, Interval, MissedTickBehavior};

use super::{start_warm, Session};

impl Session {
    /// The session's heartbeat, first beating one period after it opened — opening has just warmed
    /// the root and dialled the process.
    pub(super) fn heartbeat(&self) -> Interval {
        let every = self.source.keep_alive_every();
        let mut heartbeat = interval_at(Instant::now() + every, every);
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        heartbeat
    }

    /// One beat: the index-daemon process is in use, and so is the root its server holds.
    pub(super) async fn keep_in_use(&mut self) {
        self.source.keep_alive().await;
        let holds_a_server = matches!(
            self.state,
            index::IndexState::Ready | index::IndexState::Loading
        );
        let warming = self
            .warming
            .as_ref()
            .is_some_and(|warm| !warm.is_finished());
        if holds_a_server && !warming {
            self.warming = start_warm(self.client.clone(), self.workspace_root.clone()).await;
        }
    }
}
