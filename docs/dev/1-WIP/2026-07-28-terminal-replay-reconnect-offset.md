# Changeset: Terminal Replay — Reconnect Resume by Offset (transport-blip survival)

**Date**: 2026-07-28
**Status**: ✅ Implemented — acceptance + unit tests green
**Type**: Feature (amends `2026-07-28-terminal-replay-viewport`)

## Planning artifacts

- [x] PRD: [docs/ft/web/terminal-replay-lazy-scroll.md](../../docs/ft/web/terminal-replay-lazy-scroll.md)
  (amended — reconnect resume-by-offset + no-blank-page sections).
- [x] Changeset: this document.
- [x] Acceptance tests (Cypress component): `cypress/component/GrpcSessionTerminalResume.cy.tsx`
  (reconnect resume-by-offset) + `GhosttyTerminalGrpcLazyHistory.cy.tsx` blank-reconnect regression
  tests; driver `cypress/support/drivers/ghosttyTerminalGrpcLazyHistoryDriver.tsx`.
- [x] Acceptance tests (Rust): `tddy-terminal-rpc/tests/bridge.rs` (FROM_OFFSET catch-up),
  `tddy-daemon` terminal acceptance suites, `tddy-coder` participant tests.

## Affected Packages

- `tddy-service` / `tddy-terminal-rpc` — proto: `StreamTerminalOutputRequest` + bidi open-frame
  gain `mode` (`StreamReplayMode`) + `from_offset`; new `StreamReplayMode` enum
  (`TAIL` default / `FROM_OFFSET`).
- `tddy-terminal-rpc` — `bridge.rs` extracts the shared `open_replay_ack_live` sequence and adds the
  `FROM_OFFSET` catch-up branch (chunked `replay_from(from_offset, tip, …)` until `at_end`, no tail
  chunk, no PTY resize/drain).
- `tddy-daemon` — `connection_service` `stream_terminal_output` threads `mode`/`from_offset` into
  the bridge; bidi `stream_session_terminal_io` routes through the same helper.
- `tddy-coder` — `session_participant` `stream_terminal_output` / bidi delegate `mode`/`from_offset`.
- `tddy-tools` — `pty_relay.rs` arg dispatch updated for the new bridge surface.
- `tddy-web` — `GrpcSessionTerminal` tracks the cumulative output offset, sends `FROM_OFFSET` with
  the tracked offset on reconnect (no duplicate replay), and survives a transient transport blip
  (null client) without evicting the runtime; `GhosttyTerminalGrpc` bounds the forward-fill by the
  current live tip (see `2026-07-28-terminal-native-scrolling`).

## Related Feature Documentation

- [terminal-replay-lazy-scroll.md](../../docs/ft/web/terminal-replay-lazy-scroll.md) — the WHAT
  (reconnect resume-by-offset + no-blank-page-on-stale-fill).

## Summary

A terminal that has already synced its state should not re-replay the whole retained buffer when
its transport blips and reconnects. The replay wire contract gains a `StreamReplayMode`:

- **`TAIL`** (default, first connect): the server sends the mode prologue + the current last-frame
  tail chunk (tagged with absolute offsets), resizes the PTY to the client's dimensions, drains the
  pre-resize broadcast, then bridges live output.
- **`FROM_OFFSET`** (reconnect): the server sends the mode prologue + chunked catch-up via
  `replay_from(from_offset, tip, …)` until `at_end`, then live output. No tail chunk, no PTY
  resize/drain — a terminal that already holds state up to `from_offset` receives only the bytes it
  missed, with no duplicate content.

The bidi `StreamSessionTerminalIO` open frame carries the same `mode`/`from_offset` so a bidi
client can replay-once-at-init / resume-by-offset on the same connection that carries its input.

On the client side, `GrpcSessionTerminal`:

- Tracks the cumulative output offset (`currentOffsetRef`) — snapped to the frame's absolute
  `endOffset` on replay/catch-up frames, advanced by byte length on live tail frames.
- Records `hasSynced` once the initial TAIL replay has landed; a subsequent stream open (reconnect)
  sends `FROM_OFFSET` with the tracked offset instead of re-replaying.
- Treats a **null client** (transient transport blip) as a **pause**, not an evict: the terminal
  stays mounted (its scrollback and the ghostty instance survive), input is queued, and the stream
  resumes with `FROM_OFFSET` when a non-null client returns. Only a stream-end with a **valid**
  client (real `pty_done`) evicts the runtime.

The forward-fill bounding by the current live tip (so a stale/evicted anchor does not yield a blank
page) is documented in `2026-07-28-terminal-native-scrolling`.

## Scope

- [x] Proto: `StreamReplayMode` enum + `mode`/`from_offset` on `StreamTerminalOutputRequest` and the
  bidi `SessionTerminalInput` open frame; regenerated stubs.
- [x] `tddy-terminal-rpc/src/bridge.rs`: extract `open_replay_ack_live`; `FROM_OFFSET` catch-up
  branch.
- [x] `tddy-daemon`/`tddy-coder`: thread `mode`/`from_offset`; bidi routes through the shared
  helper.
- [x] `tddy-web` `GrpcSessionTerminal`: offset tracking, `FROM_OFFSET` on reconnect, null-client
  pause (no evict on transport blip).
- [x] Acceptance tests: Cypress reconnect resume + Rust bridge FROM_OFFSET + daemon/coder
  acceptance.

## Technical Changes

### State A (before)

- `StreamTerminalOutput` always sends the current last-frame tail chunk + resizes the PTY. A
  reconnecting terminal re-receives the whole retained tail — duplicate content in the live
  terminal.
- `GrpcSessionTerminal.client` is non-null; a transport blip tears down the stream and evicts the
  runtime (the terminal unmounts and re-replays from scratch on reconnect).

### State B (after)

- `StreamTerminalOutput` honors `mode`: `TAIL` (first connect) vs `FROM_OFFSET` (reconnect catch-up
  from `from_offset` to the tip, no tail/resize/drain).
- `GrpcSessionTerminal.client` is `ConnectionClient | null`; a null client pauses the terminal
  (mounted, input queued) and resumes with `FROM_OFFSET` when a non-null client returns. Only a
  stream-end with a valid client evicts.

### Delta

- `proto/terminal_session.proto`, `proto/connection.proto`: `StreamReplayMode` + `mode`/`from_offset`.
- `tddy-terminal-rpc/src/bridge.rs`: `open_replay_ack_live` + `FROM_OFFSET` branch;
  `replay_mode_from_i32`.
- `tddy-daemon/src/connection_service.rs`: `stream_terminal_output` / bidi delegate `mode`/`from_offset`.
- `tddy-coder/src/session_participant/mod.rs`: delegate `mode`/`from_offset`.
- `tddy-tools/src/pty_relay.rs`: arg dispatch.
- `tddy-web/src/components/sessions/GrpcSessionTerminal.tsx`: offset tracking, `FROM_OFFSET`
  reconnect, null-client pause.
- `tddy-web/src/components/sessions/SessionRuntime.tsx`: thread null client.
- `tddy-web/cypress/component/GrpcSessionTerminalResume.cy.tsx`: reconnect resume-by-offset.
- `tddy-web/src/gen/connection_pb.ts`: regenerated.

## Acceptance Tests

- Cypress `GrpcSessionTerminalResume.cy.tsx` — a reconnect after the initial sync sends
  `FROM_OFFSET` with the tracked offset (no duplicate replay); a transport blip (null client)
  keeps the terminal mounted and queues input until a non-null client returns.
- Cypress `GhosttyTerminalGrpcLazyHistory.cy.tsx` — blank-reconnect regression (stale/evicted
  anchor, empty fill, error fill stay on the live pane).
- Rust `tddy-terminal-rpc/tests/bridge.rs` — `FROM_OFFSET` catch-up chunks reach `at_end` without a
  tail chunk or resize.
- `tddy-daemon`/`tddy-coder` terminal acceptance suites green.

## Technical Debt & Production Readiness

- The bidi open-frame params are read only from the first message of a `StreamSessionTerminalIO`
  stream and ignored on subsequent chunks (documented in the proto comments).
- `chunk_terminal_output` / `sandbox_replay_frames` in `connection_service.rs` are retained for the
  `sandbox_replay_tests` unit tests (the production sandbox path now uses `replay_from` directly);
  marked `#[cfg_attr(not(test), allow(dead_code))]`.

## Decisions & Trade-offs

See PRD § Decisions & trade-offs (reconnect resume-by-offset).

## References

- [PRD](../../docs/ft/web/terminal-replay-lazy-scroll.md)
- `packages/tddy-terminal-rpc/src/bridge.rs`
- `packages/tddy-daemon/src/connection_service.rs`
- `packages/tddy-web/src/components/sessions/GrpcSessionTerminal.tsx`
