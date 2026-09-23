# heavy-params: connect_common_room_publish_metadata

**Location:** `packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs` — `connect_common_room_publish_metadata`
**Category:** heavy-params
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518
**Metrics:** **11 parameters** · 65 lines · nesting 6 · budget 5 / 60 / 4
**Thresholds breached:** parameters 11 > 5 (**2.2×**); length 65 > 60; nesting 6 > 4
**Restructure:** an options struct
**Status:** Open — partially fixed (parameters 5 and length 56 are within budget; nesting 6 > 4 remains) — **unclaimed**

## Measurement history

| Run | Params | Lines | Nesting | Note |
|---|---|---|---|---|
| 2026-09-19 | 11 | 65 | 6 | PR #518 did not change the signature; it reads `sandboxed_codebase_support()` inside |
| 2026-09-23 | 5 | 56 | 6 | #508 (`#keyring` 1/9): the caller builds the `DaemonAdvertisement` (signing key included) and passes it whole, replacing `local_id`, `repos_base_path` and `max_attachment_bytes`. Recount on `origin/master` (`77187dbe`) before the change gives **7** params / 65 lines / nesting 6 — the 11 of the first row is not reproduced by a signature count, so the drop attributable to #508 is 7 → 5 |

## What the tool found

Eleven positional parameters, several of them `Option`s and several the same type — the shape where
a caller can transpose two arguments and the compiler accepts it. This is the function that builds
and publishes the daemon's common-room advertisement, so a transposition would publish a wrong
self-description that nothing would catch.

PR #518 added `sandboxed_codebase` to the advertisement without widening the signature — the value
is read inside from `sandboxed_codebase_support()`. That is the right shape and this record is not
about that change; it is about a parameter list that was already past the point of safety.

## What would close it

**Remaining: nesting 6 > 4.** The parameter list is within budget since the advertisement travels
as one `DaemonAdvertisement`; what is left is depth — the deepest lines sit in the `RoomEvent::Connected` arm of
its wait-for-connection loop, which an `extract_method` of that wait would bring down.

The original proposal, for the record — a `CommonRoomPublish { … }` struct. Note the sibling `run_common_room_registry_loop` takes 8 and
`spawn_common_room_discovery_loop` is called from four test files — a signature change there
touches those, which is the reason the brief for PR #518 explicitly declined to thread the
capability through as a parameter.
