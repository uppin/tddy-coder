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
    Misconfigured { missing: Vec<String> },
    Connecting,
    Connected,
    /// Configured and not answering; `error` is the last failure.
    Unreachable { error: String },
}

/// One integration's entry on the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationStatus {
    pub id: String,
    pub label: String,
    pub state: IntegrationState,
    /// When the integration entered `state`. A report of the state it is already in leaves it.
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
    /// back the reporter it updates its entry with.
    pub fn reporter(self: &Arc<Self>, id: &str, label: &str) -> IntegrationReporter {
        self.lock().insert(
            id.to_string(),
            IntegrationStatus {
                id: id.to_string(),
                label: label.to_string(),
                state: IntegrationState::Disabled,
                since: SystemTime::now(),
            },
        );
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
    /// the moment it entered it.
    pub fn report(&self, state: IntegrationState) {
        let mut entries = self.board.lock();
        let entry = entries
            .get_mut(&self.id)
            .expect("a reporter's entry is registered when the reporter is made");
        if entry.state != state {
            entry.state = state;
            entry.since = SystemTime::now();
        }
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
    fn a_change_of_state_moves_when_it_began() {
        // Given an integration that became connected
        let board = IntegrationStatusBoard::new();
        let livekit = board.reporter("livekit", "LiveKit");
        livekit.report(IntegrationState::Connected);
        let connected_since = since_of(&board, "livekit");

        // When it loses its server — a clock tick later, since a state entered within the same
        // tick as the last would be dated identically on a coarse clock
        std::thread::sleep(std::time::Duration::from_millis(2));
        livekit.report(IntegrationState::Unreachable {
            error: "connection reset".to_string(),
        });

        // Then the unreachable state is dated from that report. `>`: the clock's value is the
        // runtime's, and only its order relative to the earlier report is known.
        assert!(
            since_of(&board, "livekit") > connected_since,
            "a new state must be dated from when it was entered"
        );
    }

    #[test]
    fn integrations_are_listed_by_id_and_report_independently() {
        // Given two integrations registered out of order
        let board = IntegrationStatusBoard::new();
        let telegram = board.reporter("telegram", "Telegram");
        let livekit = board.reporter("livekit", "LiveKit");

        // When only one of them reports
        livekit.report(IntegrationState::Connected);
        drop(telegram);

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
