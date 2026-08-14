# Broadcast and room metadata (`tddy_livekit::broadcast`, `tddy_livekit::room_metadata`)

## Role

Two primitives a room needs once it has more than two participants: a **room-wide publish on its own
topic**, and **room metadata** as a snapshot every joiner reads on arrival.

Everything else this crate publishes is unicast on the single `tddy-rpc` topic, addressed with
`destination_identities`, and every receiver hard-filters on that topic. A broadcast inverts both
halves — empty `destination_identities` so the server fans the packet out, and its own topic so the
RPC receivers drop it instead of trying to decode it as an envelope.

First consumer: [session rooms](../../../docs/ft/daemon/session-room.md).

## Public API (summary)

| Item | Role |
|------|------|
| **`BroadcastChannel`** | One topic on a room's data channel. `new`, `publish`, `publisher`, `subscribe`. |
| **`BroadcastPublisher`** | The publish half alone, over a connection rather than a `Room` handle. |
| **`BroadcastMessage`** | `from` (sender identity, when the server attributed one) and `payload`. |
| **`BroadcastError`** | `PayloadTooLarge { bytes, limit }` or `Publish(String)`. |
| **`MAX_BROADCAST_PAYLOAD_BYTES`** | 8 KiB. |
| **`RoomMetadataClient`** | `with_api_key`, `create_with_metadata`, `set_metadata`. |
| **`livekit_http_url`** | `ws://` → `http://`, `wss://` → `https://`; anything else unchanged. |

## Broadcasting

`publish` sends one reliable `DataPacket` with the channel's topic and an **empty**
`destination_identities`. That empty vec is the whole difference from an RPC publish: it is what
reaches participants the publisher never named, including ones that joined after it did.

An oversized payload is **refused, never chunk-framed**. `chunking` exists precisely to split large
messages, and this deliberately does not use it: chunking is only safe when a lost frame eventually
surfaces as a failed call, and a fire-and-forget broadcast has no reply to time out. A
half-delivered chunked broadcast would leave every subscriber waiting on a message that never
completes and never errors, with nothing to retry it.

`subscribe` takes its own `room.subscribe()` handle rather than sharing one with the RPC loops, which
would race them for each event, and drops everything on other topics — `tddy-rpc` above all.

### Publishing over a served connection

`LiveKitParticipant::serve` consumes the participant and its `Room`, so a daemon that has joined a
room cannot get an `Arc<Room>` back to build a `BroadcastChannel` from. Opening a second connection
would put a second identity into the room, which defeats a room whose claim is that only the daemon
is in it.

`JoinedParticipant::broadcast_on` returns a `BroadcastPublisher` bound to the connection's
`SharedPublisher` instead. That is the same indirection the response drains use, and it matters for
the same reason: it follows the connection across reconnects rather than pinning the
`LocalParticipant` it happens to have right now. A publisher held for the life of a room keeps
reaching the room.

## Room metadata

A broadcast only reaches whoever is connected at publish time, so it cannot tell a later joiner where
things stand. Room metadata covers exactly that gap: the server hands the current value to each
participant as part of the join, with no history to replay.

Writes go through LiveKit's Twirp server API rather than the room connection, because room metadata
is a room-admin operation and a participant's token does not grant it.
`livekit_api::services::room::RoomClient` signs its **own** room-admin token from the api key/secret
on every call — which is why nothing in `token.rs` needs a `room_admin` grant, and why this client
takes the api secret rather than a participant token.

`create_with_metadata` is two calls on purpose. `create_room` against an existing room succeeds by
returning the room it found, *without* applying the options it was given, so a bare create would
silently leave a re-created room showing whatever metadata it had the first time. Creating and then
setting makes the post-condition the same on both paths, which is what lets a daemon call it on every
session start.

`set_metadata` is last-write-wins with no merge — correct only while a single writer owns a room's
metadata, which is the contract session rooms keep.

## Tests

`tests/broadcast_and_room_metadata.rs` (real LiveKit, `#[serial]`): one publish reaching two
subscribers the publisher never named, topic isolation from `tddy-rpc`, the inclusive size limit and
the refusal one byte past it, metadata readable by a first joiner, metadata replaced by an update,
and re-creating an existing room replacing its metadata. Room names carry a per-run nonce because the
testkit container is deliberately reused across runs.

## Related

- [Participant metadata](participant-metadata.md) — the per-participant sibling of this room-level state
- [Session rooms](../../../docs/ft/daemon/session-room.md) — the first consumer
