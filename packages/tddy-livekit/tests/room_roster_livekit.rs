//! Integration tests: `LiveKitRoomRoster` against a real LiveKit server API.
//!
//! What only a live server can pin is the mapping — that the fields `ListRooms` /
//! `ListParticipants` actually return land in the wire types the rooms panel renders. The panel's
//! arithmetic (what a roster *pair* turns into) is pinned separately by `diff_rosters`.
//!
//! Run: `cargo test -p tddy-livekit --test room_roster_livekit`
//! With shared kit: `eval $(./run-livekit-testkit-server | grep '^export ')` then the same command.
//!
//! Feature: `docs/ft/web/livekit-rooms-panel.md`

use anyhow::Result;
use livekit::prelude::*;
use livekit_api::access_token::{AccessToken, VideoGrants};
use serial_test::serial;
use std::time::Duration;
use tddy_livekit::room_roster::LiveKitRoomRoster;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_service::proto::connection::LiveKitRoomInfo;

/// The testkit server's dev credentials, as configured in `tddy-livekit-testkit`.
const API_KEY: &str = "devkey";
const API_SECRET: &str = "secret";

/// A participant's state settles from `JOINED` to `ACTIVE` only once ICE is up, which is a
/// server-side transition this test cannot drive directly — hence a bounded poll rather than a
/// single read.
const STATE_SETTLE_TIMEOUT: Duration = Duration::from_secs(15);
const STATE_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// A room name unique to this run, so a shared testkit container's other rooms cannot be mistaken
/// for this test's.
fn a_room_named(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    )
}

/// A join token carrying `metadata` from the moment of the join, so the participant's metadata is
/// never momentarily absent the way a post-join `set_metadata` would leave it.
fn a_join_token(room: &str, identity: &str, metadata: &str) -> Result<String> {
    Ok(AccessToken::with_api_key(API_KEY, API_SECRET)
        .with_identity(identity)
        .with_metadata(metadata)
        .with_ttl(Duration::from_secs(600))
        .with_grants(VideoGrants {
            room_join: true,
            room: room.to_string(),
            ..Default::default()
        })
        .to_jwt()?)
}

fn a_roster_for(ws_url: &str) -> LiveKitRoomRoster {
    LiveKitRoomRoster::from_ws_url(ws_url, API_KEY, API_SECRET).expect("roster for the testkit url")
}

/// The state the server reports for `identity` in `room` once it has settled, or the last state it
/// reported if it never does — so the assertion names what was actually observed.
///
/// Polls rather than reading once because the `JOINED` → `ACTIVE` transition happens server-side
/// when ICE comes up, which this test cannot drive.
async fn settled_state(roster: &LiveKitRoomRoster, room: &str, identity: &str) -> Result<String> {
    let deadline = tokio::time::Instant::now() + STATE_SETTLE_TIMEOUT;
    loop {
        let rooms = roster
            .list_rooms()
            .await
            .map_err(|err| anyhow::anyhow!("the roster reads the server API: {err}"))?;
        let state = reported_state(&rooms, room, identity)?;
        if state == "ACTIVE" || tokio::time::Instant::now() >= deadline {
            return Ok(state);
        }
        tokio::time::sleep(STATE_POLL_INTERVAL).await;
    }
}

/// One participant's state in one room of a roster reading, naming whichever of the two is missing
/// rather than reporting its absence as an empty state.
fn reported_state(rooms: &[LiveKitRoomInfo], room: &str, identity: &str) -> Result<String> {
    let found = rooms
        .iter()
        .find(|r| r.name == room)
        .ok_or_else(|| anyhow::anyhow!("room {room} missing from roster {rooms:?}"))?;
    let participant = found
        .participants
        .iter()
        .find(|p| p.identity == identity)
        .ok_or_else(|| anyhow::anyhow!("participant {identity} missing from room {found:?}"))?;
    Ok(participant.state.clone())
}

#[tokio::test]
#[serial]
async fn reports_a_joined_participant_under_its_room_with_the_metadata_it_published() -> Result<()>
{
    // Given a participant joined to a room, carrying metadata
    let livekit = LiveKitTestkit::start().await?;
    let room_name = a_room_named("roster-metadata");
    let token = a_join_token(&room_name, "browser-alice", r#"{"owned_project_count":3}"#)?;
    let (_room, _events) = Room::connect(&livekit.get_ws_url(), &token, RoomOptions::default())
        .await
        .map_err(|e| anyhow::anyhow!("browser-alice connect: {e}"))?;

    // When the daemon reads the server's roster
    let rooms = a_roster_for(&livekit.get_ws_url())
        .list_rooms()
        .await
        .expect("the roster reads the server API");

    // Then the room carries that one participant, identity and metadata relayed verbatim
    let room = rooms
        .iter()
        .find(|r| r.name == room_name)
        .unwrap_or_else(|| panic!("room {room_name} missing from roster {rooms:?}"));
    let identities: Vec<&str> = room
        .participants
        .iter()
        .map(|p| p.identity.as_str())
        .collect();
    assert_eq!(identities, vec!["browser-alice"]);
    assert_eq!(
        room.participants[0].metadata,
        r#"{"owned_project_count":3}"#
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn reports_a_connected_participant_as_active() -> Result<()> {
    // Given a participant connected to a room
    let livekit = LiveKitTestkit::start().await?;
    let room_name = a_room_named("roster-state");
    let token = a_join_token(&room_name, "browser-alice", "")?;
    let (_room, _events) = Room::connect(&livekit.get_ws_url(), &token, RoomOptions::default())
        .await
        .map_err(|e| anyhow::anyhow!("browser-alice connect: {e}"))?;
    let roster = a_roster_for(&livekit.get_ws_url());

    // When the daemon reads the server's roster once the connection has settled
    let state = settled_state(&roster, &room_name, "browser-alice").await?;

    // Then the server's state enum reaches the panel as the name the protocol gives it
    assert_eq!(state, "ACTIVE");
    Ok(())
}
