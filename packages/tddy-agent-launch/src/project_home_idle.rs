//! Auto-stop for idle project-home sessions (`#project-home` 6/7).
//!
//! A project-home session is **busy** while its hook-reported `activity_status` is `Running` (a
//! turn in flight); otherwise it is idle **since** its metadata's `updated_at`, which every status
//! report bumps — the end of its last turn, or the input that started one. Once idle for
//! `project_home.idle_stop_secs`, the reaper stops it through the normal stop path; its
//! `.session.yaml` keeps what resume needs, so `EnsureProjectHomeSession` resumes the same
//! conversation. Sessions without the `project-home` role are never touched; `0` disables it.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-22-project-home-auto-stop.md.

use std::time::{Duration, SystemTime};

use crate::launch_ports::LaunchSessions;

/// What the policy reads about one running project-home session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleObservation {
    /// A turn is in flight.
    pub busy: bool,
    /// When the session last reported a status change (turn end or input).
    pub idle_since: SystemTime,
}

impl IdleObservation {
    /// Build the observation from a session's hook-reported `activity_status` and `updated_at`
    /// (RFC 3339). `None` when `updated_at` does not parse — such a session is left alone.
    pub fn from_metadata(activity_status: Option<&str>, updated_at: &str) -> Option<Self> {
        // TODO(home-auto-stop): implement
        let _ = (activity_status, updated_at);
        None
    }
}

/// Whether a session observed as `obs` should be stopped at `now`. Never for a busy session, never
/// when `idle_stop` is zero.
pub fn should_stop(obs: &IdleObservation, now: SystemTime, idle_stop: Duration) -> bool {
    // TODO(home-auto-stop): implement
    let _ = (obs, now, idle_stop);
    false
}

impl LaunchSessions {
    /// Stop every running project-home session idle for at least the configured time at `now`.
    /// Returns the ids it stopped.
    pub async fn reap_idle_project_home_sessions(&self, now: SystemTime) -> Vec<String> {
        // TODO(home-auto-stop): implement
        let _ = now;
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEN_MINUTES: Duration = Duration::from_secs(600);

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn stops_only_when_idle_since_the_later_of_turn_end_and_input() {
        // Given sessions observed at various points, judged at t = 10_000
        let now = at(10_000);
        let cases = [
            (
                "idle for longer than the limit",
                IdleObservation {
                    busy: false,
                    idle_since: at(10_000 - 601),
                },
                true,
            ),
            (
                "idle for exactly the limit",
                IdleObservation {
                    busy: false,
                    idle_since: at(10_000 - 600),
                },
                true,
            ),
            (
                "idle, but input arrived recently",
                IdleObservation {
                    busy: false,
                    idle_since: at(10_000 - 30),
                },
                false,
            ),
            (
                "a turn in flight for a long time",
                IdleObservation {
                    busy: true,
                    idle_since: at(0),
                },
                false,
            ),
        ];

        // Then
        for (case, obs, expected) in cases {
            assert_eq!(should_stop(&obs, now, TEN_MINUTES), expected, "{case}");
        }
    }

    #[test]
    fn a_zero_limit_never_stops() {
        let obs = IdleObservation {
            busy: false,
            idle_since: at(0),
        };
        assert!(!should_stop(&obs, at(1_000_000), Duration::ZERO));
    }

    #[test]
    fn a_running_status_is_busy_and_any_other_is_idle_since_the_last_report() {
        let running = IdleObservation::from_metadata(Some("Running"), "2026-10-22T10:00:00Z");
        let waiting =
            IdleObservation::from_metadata(Some("WaitingForInput"), "2026-10-22T10:00:00Z");
        let since =
            SystemTime::from(chrono::DateTime::parse_from_rfc3339("2026-10-22T10:00:00Z").unwrap());
        assert_eq!(
            running,
            Some(IdleObservation {
                busy: true,
                idle_since: since
            })
        );
        assert_eq!(
            waiting,
            Some(IdleObservation {
                busy: false,
                idle_since: since
            })
        );
        assert_eq!(IdleObservation::from_metadata(None, "not a time"), None);
    }
}
