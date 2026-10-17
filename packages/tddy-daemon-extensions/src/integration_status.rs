//! The live state of each optional integration, recorded in one place.
//!
//! Each integration is handed an [`IntegrationReporter`] at wiring time and reports into the
//! daemon's one [`IntegrationStatusBoard`]; `DaemonConfigService.GetIntegrationStatus` reads the
//! board. Nothing re-derives "is it configured, is it up" from configuration on its own.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Where an integration is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrationState {
    /// Switched off, or not configured at all.
    Disabled,
    /// Configured with required fields missing, named in configuration order.
    Misconfigured {
        missing: Vec<String>,
    },
    Connecting,
    Connected,
    /// Configured and not answering; `error` is the last failure.
    Unreachable {
        error: String,
    },
}

/// One integration's entry on the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationStatus {
    pub id: String,
    pub label: String,
    pub state: IntegrationState,
    /// When the integration entered `state`. A report of the same kind of state, whatever its
    /// detail, leaves it.
    pub since: SystemTime,
}

/// The daemon's integrations and their states.
#[derive(Debug, Default)]
pub struct IntegrationStatusBoard {
    entries: Mutex<BTreeMap<String, IntegrationStatus>>,
}

impl IntegrationStatusBoard {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Register integration `id` (displayed as `label`) as [`IntegrationState::Disabled`] and hand
    /// back the reporter it updates its entry with. An `id` already registered keeps its entry, so
    /// a second reporter for it never resets what the first has reported.
    pub fn reporter(self: &Arc<Self>, id: &str, label: &str) -> IntegrationReporter {
        self.lock()
            .entry(id.to_string())
            .or_insert_with(|| IntegrationStatus {
                id: id.to_string(),
                label: label.to_string(),
                state: IntegrationState::Disabled,
                since: SystemTime::now(),
            });
        IntegrationReporter {
            board: Arc::clone(self),
            id: id.to_string(),
        }
    }

    /// Every registered integration, ordered by id.
    pub fn snapshot(&self) -> Vec<IntegrationStatus> {
        self.lock().values().cloned().collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, IntegrationStatus>> {
        // A panic while holding the lock leaves a map of plain values, never a half-written one.
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// One integration's handle on its own board entry.
#[derive(Debug, Clone)]
pub struct IntegrationReporter {
    board: Arc<IntegrationStatusBoard>,
    id: String,
}

impl IntegrationReporter {
    /// Record `state` as the integration's current one. Reporting the state it is already in keeps
    /// the moment it entered it, and so does a report of the same kind of state with new detail:
    /// an `Unreachable` whose error text varies between retries is still the one outage.
    pub fn report(&self, state: IntegrationState) {
        let mut entries = self.board.lock();
        let entry = entries
            .get_mut(&self.id)
            .expect("a reporter's entry is registered when the reporter is made");
        if std::mem::discriminant(&entry.state) != std::mem::discriminant(&state) {
            entry.since = SystemTime::now();
        }
        entry.state = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The state `id` is in on `board`.
    fn state_of(board: &IntegrationStatusBoard, id: &str) -> IntegrationState {
        board
            .snapshot()
            .into_iter()
            .find(|integration| integration.id == id)
            .expect("the integration is registered")
            .state
    }

    fn since_of(board: &IntegrationStatusBoard, id: &str) -> SystemTime {
        board
            .snapshot()
            .into_iter()
            .find(|integration| integration.id == id)
            .expect("the integration is registered")
            .since
    }

    #[test]
    fn an_integration_is_disabled_until_it_reports_otherwise() {
        // Given a board
        let board = IntegrationStatusBoard::new();

        // When an integration registers and reports nothing
        board.reporter("livekit", "LiveKit");

        // Then it is listed, under its label, as disabled
        assert_eq!(
            board
                .snapshot()
                .into_iter()
                .map(|integration| (integration.id, integration.label, integration.state))
                .collect::<Vec<_>>(),
            vec![(
                "livekit".to_string(),
                "LiveKit".to_string(),
                IntegrationState::Disabled
            )]
        );
    }

    #[test]
    fn the_latest_report_is_the_integration_state() {
        // Given a registered integration that has been connecting
        let board = IntegrationStatusBoard::new();
        let livekit = board.reporter("livekit", "LiveKit");
        livekit.report(IntegrationState::Connecting);

        // When it reports that its server did not answer
        livekit.report(IntegrationState::Unreachable {
            error: "connection refused".to_string(),
        });

        // Then that is its state
        assert_eq!(
            state_of(&board, "livekit"),
            IntegrationState::Unreachable {
                error: "connection refused".to_string()
            }
        );
    }

    #[test]
    fn reporting_the_state_it_is_already_in_keeps_when_it_entered_it() {
        // Given an integration that became connected
        let board = IntegrationStatusBoard::new();
        let livekit = board.reporter("livekit", "LiveKit");
        livekit.report(IntegrationState::Connected);
        let connected_since = since_of(&board, "livekit");

        // When it reports connected again — every room it opens says so
        livekit.report(IntegrationState::Connected);

        // Then it has been connected since the first report, not the latest
        assert_eq!(since_of(&board, "livekit"), connected_since);
    }

    #[test]
    fn a_change_of_state_is_dated_from_when_it_was_entered() {
        // Given an integration that became connected
        let board = IntegrationStatusBoard::new();
        let livekit = board.reporter("livekit", "LiveKit");
        livekit.report(IntegrationState::Connected);
        let connected_since = since_of(&board, "livekit");

        // When it loses its server
        livekit.report(IntegrationState::Unreachable {
            error: "connection reset".to_string(),
        });

        // Then the unreachable state is dated no earlier than the connected one. `>=`, not `>`: a
        // coarse clock may date both reports alike, so only the order is known, never a gap.
        assert!(
            since_of(&board, "livekit") >= connected_since,
            "a new state must be dated from when it was entered, never before the state it replaced"
        );
    }

    #[test]
    fn a_retry_that_fails_differently_keeps_when_the_outage_began() {
        // Given an integration unreachable with one error
        let board = IntegrationStatusBoard::new();
        let livekit = board.reporter("livekit", "LiveKit");
        livekit.report(IntegrationState::Unreachable {
            error: "a".to_string(),
        });
        let unreachable_since = since_of(&board, "livekit");

        // When a retry fails with other error text
        livekit.report(IntegrationState::Unreachable {
            error: "b".to_string(),
        });

        // Then the newest error is shown, and the outage still dates from its first report
        assert_eq!(
            (state_of(&board, "livekit"), since_of(&board, "livekit")),
            (
                IntegrationState::Unreachable {
                    error: "b".to_string()
                },
                unreachable_since
            )
        );
    }

    #[test]
    fn a_reporter_made_twice_for_one_integration_keeps_what_was_reported() {
        // Given an integration that has reported connected
        let board = IntegrationStatusBoard::new();
        board
            .reporter("livekit", "LiveKit")
            .report(IntegrationState::Connected);

        // When a second reporter is made for it
        board.reporter("livekit", "LiveKit");

        // Then it is still connected
        assert_eq!(state_of(&board, "livekit"), IntegrationState::Connected);
    }

    #[test]
    fn integrations_are_listed_by_id_and_report_independently() {
        // Given two integrations registered out of order
        let board = IntegrationStatusBoard::new();
        let _telegram = board.reporter("telegram", "Telegram");
        let livekit = board.reporter("livekit", "LiveKit");

        // When only one of them reports
        livekit.report(IntegrationState::Connected);

        // Then both are listed, ordered by id, each in its own state
        assert_eq!(
            board
                .snapshot()
                .into_iter()
                .map(|integration| (integration.id, integration.state))
                .collect::<Vec<_>>(),
            vec![
                ("livekit".to_string(), IntegrationState::Connected),
                ("telegram".to_string(), IntegrationState::Disabled),
            ]
        );
    }
}
