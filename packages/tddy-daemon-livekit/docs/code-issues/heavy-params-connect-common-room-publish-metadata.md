# heavy-params: connect_common_room_publish_metadata

**Location:** `packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs` — `connect_common_room_publish_metadata`
**Category:** heavy-params
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518
**Metrics:** **11 parameters** · 65 lines · nesting 6 · budget 5 / 60 / 4
**Thresholds breached:** parameters 11 > 5 (**2.2×**); length 65 > 60; nesting 6 > 4
**Restructure:** an options struct
**Status:** Open — **unclaimed**

## Measurement history

| Run | Params | Lines | Nesting | Note |
|---|---|---|---|---|
| 2026-09-19 | 11 | 65 | 6 | PR #518 did not change the signature; it reads `sandboxed_codebase_support()` inside |

## What the tool found

Eleven positional parameters, several of them `Option`s and several the same type — the shape where
a caller can transpose two arguments and the compiler accepts it. This is the function that builds
and publishes the daemon's common-room advertisement, so a transposition would publish a wrong
self-description that nothing would catch.

PR #518 added `sandboxed_codebase` to the advertisement without widening the signature — the value
is read inside from `sandboxed_codebase_support()`. That is the right shape and this record is not
about that change; it is about a parameter list that was already past the point of safety.

## What would close it

A `CommonRoomPublish { … }` struct. Note the sibling `run_common_room_registry_loop` takes 8 and
`spawn_common_room_discovery_loop` is called from four test files — a signature change there
touches those, which is the reason the brief for PR #518 explicitly declined to thread the
capability through as a parameter.
