//! The two LiveKit primitives a session room is built from: a room-wide broadcast on its own topic,
//! and room metadata as a snapshot late joiners can read.
//!
//! Module docs: `packages/tddy-livekit/docs/broadcast-and-room-metadata.md`
//!
//! Everything the project publishes today is unicast on the single `tddy-rpc` topic, addressed with
//! `destination_identities`, and every receiver hard-filters on that topic. Both halves of that are
//! load-bearing here: a broadcast must reach participants the publisher never named, and it must not
//! disturb the RPC traffic sharing the same data channel.
//!
//! Needs the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`); `#[serial]` so these own
//! it alone.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use livekit::prelude::RoomOptions;
use livekit::Room;
use serial_test::serial;
use tddy_livekit::{
    BroadcastChannel, BroadcastError, BroadcastMessage, RoomMetadataClient,
    MAX_BROADCAST_PAYLOAD_BYTES,
};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_testing_commons::wait::eventually_awaiting;

/// The topic under test. Deliberately not `tddy-rpc`: proving a second topic can coexist is the
/// point, and reusing the RPC topic would prove nothing.
const TOPIC: &str = "worktree.activity";

/// A cold container has to accept every participant before the first packet moves.
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(20);

/// A room name no other run of this suite can have used.
///
/// The testkit container is deliberately long-lived and reused between runs (see
/// `./run-livekit-testkit-server`), so a fixed name meets whatever a previous run left behind — and
/// `room_metadata_written_before_a_participant_joins_is_readable_on_arrival` would quietly stop
/// testing a first write against a room that had never existed.
fn a_room_for(purpose: &str) -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock must be after 1970")
        .as_nanos();
    format!("{purpose}-{nonce}")
}

async fn a_participant(livekit: &LiveKitTestkit, room: &str, identity: &str) -> Result<Arc<Room>> {
    let token = livekit.generate_token(room, identity)?;
    let (joined, _events) = Room::connect(&livekit.get_ws_url(), &token, RoomOptions::default())
        .await
        .map_err(|e| anyhow::anyhow!("{identity} connect: {e}"))?;
    Ok(Arc::new(joined))
}

/// Waits for `identity` to appear in `room`'s participant list.
///
/// A broadcast reaches whoever is connected *at publish time*, so publishing before the far side has
/// been accepted is a message sent to an empty room — indistinguishable from a broken broadcast.
async fn wait_for_participant(room: &Room, identity: &str) {
    eventually_awaiting(
        &format!("{identity} to be accepted into the room"),
        DELIVERY_TIMEOUT,
        || async {
            let present: Vec<String> = room
                .remote_participants()
                .values()
                .map(|p| p.identity().to_string())
                .collect();
            present
                .iter()
                .any(|p| p == identity)
                .then_some(())
                .ok_or_else(|| format!("the room held {present:?}"))
        },
    )
    .await;
}

async fn next_message(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<BroadcastMessage>,
) -> Result<BroadcastMessage> {
    tokio::time::timeout(DELIVERY_TIMEOUT, rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("no broadcast arrived within {DELIVERY_TIMEOUT:?}"))?
        .ok_or_else(|| anyhow::anyhow!("the broadcast subscription closed"))
}

#[tokio::test]
#[serial]
async fn one_publish_reaches_every_participant_the_publisher_never_named() -> Result<()> {
    // Given a publisher and two subscribers, none of them known to the publisher
    let livekit = LiveKitTestkit::start().await?;
    let room = a_room_for("broadcast-reaches-everyone");
    let publisher = a_participant(&livekit, &room, "host").await?;
    let first = a_participant(&livekit, &room, "agent-one").await?;
    let second = a_participant(&livekit, &room, "agent-two").await?;

    let mut from_first = BroadcastChannel::new(first.clone(), TOPIC).subscribe();
    let mut from_second = BroadcastChannel::new(second.clone(), TOPIC).subscribe();
    wait_for_participant(&publisher, "agent-one").await;
    wait_for_participant(&publisher, "agent-two").await;

    // When the publisher broadcasts once
    BroadcastChannel::new(publisher, TOPIC)
        .publish(b"head moved")
        .await?;

    // Then both receive the same message, attributed to the publisher — one publish, not one per
    // participant the publisher would first have had to know about
    let to_first = next_message(&mut from_first).await?;
    let to_second = next_message(&mut from_second).await?;
    assert_eq!(to_first.payload, b"head moved");
    assert_eq!(to_first.from.as_deref(), Some("host"));
    assert_eq!(
        to_second, to_first,
        "a broadcast must deliver the identical message to every participant"
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn a_subscriber_ignores_traffic_on_other_topics() -> Result<()> {
    // Given a subscriber to one topic
    let livekit = LiveKitTestkit::start().await?;
    let room = a_room_for("broadcast-topic-isolation");
    let publisher = a_participant(&livekit, &room, "host").await?;
    let listener = a_participant(&livekit, &room, "agent").await?;
    let mut activity = BroadcastChannel::new(listener, TOPIC).subscribe();
    wait_for_participant(&publisher, "agent").await;

    // When a message on a different topic is published, followed by one on the subscribed topic
    BroadcastChannel::new(publisher.clone(), "some-other-topic")
        .publish(b"not for you")
        .await?;
    BroadcastChannel::new(publisher, TOPIC)
        .publish(b"for you")
        .await?;

    // Then only the subscribed topic is delivered. Ordering on the reliable channel is what makes
    // this exact rather than a race: the foreign message was sent first, so receiving "for you" as
    // the first message means the other one was filtered, not merely late.
    let received = next_message(&mut activity).await?;
    assert_eq!(received.payload, b"for you");
    Ok(())
}

#[tokio::test]
#[serial]
async fn a_payload_too_large_to_send_in_one_frame_is_refused_rather_than_split() -> Result<()> {
    // Given a payload one byte past what a single data frame carries
    let livekit = LiveKitTestkit::start().await?;
    let publisher = a_participant(&livekit, &a_room_for("broadcast-oversized"), "host").await?;
    let oversized = vec![b'x'; MAX_BROADCAST_PAYLOAD_BYTES + 1];

    // When it is broadcast
    let result = BroadcastChannel::new(publisher, TOPIC)
        .publish(&oversized)
        .await;

    // Then the publish fails loudly. A fire-and-forget topic has no reply to time out, so a chunked
    // payload that lost a frame would leave every subscriber waiting on a message that never
    // completes and never errors.
    let error = result.expect_err("an oversized broadcast must be refused");
    assert_eq!(
        error,
        BroadcastError::PayloadTooLarge {
            bytes: MAX_BROADCAST_PAYLOAD_BYTES + 1,
            limit: MAX_BROADCAST_PAYLOAD_BYTES,
        }
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn a_payload_of_exactly_the_maximum_size_is_delivered_whole() -> Result<()> {
    // Given a subscriber and a payload sitting exactly on the limit
    let livekit = LiveKitTestkit::start().await?;
    let room = a_room_for("broadcast-at-the-limit");
    let publisher = a_participant(&livekit, &room, "host").await?;
    let listener = a_participant(&livekit, &room, "agent").await?;
    let mut activity = BroadcastChannel::new(listener, TOPIC).subscribe();
    wait_for_participant(&publisher, "agent").await;
    let at_the_limit = vec![b'x'; MAX_BROADCAST_PAYLOAD_BYTES];

    // When it is broadcast
    BroadcastChannel::new(publisher, TOPIC)
        .publish(&at_the_limit)
        .await?;

    // Then it arrives whole. The limit is inclusive, so the largest legal payload is one the room
    // carries rather than the first one it refuses — a publisher sizing an event against the
    // constant would otherwise be off by one against the code enforcing it.
    let received = next_message(&mut activity).await?;
    assert_eq!(received.payload, at_the_limit);
    Ok(())
}

#[tokio::test]
#[serial]
async fn room_metadata_written_before_a_participant_joins_is_readable_on_arrival() -> Result<()> {
    // Given a room whose metadata was set before anyone joined
    let livekit = LiveKitTestkit::start().await?;
    let room = a_room_for("room-metadata-late-joiner");
    let metadata = RoomMetadataClient::with_api_key(&livekit.get_ws_url(), "devkey", "secret");
    metadata
        .create_with_metadata(&room, r#"{"changed_files":2}"#)
        .await?;

    // When a participant joins for the first time
    let joiner = a_participant(&livekit, &room, "late-agent").await?;

    // Then it reads the current state immediately, with no event to replay. This is what lets a
    // second agent start mid-session and know where the checkout stands.
    assert_eq!(joiner.metadata(), r#"{"changed_files":2}"#);
    Ok(())
}

#[tokio::test]
#[serial]
async fn updating_room_metadata_replaces_what_a_later_joiner_reads() -> Result<()> {
    // Given a room whose metadata has already been published once
    let livekit = LiveKitTestkit::start().await?;
    let room = a_room_for("room-metadata-update");
    let metadata = RoomMetadataClient::with_api_key(&livekit.get_ws_url(), "devkey", "secret");
    metadata
        .create_with_metadata(&room, r#"{"changed_files":1}"#)
        .await?;

    // When it is updated
    metadata
        .set_metadata(&room, r#"{"changed_files":7}"#)
        .await?;

    // Then a joiner sees the new value, not the old one — the facilitating daemon is the sole writer,
    // so last write wins is the whole contract
    let joiner = a_participant(&livekit, &room, "reader").await?;
    assert_eq!(joiner.metadata(), r#"{"changed_files":7}"#);
    Ok(())
}

#[tokio::test]
#[serial]
async fn re_creating_an_existing_room_replaces_its_metadata() -> Result<()> {
    // Given a room created once already, carrying the summary of that first session
    let livekit = LiveKitTestkit::start().await?;
    let room = a_room_for("room-metadata-recreated");
    let metadata = RoomMetadataClient::with_api_key(&livekit.get_ws_url(), "devkey", "secret");
    metadata
        .create_with_metadata(&room, r#"{"changed_files":1}"#)
        .await?;

    // When the same room is created again with a newer summary — a daemon re-opening the room of a
    // checkout it already hosts
    metadata
        .create_with_metadata(&room, r#"{"changed_files":9}"#)
        .await?;

    // Then a joiner reads the newer one. LiveKit's `create_room` against a room that exists returns
    // the room it found *without* applying the options it was handed, so creating and then setting
    // is what makes the post-condition identical on the first call and every one after it.
    let joiner = a_participant(&livekit, &room, "reader").await?;
    assert_eq!(joiner.metadata(), r#"{"changed_files":9}"#);
    Ok(())
}
