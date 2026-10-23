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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tddy_core::read_session_metadata;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::PROJECT_HOME_ROLE;
use tddy_daemon_kernel::user_paths::sessions_base_for_user;
use tddy_session_activity::session_reader;

use crate::launch_ports::LaunchSessions;

/// The hook-reported `activity_status` of a session whose turn is in flight.
const RUNNING: &str = "Running";
/// How long a stopped session's process may take to exit before the reaper stops waiting for it.
const EXIT_WAIT: Duration = Duration::from_secs(5);
/// How often the reaper checks whether a stopped session's process has exited.
const EXIT_POLL: Duration = Duration::from_millis(20);

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
        let idle_since = chrono::DateTime::parse_from_rfc3339(updated_at).ok()?;
        Some(Self {
            busy: activity_status == Some(RUNNING),
            idle_since: SystemTime::from(idle_since),
        })
    }
}

/// Whether a session observed as `obs` should be stopped at `now`. Never for a busy session, never
/// when `idle_stop` is zero.
pub fn should_stop(obs: &IdleObservation, now: SystemTime, idle_stop: Duration) -> bool {
    if obs.busy || idle_stop.is_zero() {
        return false;
    }
    now.duration_since(obs.idle_since)
        .is_ok_and(|idle| idle >= idle_stop)
}

impl LaunchSessions {
    /// Stop every running project-home session idle for at least the configured time at `now`.
    /// Returns the ids it stopped.
    pub async fn reap_idle_project_home_sessions(&self, now: SystemTime) -> Vec<String> {
        let idle_stop = Duration::from_secs(
            self.config
                .project_home
                .clone()
                .unwrap_or_default()
                .idle_stop_secs,
        );
        if idle_stop.is_zero() {
            return Vec::new();
        }
        let mut stopped = Vec::new();
        for sessions_base in self.session_bases() {
            let sessions = match session_reader::list_sessions_in_dir(&sessions_base) {
                Ok(sessions) => sessions,
                Err(e) => {
                    log::warn!(
                        "project-home auto-stop: listing {} failed: {e:#}",
                        sessions_base.display()
                    );
                    continue;
                }
            };
            for session in sessions {
                if !session.is_active || session.session_role != PROJECT_HOME_ROLE {
                    continue;
                }
                if self
                    .stop_if_idle(&sessions_base, &session.session_id, now, idle_stop)
                    .await
                {
                    stopped.push(session.session_id);
                }
            }
        }
        stopped
    }

    /// The sessions directory of every mapped OS user. With a data dir configured every user's is
    /// that one directory, so it is listed once.
    fn session_bases(&self) -> BTreeSet<PathBuf> {
        self.config
            .users
            .snapshot()
            .iter()
            .filter_map(|row| sessions_base_for_user(&row.os_user, Some(&self.tddy_data_dir)))
            .collect()
    }

    /// Stop `session_id` when it is idle at `now`. `true` only when it was stopped and its process
    /// has exited.
    async fn stop_if_idle(
        &self,
        sessions_base: &Path,
        session_id: &str,
        now: SystemTime,
        idle_stop: Duration,
    ) -> bool {
        let session_dir = unified_session_dir_path(sessions_base, session_id);
        let Ok(metadata) = read_session_metadata(&session_dir) else {
            return false;
        };
        let Some(obs) = IdleObservation::from_metadata(
            metadata.activity_status.as_deref(),
            &metadata.updated_at,
        ) else {
            return false;
        };
        if !should_stop(&obs, now, idle_stop) {
            return false;
        }
        let Some(pid) = metadata.pid else {
            return false;
        };
        log::info!("project-home auto-stop: stopping idle session {session_id} (pid {pid})");
        // The stop an explicit termination makes: the tracked terminal is cancelled (SIGTERM, then
        // SIGKILL to its child) and forgotten, so ensure no longer sees it as running. Nothing in
        // `.session.yaml` changes, so the next ensure resumes the same conversation.
        self.claude_cli_manager.stop_session(session_id).await;
        if exits_within(pid, EXIT_WAIT).await {
            return true;
        }
        log::warn!("project-home auto-stop: session {session_id} (pid {pid}) did not exit in time");
        false
    }
}

/// Whether `pid` is gone within `within`.
async fn exits_within(pid: u32, within: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + within;
    while process_alive(pid) {
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(EXIT_POLL).await;
    }
    true
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // Signal 0 sends nothing; it only asks whether the pid exists.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    false
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
