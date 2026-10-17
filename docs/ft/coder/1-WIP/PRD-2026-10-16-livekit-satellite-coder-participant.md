# tddy-coder's room participant through a port - PRD

**Date**: 2026-10-16
**PRD Type**: Technical Improvement
**Stack**: #satellite 8/11. Umbrella PRD on branch `satellite-1-extension-ports` (#623).

## Affected Features

- **Primary Feature**: `tddy-coder`'s LiveKit mode (`--livekit-url`, `--livekit-room`,
  `--livekit-identity`, key/secret or `--livekit-token`): the coder serves its session RPC surface as
  a room participant. Behaviour is unchanged; the participant is reached through a port.
- **Related Feature**: the daemon-spawned session participant (`session_participant/`), which serves
  one session's RPC under its own identity.

## Summary

`tddy-coder`'s library stops naming LiveKit. Serving an RPC surface in a room — once with a fixed
token, or with reconnect and a credential-minted token; publishing participant metadata from a
`watch` channel; watching the codex-OAuth file — goes through a `SessionParticipantHost` port the
library defines. The LiveKit implementation lives with the binary's wiring. The pure JSON helper
`merge_participant_metadata_json` moves out of `tddy-livekit` into `tddy-core`'s
`session_participant_metadata`, where its consumer's types already live. `tddy-coder` leaves the
ratchet's `KNOWN_OFFENDERS` once its binary is a separate crate (see Decisions).

## Background

`run.rs` (5,199 lines; code issue `oversized-file-run.md`, unclaimed) joins rooms in four places
(`:1980`, `:2229–2290`, `:3550–3660`, `:3680`), each minting with `TokenGenerator` and driving
`LiveKitParticipant`. `session_participant/mod.rs:107–121` does the same for one session. The
library is what the coder's tests link, so every test links the SDK.

## Proposed Changes

### What's Changing
- `SessionParticipantHost` port in `tddy-coder` (lib): `serve(ParticipantJoin, MultiRpcService,
  ParticipantOptions) -> ServedParticipant`, where `ParticipantJoin` = URL, room, identity and
  `Credentials{key, secret} | Token`, and `ParticipantOptions` = metadata `watch::Receiver`,
  codex-OAuth path, projects-registry dir, reconnect on/off, shutdown flag. `NoParticipantHost`
  refuses.
- The four `run.rs` sites and `session_participant` take the host from their caller.
- A new binary crate `tddy-coder-cli` (binary name stays `tddy-coder`) wires
  `LiveKitParticipantHost` (implemented there over `tddy_livekit::LiveKitParticipant`); `tddy-coder`
  becomes library-only and drops `tddy-livekit`.
- `merge_participant_metadata_json` moves to `tddy-core::session_participant_metadata`; `tddy-livekit`
  re-exports it for its own use.

### What's Staying the Same
- CLI flags, identities, rooms, TTLs, reconnect behaviour, metadata semantics.
- `./release`, `./install`, `./web-dev`: the binary keeps its name and path `target/<profile>/tddy-coder`.

## Impact Analysis
- **Technical:** one new crate (bin), `run.rs` call sites, scripts unaffected by name. Tests of the
  library can use an in-memory host.
- **User:** none.

## Implementation Plan
1. Port + null object in `tddy-coder`; JSON helper move.
2. Switch `run.rs` and `session_participant`.
3. Split the binary into `tddy-coder-cli` with the LiveKit host; strike the ratchet entry.

## Success Criteria
- `tddy-coder` names no LiveKit crate; the ratchet agrees (`tddy-coder-cli` is on the wiring list).
- The coder's LiveKit e2e/testkit suites pass through the binary.
- The library's join logic is tested against an in-memory host.

## Out of Scope
- Splitting `run.rs` (its code issue); this node only replaces call sites.
