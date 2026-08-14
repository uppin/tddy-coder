//! Integration tests for the `StreamLiveKitRooms` RPC.
//!
//! The feed's emit logic — snapshot content and one-change-per-delta — is pinned as pure functions in
//! `tddy_daemon::livekit_rooms_stream::diff_rosters`, which is where the arithmetic lives. What only
//! an integration test can pin is the handler's contract with the transport: it authenticates before
//! it opens a stream, it turns a *sequence* of roster readings into a snapshot followed by changes,
//! silence, or an error, and it stops reading LiveKit once its subscriber is gone.
//!
//! Feature: `docs/ft/web/livekit-rooms-panel.md`
//! Reference: `packages/tddy-daemon/docs/connection-service.md` § LiveKit rooms

use async_trait::async_trait;
use futures_util::StreamExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_rooms_stream::{RoomRoster, RosterError};
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::connection::{
    live_kit_rooms_change::Change, live_kit_rooms_event::Event,
    ConnectionService as ConnectionServiceTrait, LiveKitParticipantInfo, LiveKitRoomInfo,
    LiveKitRoomsEvent, StreamLiveKitRoomsRequest,
};
use tddy_testing_commons::wait::eventually;
use tempfile::TempDir;

const COMMON_ROOM: &str = "livekit.common_room";

/// Poll cadence for the tests: short enough that a tick lands inside the test's read timeout, far
/// below the production three seconds.
const TEST_POLL_INTERVAL: Duration = Duration::from_millis(20);
/// How long a test waits for a message it expects to arrive.
const EVENT_TIMEOUT: Duration = Duration::from_secs(1);
/// How long a test waits before concluding no message is coming. Several poll ticks — and the tests
/// that use it assert the roster was actually re-read inside the window, so a starved machine that
/// managed no tick fails rather than passing vacuously.
const SILENCE_WINDOW: Duration = Duration::from_millis(300);
/// How long a test gives the poll loop to notice its subscriber left and to finish whatever read was
/// already in flight when it did.
const TEARDOWN_WINDOW: Duration = Duration::from_millis(300);
/// Ceiling on waiting for the poll loop to prove it is running. A safety net, not a prediction: the
/// first tick is due after one 20 ms interval.
const FIRST_TICK_TIMEOUT: Duration = Duration::from_secs(5);

fn a_participant(identity: &str) -> LiveKitParticipantInfo {
    LiveKitParticipantInfo {
        identity: identity.to_string(),
        name: String::new(),
        metadata: String::new(),
        joined_at_ms: 1_786_431_600_000,
        state: "ACTIVE".to_string(),
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

/// A roster that hands out a scripted sequence of readings — one per poll — and keeps reporting the
/// last one once the script runs out, so a test spells out what the LiveKit server does on each
/// tick without running one. It counts its reads, which is how a test observes that the poll loop is
/// running, and that it stopped.
struct ScriptedRoster {
    readings: Vec<Result<Vec<LiveKitRoomInfo>, RosterError>>,
    reads: AtomicUsize,
}

impl ScriptedRoster {
    fn new() -> Self {
        Self {
            readings: Vec::new(),
            reads: AtomicUsize::new(0),
        }
    }

    fn reporting(mut self, rooms: Vec<LiveKitRoomInfo>) -> Self {
        self.readings.push(Ok(rooms));
        self
    }

    fn failing_with(mut self, error: &str) -> Self {
        self.readings
            .push(Err(RosterError::ReadFailed(error.to_string())));
        self
    }

    fn unconfigured_because(mut self, reason: &str) -> Self {
        self.readings
            .push(Err(RosterError::Unconfigured(reason.to_string())));
        self
    }

    /// How many times the server has been read so far.
    fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl RoomRoster for ScriptedRoster {
    async fn list_rooms(&self) -> Result<Vec<LiveKitRoomInfo>, RosterError> {
        let last = self.readings.len().checked_sub(1).expect(
            "a scripted roster needs at least one reading — add .reporting(...), \
             .failing_with(...) or .unconfigured_because(...)",
        );
        let nth = self.reads.fetch_add(1, Ordering::SeqCst);
        self.readings[nth.min(last)].clone()
    }
}

fn a_roster() -> ScriptedRoster {
    ScriptedRoster::new()
}

/// A daemon whose rooms feed reads a scripted roster on a cadence short enough to observe, holding
/// on to the temp dir its sessions live in — dropping that guard would delete the directory out from
/// under the test.
struct WatchingDaemon {
    service: ConnectionServiceImpl,
    roster: Arc<ScriptedRoster>,
    _sessions_dir: TempDir,
}

fn a_service_watching(roster: ScriptedRoster) -> WatchingDaemon {
    let sessions_dir = tempfile::tempdir().expect("temp dir");
    let roster = Arc::new(roster);
    WatchingDaemon {
        service: test_service(sessions_dir.path().to_path_buf())
            .with_room_roster(Arc::clone(&roster) as Arc<dyn RoomRoster>)
            .with_room_poll_interval(TEST_POLL_INTERVAL),
        roster,
        _sessions_dir: sessions_dir,
    }
}

impl WatchingDaemon {
    async fn subscribe(
        &self,
    ) -> impl futures_util::Stream<Item = Result<LiveKitRoomsEvent, Status>> + Unpin {
        self.service
            .stream_live_kit_rooms(Request::new(StreamLiveKitRoomsRequest {
                session_token: TEST_TOKEN.to_string(),
            }))
            .await
            .expect("a valid token opens the stream")
            .into_inner()
    }

    /// How many times the feed has read the LiveKit server so far.
    fn roster_reads(&self) -> usize {
        self.roster.reads()
    }

    /// Block until the feed has read the server more than `reads` times, so a test that goes on to
    /// assert about polling knows polling actually happened.
    async fn await_a_read_past(&self, reads: usize) {
        eventually(
            "the rooms feed polls the server",
            FIRST_TICK_TIMEOUT,
            || {
                let observed = self.roster_reads();
                match observed > reads {
                    true => Ok(()),
                    false => Err(format!("still at {observed} reads")),
                }
            },
        )
        .await;
    }

    /// The read count once `window` has passed — long enough for a read that was already in flight
    /// to finish, and for several further ticks to have been due.
    async fn reads_after(&self, window: Duration) -> usize {
        tokio::time::sleep(window).await;
        self.roster_reads()
    }
}

/// The next message on the stream, with a bounded wait so a missing message fails loudly rather
/// than hanging the test.
async fn next_event(
    stream: &mut (impl futures_util::Stream<Item = Result<LiveKitRoomsEvent, Status>> + Unpin),
) -> Result<LiveKitRoomsEvent, Status> {
    tokio::time::timeout(EVENT_TIMEOUT, stream.next())
        .await
        .expect("no rooms event arrived within the timeout")
        .expect("the rooms stream closed unexpectedly")
}

fn snapshot_rooms(event: Result<LiveKitRoomsEvent, Status>) -> Vec<LiveKitRoomInfo> {
    match event.expect("the stream yielded an error").event {
        Some(Event::Snapshot(snapshot)) => snapshot.rooms,
        other => panic!("expected a snapshot, got {other:?}"),
    }
}

fn change(event: Result<LiveKitRoomsEvent, Status>) -> Change {
    match event.expect("the stream yielded an error").event {
        Some(Event::Change(change)) => change.change.expect("change set"),
        other => panic!("expected a change, got {other:?}"),
    }
}

/// Acceptance: `StreamLiveKitRooms` refuses a token it cannot resolve to a user.
#[tokio::test]
async fn stream_livekit_rooms_rejects_an_invalid_session_token() {
    // Given a daemon and a token it never issued
    let temp = tempfile::tempdir().unwrap();
    let service = test_service(temp.path().to_path_buf());

    // When a caller subscribes with it
    let result = service
        .stream_live_kit_rooms(Request::new(StreamLiveKitRoomsRequest {
            session_token: "not-a-real-token".to_string(),
        }))
        .await;

    // Then the subscription is refused as unauthenticated — the room roster is not public
    let status = result.expect_err("invalid token must not open a stream");
    assert_eq!(
        status.code(),
        Code::Unauthenticated,
        "invalid token must yield unauthenticated status, got {status:?}"
    );
}

#[tokio::test]
async fn opens_with_a_snapshot_of_every_room_on_the_server() {
    // Given a server holding one occupied room
    let daemon = a_service_watching(a_roster().reporting(vec![a_room(
        COMMON_ROOM,
        vec![a_participant("browser-alice")],
    )]));

    // When a caller subscribes
    let mut stream = daemon.subscribe().await;

    // Then the first message is the whole roster, so the panel populates without a second request
    let rooms = snapshot_rooms(next_event(&mut stream).await);
    assert_eq!(
        rooms,
        vec![a_room(COMMON_ROOM, vec![a_participant("browser-alice")])]
    );
}

#[tokio::test]
async fn follows_the_snapshot_with_one_change_per_delta() {
    // Given a server that gains an occupant on the tick after the snapshot
    let daemon = a_service_watching(
        a_roster()
            .reporting(vec![a_room(
                COMMON_ROOM,
                vec![a_participant("browser-alice")],
            )])
            .reporting(vec![a_room(
                COMMON_ROOM,
                vec![a_participant("browser-alice"), a_participant("browser-bob")],
            )]),
    );

    // When a caller subscribes and reads past the snapshot
    let mut stream = daemon.subscribe().await;
    let _snapshot = next_event(&mut stream).await;

    // Then the next message names exactly the participant that joined
    match change(next_event(&mut stream).await) {
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

#[tokio::test]
async fn stays_silent_on_a_tick_that_produced_no_delta() {
    // Given a server whose roster never moves
    let daemon = a_service_watching(a_roster().reporting(vec![a_room(
        COMMON_ROOM,
        vec![a_participant("browser-alice")],
    )]));

    // When a caller subscribes and the feed re-reads the server past the snapshot — waiting for the
    // read rather than for a duration, so the silence below is measured after a tick provably
    // happened rather than after a window a loaded machine might have slept through
    let mut stream = daemon.subscribe().await;
    let _snapshot = next_event(&mut stream).await;
    daemon.await_a_read_past(1).await;

    // Then an idle server produces an idle stream
    let after_the_snapshot = tokio::time::timeout(SILENCE_WINDOW, stream.next()).await;
    assert!(
        after_the_snapshot.is_err(),
        "expected silence, got {after_the_snapshot:?}"
    );
}

#[tokio::test]
async fn stops_reading_the_server_once_the_subscriber_is_gone() {
    // Given a subscriber being served by a poll loop that is provably running
    let daemon = a_service_watching(a_roster().reporting(vec![a_room(COMMON_ROOM, vec![])]));
    let mut stream = daemon.subscribe().await;
    let _snapshot = next_event(&mut stream).await;
    daemon.await_a_read_past(1).await;

    // When the subscriber goes away
    drop(stream);

    // Then the loop stops reading LiveKit rather than polling it for the life of the process
    let when_torn_down = daemon.reads_after(TEARDOWN_WINDOW).await;
    assert_eq!(
        daemon.reads_after(SILENCE_WINDOW).await,
        when_torn_down,
        "the poll loop kept reading the server after its subscriber left"
    );
}

#[tokio::test]
async fn ends_the_stream_with_the_error_when_the_roster_cannot_be_read() {
    // Given a LiveKit server API the daemon cannot reach
    let daemon = a_service_watching(a_roster().failing_with("livekit ListRooms failed: timeout"));

    // When a caller subscribes
    let mut stream = daemon.subscribe().await;

    // Then the stream carries that error rather than an empty room list, which would read as "the
    // server has no rooms"
    let status = next_event(&mut stream)
        .await
        .expect_err("an unreachable server must not yield a snapshot");
    assert_eq!(status.code(), Code::Internal);
    assert_eq!(status.message(), "livekit ListRooms failed: timeout");
}

#[tokio::test]
async fn ends_the_stream_with_the_error_when_a_later_poll_cannot_read_the_roster() {
    // Given a server that answers the first read and is unreachable by the next
    let daemon = a_service_watching(
        a_roster()
            .reporting(vec![a_room(COMMON_ROOM, vec![])])
            .failing_with("livekit ListRooms failed: connection refused"),
    );

    // When a caller subscribes and reads past the snapshot
    let mut stream = daemon.subscribe().await;
    let _snapshot = next_event(&mut stream).await;

    // Then the failed poll ends the stream with that error, so the panel says the roster went stale
    // instead of silently freezing on the last snapshot
    let status = next_event(&mut stream)
        .await
        .expect_err("a failed poll must not yield a change");
    assert_eq!(status.code(), Code::Internal);
    assert_eq!(
        status.message(),
        "livekit ListRooms failed: connection refused"
    );
}

#[tokio::test]
async fn reports_a_daemon_with_no_livekit_configuration_as_a_failed_precondition() {
    // Given a daemon that cannot address a LiveKit server API at all
    let daemon = a_service_watching(a_roster().unconfigured_because(
        "this daemon has no livekit url, api_key and api_secret configured, so it cannot read the \
         LiveKit server's rooms",
    ));

    // When a caller subscribes
    let mut stream = daemon.subscribe().await;

    // Then the configuration gap is classified as one — not as a fault of the server, which
    // retrying could fix
    let status = next_event(&mut stream)
        .await
        .expect_err("an unconfigured daemon must not yield a snapshot");
    assert_eq!(status.code(), Code::FailedPrecondition);
    assert_eq!(
        status.message(),
        "this daemon has no livekit url, api_key and api_secret configured, so it cannot read the \
         LiveKit server's rooms"
    );
}
