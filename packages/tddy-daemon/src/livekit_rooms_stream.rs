//! The `StreamLiveKitRooms` poll/diff engine.
//!
//! LiveKit's server API has no change feed, so the daemon polls it and turns consecutive rosters
//! into the change events the web folds onto its snapshot. Keeping the diff as a pure function over
//! two roster slices is what lets "a tick with no delta emits nothing" be a unit test rather than a
//! timing test against a live server.
//!
//! Feature: `docs/ft/web/livekit-rooms-panel.md`
//! Changeset: `docs/dev/1-WIP/livekit-rooms-panel.md`

use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tddy_livekit::room_roster::LiveKitRoomRoster;
use tddy_rpc::Status;
use tddy_service::proto::connection::{
    live_kit_rooms_change::Change, live_kit_rooms_event, LiveKitParticipantInfo,
    LiveKitParticipantJoined, LiveKitParticipantLeft, LiveKitParticipantMetadataChanged,
    LiveKitParticipantStateChanged, LiveKitRoomAdded, LiveKitRoomInfo, LiveKitRoomRemoved,
    LiveKitRoomsChange, LiveKitRoomsEvent, LiveKitRoomsSnapshot,
};
use tokio::sync::mpsc::UnboundedSender;

/// Why a roster read produced no rooms.
///
/// The two cases are different problems for whoever is looking at the panel: one is this daemon's
/// configuration, the other is the LiveKit server or the path to it — so they reach the subscriber
/// as different status codes rather than as one opaque `internal`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RosterError {
    /// This daemon cannot address a LiveKit server API at all — credentials missing, or a
    /// `livekit.url` that is not a WebSocket address. A deployment gap, not a fault of the server.
    Unconfigured(String),
    /// The server API was addressed and the read failed, timed out, or answered with an error.
    ReadFailed(String),
}

impl From<RosterError> for Status {
    fn from(err: RosterError) -> Self {
        match err {
            // A configuration gap is a precondition the caller's daemon has not met: retrying the
            // subscription changes nothing until the operator configures LiveKit.
            RosterError::Unconfigured(reason) => Status::failed_precondition(reason),
            RosterError::ReadFailed(reason) => Status::internal(reason),
        }
    }
}

/// Reads the current rooms and their participants from a LiveKit server.
///
/// A trait so the handler's tests can drive a scripted roster sequence without a LiveKit server,
/// mirroring how `HostStats` is injected for `StreamHostStats`.
#[async_trait]
pub trait RoomRoster: Send + Sync {
    /// Every room on the server with its participants, in whatever order the server reports.
    async fn list_rooms(&self) -> Result<Vec<LiveKitRoomInfo>, RosterError>;
}

/// The production roster: LiveKit's server API, reached over the daemon's own `livekit.url`.
#[async_trait]
impl RoomRoster for tddy_livekit::room_roster::LiveKitRoomRoster {
    async fn list_rooms(&self) -> Result<Vec<LiveKitRoomInfo>, RosterError> {
        Self::list_rooms(self)
            .await
            .map_err(RosterError::ReadFailed)
    }
}

/// The roster of a daemon with no LiveKit credentials configured: it reports why it cannot answer,
/// so a subscriber sees the configuration gap instead of an empty room list that would read as
/// "the server has no rooms".
pub struct UnconfiguredRoomRoster {
    reason: String,
}

impl UnconfiguredRoomRoster {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl RoomRoster for UnconfiguredRoomRoster {
    async fn list_rooms(&self) -> Result<Vec<LiveKitRoomInfo>, RosterError> {
        Err(RosterError::Unconfigured(self.reason.clone()))
    }
}

/// The roster a daemon reads from its own configuration.
///
/// The server API is reached over `livekit.url` — the daemon's own address for the server — rather
/// than `public_url`, which is the browser-facing one. A daemon missing any of the three settings,
/// or carrying a URL that is not a WebSocket address, gets a roster that reports exactly that.
pub fn room_roster_from_config(
    livekit: Option<&crate::config::LiveKitConfig>,
) -> Arc<dyn RoomRoster> {
    match configured_roster(livekit) {
        Ok(roster) => Arc::new(roster),
        Err(reason) => Arc::new(UnconfiguredRoomRoster::new(reason)),
    }
}

/// The configured server-API reader, or why this daemon has none.
fn configured_roster(
    livekit: Option<&crate::config::LiveKitConfig>,
) -> Result<LiveKitRoomRoster, String> {
    let credentials = livekit.and_then(|lk| {
        Some((
            lk.url.as_deref()?,
            lk.api_key.as_deref()?,
            lk.api_secret.as_deref()?,
        ))
    });
    let Some((url, api_key, api_secret)) = credentials else {
        return Err(
            "this daemon has no livekit url, api_key and api_secret configured, so it cannot read \
             the LiveKit server's rooms"
                .to_string(),
        );
    };
    LiveKitRoomRoster::from_ws_url(url, api_key, api_secret).map_err(|err| err.to_string())
}

/// Feed one `StreamLiveKitRooms` subscriber: a snapshot of the roster, then one change event per
/// delta found by re-reading it on `poll_interval`.
///
/// Each read is diffed against the state **this** subscriber was last sent, so two watchers cannot
/// consume each other's deltas. Returns when the subscriber is gone, or with the error a failed
/// read produced — an empty room list would read to the panel as "the server has no rooms".
pub async fn pump_rooms(
    roster: Arc<dyn RoomRoster>,
    poll_interval: Duration,
    tx: UnboundedSender<Result<LiveKitRoomsEvent, Status>>,
) {
    // The state this stream has been told about; every later read is diffed against it.
    let mut sent = match roster.list_rooms().await {
        Ok(rooms) => rooms,
        Err(err) => {
            let _ = tx.send(Err(err.into()));
            return;
        }
    };
    let snapshot = LiveKitRoomsEvent {
        event: Some(live_kit_rooms_event::Event::Snapshot(
            LiveKitRoomsSnapshot {
                rooms: sent.clone(),
            },
        )),
    };
    if tx.send(Ok(snapshot)).is_err() {
        return;
    }

    // First tick fires after one full period, so a tick provably reflects a fresh read.
    let mut poll =
        tokio::time::interval_at(tokio::time::Instant::now() + poll_interval, poll_interval);
    // A read slower than the cadence must not queue up the ticks it outlasted: bursting them would
    // fire another `1 + rooms` server-API calls back to back at exactly the moment the server is
    // already struggling. Skipping them keeps the cost of a slow server flat.
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            // An idle roster sends nothing, so a send failure would never be reached to notice the
            // subscriber leaving — without this the poll loop would outlive its stream forever.
            _ = tx.closed() => return,
            _ = poll.tick() => {}
        }
        let polled = match roster.list_rooms().await {
            Ok(rooms) => rooms,
            Err(err) => {
                let _ = tx.send(Err(err.into()));
                return;
            }
        };
        for change in diff_rosters(&sent, &polled) {
            let event = LiveKitRoomsEvent {
                event: Some(live_kit_rooms_event::Event::Change(change)),
            };
            if tx.send(Ok(event)).is_err() {
                return;
            }
        }
        sent = polled;
    }
}

/// Every delta between the roster last sent on a stream and the one just polled, as one change event
/// each.
///
/// Emits, per room: `room_added` for a room in `next` but not `prev` (carrying its participants, so
/// a consumer never infers a room from a partial event), `room_removed` for the converse, and for a
/// room in both — `participant_joined`, `participant_left`, `participant_metadata_changed` and
/// `participant_state_changed` for each participant delta. Returns an empty vector when the two
/// rosters describe the same state, which is what keeps an idle server producing an idle stream.
///
/// Rooms are keyed by name and participants by identity, so the order the server happens to report
/// them in is not a delta. A participant present in both readings is compared on both its metadata
/// and its server state, and a tick that moved both carries one event for each.
pub fn diff_rosters(prev: &[LiveKitRoomInfo], next: &[LiveKitRoomInfo]) -> Vec<LiveKitRoomsChange> {
    let before = by_room_name(prev);
    let after = by_room_name(next);

    let mut changes = Vec::new();
    for (name, room) in &after {
        match before.get(name) {
            // A room the stream has not announced yet: the event carries the whole row, so a
            // consumer never has to infer a room from a partial event.
            None => changes.push(as_event(Change::RoomAdded(LiveKitRoomAdded {
                room: Some((*room).clone()),
            }))),
            Some(known) => changes.extend(participant_changes(name, known, room)),
        }
    }
    for name in before.keys().filter(|name| !after.contains_key(*name)) {
        changes.push(as_event(Change::RoomRemoved(LiveKitRoomRemoved {
            room: (*name).to_string(),
        })));
    }
    changes
}

/// Every participant delta within one room known to both rosters.
fn participant_changes(
    room: &str,
    prev: &LiveKitRoomInfo,
    next: &LiveKitRoomInfo,
) -> Vec<LiveKitRoomsChange> {
    let before = by_identity(&prev.participants);
    let after = by_identity(&next.participants);

    let mut changes = Vec::new();
    for (identity, participant) in &after {
        match before.get(identity) {
            None => changes.push(as_event(Change::ParticipantJoined(
                LiveKitParticipantJoined {
                    room: room.to_string(),
                    participant: Some((*participant).clone()),
                },
            ))),
            Some(known) => changes.extend(republished_facts(room, identity, known, participant)),
        }
    }
    for identity in before.keys().filter(|id| !after.contains_key(*id)) {
        changes.push(as_event(Change::ParticipantLeft(LiveKitParticipantLeft {
            room: room.to_string(),
            identity: (*identity).to_string(),
        })));
    }
    changes
}

/// What a participant present in both readings now reports differently: its metadata, its server
/// state, or neither.
///
/// A tick that moved both carries **two** events, not one combined frame. The stream's contract is
/// one delta per event, so each fact travels on the event named for it — a consumer folding only the
/// kinds it understands still receives the other, and neither is hidden behind the other's arrival.
fn republished_facts(
    room: &str,
    identity: &str,
    prev: &LiveKitParticipantInfo,
    next: &LiveKitParticipantInfo,
) -> Vec<LiveKitRoomsChange> {
    let mut changes = Vec::new();
    if prev.metadata != next.metadata {
        changes.push(as_event(Change::ParticipantMetadataChanged(
            LiveKitParticipantMetadataChanged {
                room: room.to_string(),
                identity: identity.to_string(),
                metadata: next.metadata.clone(),
            },
        )));
    }
    if prev.state != next.state {
        changes.push(as_event(Change::ParticipantStateChanged(
            LiveKitParticipantStateChanged {
                room: room.to_string(),
                identity: identity.to_string(),
                state: next.state.clone(),
            },
        )));
    }
    changes
}

/// Rooms keyed by name, so lookups are by identity rather than by position in the server's report.
fn by_room_name(rooms: &[LiveKitRoomInfo]) -> BTreeMap<&str, &LiveKitRoomInfo> {
    rooms.iter().map(|r| (r.name.as_str(), r)).collect()
}

/// Participants keyed by identity — the stable key; `name` may be empty.
fn by_identity(participants: &[LiveKitParticipantInfo]) -> BTreeMap<&str, &LiveKitParticipantInfo> {
    participants
        .iter()
        .map(|p| (p.identity.as_str(), p))
        .collect()
}

/// Wrap one delta as the single-change event the stream carries.
fn as_event(change: Change) -> LiveKitRoomsChange {
    LiveKitRoomsChange {
        change: Some(change),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_service::proto::connection::{live_kit_rooms_change::Change, LiveKitParticipantInfo};

    const COMMON_ROOM: &str = "livekit.common_room";
    const PRESENTER_ROOM: &str = "daemon-pr-stack-presenter-room-0001";

    fn a_participant(identity: &str, metadata: &str) -> LiveKitParticipantInfo {
        LiveKitParticipantInfo {
            identity: identity.to_string(),
            name: String::new(),
            metadata: metadata.to_string(),
            joined_at_ms: 1_786_431_600_000,
            state: "ACTIVE".to_string(),
        }
    }

    /// The same participant as [`a_participant`], reported by the server in `state`.
    fn a_participant_in_state(identity: &str, state: &str) -> LiveKitParticipantInfo {
        LiveKitParticipantInfo {
            state: state.to_string(),
            ..a_participant(identity, "")
        }
    }

    fn a_room(name: &str, participants: Vec<LiveKitParticipantInfo>) -> LiveKitRoomInfo {
        LiveKitRoomInfo {
            name: name.to_string(),
            created_at_ms: 1_786_429_800_000,
            participants,
            metadata: String::new(),
        }
    }

    /// A change's proto field name, so a test can assert which kinds a tick produced without
    /// destructuring every payload.
    fn kind_of(change: &LiveKitRoomsChange) -> &'static str {
        match change.change.as_ref().expect("change set") {
            Change::RoomAdded(_) => "room_added",
            Change::RoomRemoved(_) => "room_removed",
            Change::ParticipantJoined(_) => "participant_joined",
            Change::ParticipantLeft(_) => "participant_left",
            Change::ParticipantMetadataChanged(_) => "participant_metadata_changed",
            Change::ParticipantStateChanged(_) => "participant_state_changed",
        }
    }

    /// The single change a tick produced, so a test asserts on one delta rather than indexing a list.
    fn only_change(changes: Vec<LiveKitRoomsChange>) -> Change {
        assert_eq!(
            changes.len(),
            1,
            "expected exactly one change, got {changes:?}"
        );
        changes
            .into_iter()
            .next()
            .unwrap()
            .change
            .expect("change set")
    }

    #[test]
    fn emits_nothing_when_the_roster_did_not_move() {
        // Given the same roster twice
        let roster = vec![a_room(
            COMMON_ROOM,
            vec![a_participant("browser-alice", "")],
        )];

        // When
        let changes = diff_rosters(&roster, &roster);

        // Then an idle tick is silent
        assert_eq!(changes, vec![]);
    }

    #[test]
    fn emits_nothing_when_only_the_server_ordering_changed() {
        // Given the same two rooms reported in the opposite order
        let prev = vec![a_room(COMMON_ROOM, vec![]), a_room(PRESENTER_ROOM, vec![])];
        let next = vec![a_room(PRESENTER_ROOM, vec![]), a_room(COMMON_ROOM, vec![])];

        // When
        let changes = diff_rosters(&prev, &next);

        // Then ordering is not a fact the stream carries
        assert_eq!(changes, vec![]);
    }

    #[test]
    fn reports_a_known_participant_settling_from_joined_to_active() {
        // Given a participant the stream already announced, whose server state then settles once
        // its connection is up
        let prev = vec![a_room(
            COMMON_ROOM,
            vec![a_participant_in_state("browser-alice", "JOINED")],
        )];
        let next = vec![a_room(
            COMMON_ROOM,
            vec![a_participant_in_state("browser-alice", "ACTIVE")],
        )];

        // When
        let change = only_change(diff_rosters(&prev, &next));

        // Then the transition reaches the stream, which is the only way a subscriber that was
        // already watching ever learns of it
        match change {
            Change::ParticipantStateChanged(changed) => {
                assert_eq!(changed.room, COMMON_ROOM);
                assert_eq!(changed.identity, "browser-alice");
                assert_eq!(changed.state, "ACTIVE");
            }
            other => panic!("expected ParticipantStateChanged, got {other:?}"),
        }
    }

    #[test]
    fn reports_metadata_and_state_as_two_events_when_both_move_on_one_tick() {
        // Given a participant that both settles and republishes between two reads
        let prev = vec![a_room(
            COMMON_ROOM,
            vec![LiveKitParticipantInfo {
                state: "JOINED".to_string(),
                ..a_participant("workstation", r#"{"owned_project_count":3}"#)
            }],
        )];
        let next = vec![a_room(
            COMMON_ROOM,
            vec![LiveKitParticipantInfo {
                state: "ACTIVE".to_string(),
                ..a_participant("workstation", r#"{"owned_project_count":7}"#)
            }],
        )];

        // When
        let changes = diff_rosters(&prev, &next);

        // Then each fact is its own event — the stream's contract is one delta per event, so a
        // consumer that folds only the kinds it understands still gets the other one
        let kinds: Vec<&str> = changes.iter().map(kind_of).collect();
        assert_eq!(
            kinds,
            vec!["participant_metadata_changed", "participant_state_changed"],
            "got {changes:?}"
        );
    }

    #[test]
    fn emits_nothing_when_a_participant_is_re_reported_with_the_same_metadata_and_state() {
        // Given two separate readings that describe the same participant identically
        let participant = || LiveKitParticipantInfo {
            state: "JOINED".to_string(),
            ..a_participant("workstation", r#"{"owned_project_count":3}"#)
        };
        let prev = vec![a_room(COMMON_ROOM, vec![participant()])];
        let next = vec![a_room(COMMON_ROOM, vec![participant()])];

        // When
        let changes = diff_rosters(&prev, &next);

        // Then re-reporting a participant is not a delta
        assert_eq!(changes, vec![]);
    }

    #[test]
    fn reports_a_new_room_with_the_participants_already_in_it() {
        // Given a server that gains an already-occupied presenter room
        let prev = vec![a_room(COMMON_ROOM, vec![])];
        let next = vec![
            a_room(COMMON_ROOM, vec![]),
            a_room(
                PRESENTER_ROOM,
                vec![a_participant("daemon-local-sess-1", "")],
            ),
        ];

        // When
        let change = only_change(diff_rosters(&prev, &next));

        // Then the event carries the whole row, occupant included
        match change {
            Change::RoomAdded(added) => {
                let room = added.room.expect("room set");
                assert_eq!(room.name, PRESENTER_ROOM);
                assert_eq!(
                    room.participants
                        .iter()
                        .map(|p| p.identity.as_str())
                        .collect::<Vec<_>>(),
                    vec!["daemon-local-sess-1"]
                );
            }
            other => panic!("expected RoomAdded, got {other:?}"),
        }
    }

    #[test]
    fn reports_a_closed_room() {
        // Given a server that loses the presenter room
        let prev = vec![a_room(COMMON_ROOM, vec![]), a_room(PRESENTER_ROOM, vec![])];
        let next = vec![a_room(COMMON_ROOM, vec![])];

        // When
        let change = only_change(diff_rosters(&prev, &next));

        // Then
        match change {
            Change::RoomRemoved(removed) => assert_eq!(removed.room, PRESENTER_ROOM),
            other => panic!("expected RoomRemoved, got {other:?}"),
        }
    }

    #[test]
    fn reports_a_participant_that_joined_a_known_room() {
        // Given a known room that gains an occupant
        let prev = vec![a_room(
            COMMON_ROOM,
            vec![a_participant("browser-alice", "")],
        )];
        let next = vec![a_room(
            COMMON_ROOM,
            vec![
                a_participant("browser-alice", ""),
                a_participant("browser-bob", ""),
            ],
        )];

        // When
        let change = only_change(diff_rosters(&prev, &next));

        // Then
        match change {
            Change::ParticipantJoined(joined) => {
                assert_eq!(joined.room, COMMON_ROOM);
                assert_eq!(
                    joined.participant.expect("participant set").identity,
                    "browser-bob"
                );
            }
            other => panic!("expected ParticipantJoined, got {other:?}"),
        }
    }

    #[test]
    fn reports_a_participant_that_left_a_known_room() {
        // Given a known room that loses an occupant
        let prev = vec![a_room(
            COMMON_ROOM,
            vec![
                a_participant("browser-alice", ""),
                a_participant("browser-bob", ""),
            ],
        )];
        let next = vec![a_room(
            COMMON_ROOM,
            vec![a_participant("browser-alice", "")],
        )];

        // When
        let change = only_change(diff_rosters(&prev, &next));

        // Then
        match change {
            Change::ParticipantLeft(left) => {
                assert_eq!(left.room, COMMON_ROOM);
                assert_eq!(left.identity, "browser-bob");
            }
            other => panic!("expected ParticipantLeft, got {other:?}"),
        }
    }

    #[test]
    fn reports_republished_participant_metadata() {
        // Given a participant whose metadata changes in place
        let prev = vec![a_room(
            COMMON_ROOM,
            vec![a_participant("workstation", r#"{"owned_project_count":3}"#)],
        )];
        let next = vec![a_room(
            COMMON_ROOM,
            vec![a_participant("workstation", r#"{"owned_project_count":7}"#)],
        )];

        // When
        let change = only_change(diff_rosters(&prev, &next));

        // Then
        match change {
            Change::ParticipantMetadataChanged(changed) => {
                assert_eq!(changed.room, COMMON_ROOM);
                assert_eq!(changed.identity, "workstation");
                assert_eq!(changed.metadata, r#"{"owned_project_count":7}"#);
            }
            other => panic!("expected ParticipantMetadataChanged, got {other:?}"),
        }
    }

    #[test]
    fn emits_one_event_per_delta_when_a_tick_carries_several() {
        // Given a tick in which one participant leaves, another joins, and a room closes
        let prev = vec![
            a_room(
                COMMON_ROOM,
                vec![
                    a_participant("browser-alice", ""),
                    a_participant("browser-bob", ""),
                ],
            ),
            a_room(PRESENTER_ROOM, vec![]),
        ];
        let next = vec![a_room(
            COMMON_ROOM,
            vec![
                a_participant("browser-alice", ""),
                a_participant("browser-carol", ""),
            ],
        )];

        // When
        let changes = diff_rosters(&prev, &next);

        // Then each delta is its own event — three of them, not one batched frame
        assert_eq!(changes.len(), 3, "got {changes:?}");
        let kinds: Vec<&str> = changes.iter().map(kind_of).collect();
        assert_eq!(
            kinds.iter().filter(|k| **k == "participant_joined").count(),
            1
        );
        assert_eq!(
            kinds.iter().filter(|k| **k == "participant_left").count(),
            1
        );
        assert_eq!(kinds.iter().filter(|k| **k == "room_removed").count(), 1);
    }

    #[test]
    fn reports_a_first_poll_as_room_additions() {
        // Given nothing sent yet and a server holding one room
        let next = vec![a_room(
            COMMON_ROOM,
            vec![a_participant("browser-alice", "")],
        )];

        // When
        let change = only_change(diff_rosters(&[], &next));

        // Then the empty baseline yields an addition, so the same function serves the first tick
        match change {
            Change::RoomAdded(added) => assert_eq!(added.room.expect("room set").name, COMMON_ROOM),
            other => panic!("expected RoomAdded, got {other:?}"),
        }
    }

    /// What a daemon missing any of the three LiveKit settings tells a subscriber. Spelled out here
    /// so a drift in the message is a test failure rather than a silent change to what an operator
    /// reads on the panel.
    const NO_CREDENTIALS: &str = "this daemon has no livekit url, api_key and api_secret \
                                  configured, so it cannot read the LiveKit server's rooms";

    /// A daemon's LiveKit block carrying everything the server API needs.
    fn a_livekit_config() -> crate::config::LiveKitConfig {
        crate::config::LiveKitConfig {
            url: Some("ws://127.0.0.1:7880".to_string()),
            api_key: Some("devkey".to_string()),
            api_secret: Some("secret".to_string()),
            ..Default::default()
        }
    }

    /// Why the roster built from `livekit` cannot read the server — panicking if it turns out it
    /// can, so a test never passes on a roster that would have talked to a real server.
    async fn refusal_of(livekit: Option<&crate::config::LiveKitConfig>) -> String {
        match room_roster_from_config(livekit).list_rooms().await {
            Err(RosterError::Unconfigured(reason)) => reason,
            other => panic!("expected an unconfigured roster, got {other:?}"),
        }
    }

    #[test]
    fn addresses_the_server_api_when_url_key_and_secret_are_all_configured() {
        // Given a daemon configured to reach its LiveKit server
        let livekit = a_livekit_config();

        // When it builds the roster behind the rooms feed
        let roster = configured_roster(Some(&livekit));

        // Then it is the real server-API reader, not one that reports a configuration gap
        assert!(
            roster.is_ok(),
            "expected a server-API roster, got {:?}",
            roster.as_ref().err()
        );
    }

    #[tokio::test]
    async fn refuses_to_read_a_server_this_daemon_has_no_livekit_settings_for() {
        // Given a daemon with no livekit block at all

        // When a subscriber's roster reads the server
        let refusal = refusal_of(None).await;

        // Then it reports the gap, rather than an empty list that would read as "no rooms"
        assert_eq!(refusal, NO_CREDENTIALS);
    }

    #[tokio::test]
    async fn refuses_to_read_when_the_livekit_url_is_missing() {
        // Given credentials with no server address to use them against
        let livekit = crate::config::LiveKitConfig {
            url: None,
            ..a_livekit_config()
        };

        // When a subscriber's roster reads the server
        let refusal = refusal_of(Some(&livekit)).await;

        // Then
        assert_eq!(refusal, NO_CREDENTIALS);
    }

    #[tokio::test]
    async fn refuses_to_read_when_the_api_key_is_missing() {
        // Given a configured server address the daemon cannot authenticate to
        let livekit = crate::config::LiveKitConfig {
            api_key: None,
            ..a_livekit_config()
        };

        // When a subscriber's roster reads the server
        let refusal = refusal_of(Some(&livekit)).await;

        // Then
        assert_eq!(refusal, NO_CREDENTIALS);
    }

    #[tokio::test]
    async fn refuses_to_read_when_the_api_secret_is_missing() {
        // Given an api key with no secret to sign with
        let livekit = crate::config::LiveKitConfig {
            api_secret: None,
            ..a_livekit_config()
        };

        // When a subscriber's roster reads the server
        let refusal = refusal_of(Some(&livekit)).await;

        // Then
        assert_eq!(refusal, NO_CREDENTIALS);
    }

    #[tokio::test]
    async fn refuses_to_read_a_livekit_url_that_is_not_a_websocket_address() {
        // Given an operator who configured the HTTP address by mistake
        let livekit = crate::config::LiveKitConfig {
            url: Some("http://127.0.0.1:7880".to_string()),
            ..a_livekit_config()
        };

        // When a subscriber's roster reads the server
        let refusal = refusal_of(Some(&livekit)).await;

        // Then the reason names the offending value instead of the generic missing-settings message
        assert_eq!(
            refusal,
            "livekit url is not a ws:// or wss:// address: http://127.0.0.1:7880"
        );
    }
}
