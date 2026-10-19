# 2026-10-19 — tddy-coder's room participant through a port

**Type:** Architecture

`#satellite` 8/13 ([#630](https://github.com/uppin/tddy-coder/pull/630)). The coder's library no longer names
LiveKit: serving its surface in a room goes through a port, and the `tddy-coder` binary moves to a new crate
that wires the LiveKit implementation. `tddy-coder` leaves the ratchet's `KNOWN_OFFENDERS`; `tddy-coder-cli`
joins `WIRING`.

## What changed

- **Port.** `tddy_coder::participant_host`: `SessionParticipantHost`, `ParticipantJoin`, `ParticipantAuth`,
  `ParticipantOptions`, `ServedParticipant`, `NoParticipantHost`, `participant_join_from_cli`,
  `serve_until_ended`. The host also mints room tokens for the coder's token service (`token_provider`, default
  refuses). [Participant host](../participant-host.md).
- **Call sites.** `run_main(args, host)` / `run_with_args(args, shutdown, host)`; the four join sites in
  `run.rs` and `session_participant::spawn_session_participant_with` go through the host.
- **New crate.** `packages/tddy-coder-cli`: the `tddy-coder` binary plus `LiveKitParticipantHost` (also a library
  target). `tddy-coder` is library-only and no longer depends on `tddy-livekit`. `tddy-demo` wires
  `NoParticipantHost`.
- **Helper move.** `merge_participant_metadata_json` is defined in `tddy-changeset`; `tddy-core` and `tddy-livekit`
  re-export it.
- **Scripts and CI.** `release`, `test`, `scripts/dev-runtime-binaries.sh` and `.config/nextest.toml` name
  `tddy-coder-cli` where a binary is built. Tests that run the built binary, and the two LiveKit participant
  suites, moved to `packages/tddy-coder-cli/tests`; assertions are unchanged.
- **Ratchet.** `tddy-coder` struck from `KNOWN_OFFENDERS`, `tddy-coder-cli` added to `WIRING`.
  [Extension ports](../../../tddy-daemon-extensions/docs/extension-ports.md#the-satellite-rule-and-its-ratchet).

## Not changed

CLI flags, the binary's name and path, identities, rooms, token lifetime (six hours), reconnect behaviour and
metadata semantics. Two behaviours differ at the margin: a fixed-token participant now honours the shutdown flag
and logs `READY` at every join site, and a LiveKit flag on `tddy-demo` is refused instead of joining. The
LiveKit `spawn_session_participant` is removed; only `spawn_session_participant_with` remains.

## Verification

Scoped, not whole-workspace. `cargo clippy --all-targets -D warnings` is clean on `tddy-coder`, `tddy-coder-cli`,
`tddy-changeset`, `tddy-livekit`, `tddy-demo` and `tddy-daemon-extensions`; `./test -p` on those packages plus
`tddy-livekit` passes (519 passed, 0 failed, 1 ignored), including the two LiveKit participant suites against a
real server and both ratchet tests. `tddy-e2e`, `tddy-integration-tests` and `tddy-daemon-sandbox` were not run
locally.

## Code issues

`oversized-file-run` (`run.rs`, 2,678 → 2,655 production lines) and `oversized-file-participant`
(`tddy-livekit`, 956 → 907) shrank but stay open, unclaimed; `cypress.config.ts` (1,014, string edits only) is
unchanged. Closed backlog entries: none.
