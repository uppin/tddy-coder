//! The sessions this daemon holds a live agent for, at one moment.
//!
//! A session is **active** if and only if the daemon holds at least one live agent tab for it — its
//! main agent or an agent terminal tab (PRD:
//! `docs/ft/daemon/1-WIP/PRD-2026-11-19-session-liveness-from-agent-tabs.md`). The pid a session's
//! `.session.yaml` records is never consulted: nothing clears it when the agent exits, and once the
//! OS reissues it to an unrelated process `kill(pid, 0)` reads a dead session as running.
//!
//! The holders — the terminal manager, the sandbox manager — each produce a [`LiveSessions`]; a
//! listing takes their [`LiveSessions::union`]. It is a snapshot: the listing that reads it runs on
//! the blocking pool, away from the holders' async locks.

use std::collections::HashSet;

/// A snapshot of the session ids with at least one live agent tab on this daemon.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveSessions {
    ids: HashSet<String>,
}

impl LiveSessions {
    /// No session is live — what a freshly started daemon holds.
    pub fn none() -> Self {
        Self::default()
    }

    /// Exactly `ids` are live.
    pub fn of<I, S>(ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            ids: ids.into_iter().map(Into::into).collect(),
        }
    }

    /// Whether `session_id` has a live agent tab.
    pub fn contains(&self, session_id: &str) -> bool {
        self.ids.contains(session_id)
    }

    /// The sessions live in either snapshot.
    pub fn union(mut self, other: LiveSessions) -> Self {
        self.ids.extend(other.ids);
        self
    }
}

/// What a daemon-side surface that is handed the daemon's processes — the Telegram controller —
/// reads and acts through: which sessions hold a live agent, and stopping one's processes.
#[async_trait::async_trait]
pub trait HeldSessions: Send + Sync {
    /// The sessions with at least one live agent tab right now.
    async fn live_sessions(&self) -> LiveSessions;

    /// Stop every process the daemon holds for `session_id`: its main agent and its tabs.
    async fn stop_session(&self, session_id: &str);
}

/// A daemon that holds no processes: nothing is live and there is nothing to stop. What a surface
/// built without the daemon's managers (every test fixture) is given.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHeldSessions;

#[async_trait::async_trait]
impl HeldSessions for NoHeldSessions {
    async fn live_sessions(&self) -> LiveSessions {
        LiveSessions::none()
    }

    async fn stop_session(&self, _session_id: &str) {}
}

/// A [`HeldSessions`] whose holder does not exist yet: a surface built before the daemon's session
/// host (Telegram, the remote-git service) is handed this, and the host is bound once it is built.
/// Until then nothing is live and there is nothing to stop — the same answer as [`NoHeldSessions`].
#[derive(Default)]
pub struct LateBoundHeldSessions {
    bound: std::sync::OnceLock<std::sync::Arc<dyn HeldSessions>>,
}

impl LateBoundHeldSessions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind the real holder. The first binding wins; a later one is ignored.
    pub fn bind(&self, holder: std::sync::Arc<dyn HeldSessions>) {
        let _ = self.bound.set(holder);
    }
}

#[async_trait::async_trait]
impl HeldSessions for LateBoundHeldSessions {
    async fn live_sessions(&self) -> LiveSessions {
        match self.bound.get() {
            Some(holder) => holder.live_sessions().await,
            None => LiveSessions::none(),
        }
    }

    async fn stop_session(&self, session_id: &str) {
        if let Some(holder) = self.bound.get() {
            holder.stop_session(session_id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Holding(&'static str);

    #[async_trait::async_trait]
    impl HeldSessions for Holding {
        async fn live_sessions(&self) -> LiveSessions {
            LiveSessions::of([self.0])
        }
        async fn stop_session(&self, _session_id: &str) {}
    }

    #[tokio::test]
    async fn a_late_bound_holder_reports_nothing_live_until_it_is_bound() {
        // Given
        let late = LateBoundHeldSessions::new();
        let before = late.live_sessions().await;

        // When
        late.bind(std::sync::Arc::new(Holding("jailed")));
        let after = late.live_sessions().await;

        // Then
        assert_eq!(
            (before, after),
            (LiveSessions::none(), LiveSessions::of(["jailed"]))
        );
    }

    #[test]
    fn a_fresh_daemon_holds_no_live_session() {
        // Given / When
        let live = LiveSessions::none();

        // Then
        assert!(!live.contains("any-session"));
    }

    #[test]
    fn only_the_named_sessions_are_live() {
        // Given / When
        let live = LiveSessions::of(["s-1", "s-2"]);

        // Then
        assert_eq!(
            (
                live.contains("s-1"),
                live.contains("s-2"),
                live.contains("s-3")
            ),
            (true, true, false)
        );
    }

    #[test]
    fn a_union_is_live_wherever_either_side_is() {
        // Given
        let terminals = LiveSessions::of(["pty-session"]);
        let sandboxes = LiveSessions::of(["jailed-session"]);

        // When
        let live = terminals.union(sandboxes);

        // Then
        assert_eq!(live, LiveSessions::of(["pty-session", "jailed-session"]));
    }
}
