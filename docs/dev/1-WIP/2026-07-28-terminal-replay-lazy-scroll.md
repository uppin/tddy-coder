# Changeset: Terminal Replay & PTY-over-RPC Unification

**Date**: 2026-07-28
**Status**: ✅ Complete
**Type**: Architecture Change / Feature

## Affected Packages

- `tddy-task` — `TerminalCapture` absolute byte-offset tracking (`replay_last` / `replay_from`).
- `tddy-terminal-rpc` (new) — unified PTY-over-RPC bridge crate (`TerminalSession` / `TerminalSessionStore` traits, `serve_stream_terminal_output` / `serve_get_terminal_history` / `serve_send_terminal_input`, `local_pty_relay`).
- `tddy-service` — `connection.proto` changes: `SessionTerminalOutput` offset fields, `GetTerminalHistory` RPC + `GetTerminalHistoryRequest` / `TerminalHistoryChunk` messages (forward chunking: `from_offset` / `until_offset` / `at_end`).
- `tddy-daemon` — `connection_service` claude-cli `StreamTerminalOutput` + `GetTerminalHistory` delegate to the bridge via `DaemonTerminalSessionStore`; tonic adapter wired.
- `tddy-coder` — `session_participant` `StreamTerminalOutput` + `GetTerminalHistory` arms delegate to the bridge via `CoderTerminalSessionStore`.
- `tddy-tools` — `pty_relay.rs` shrunk to arg dispatch over `tddy-terminal-rpc::local_pty_relay`.
- `tddy-sandbox-runner` — `SessionTerminalOutput` literals updated for the new fields.
- `tddy-web` — regenerated gRPC stubs; `TerminalHistoryForwardLoader` client primitive + unit tests; `GrpcSessionTerminal` anchor capture + forward-fetcher wiring; in-memory Cypress backend `getTerminalHistory`; Cypress component test; fixed 3 pre-existing `UploadProgressProvider` test-harness bugs.

## Related Feature Documentation

- [Terminal Sessions — § Lazy replay & scroll-up history](../../docs/ft/daemon/terminal-sessions.md)

## Summary

Replaces the eager full-capture replay with a last-frame-first lazy model and unifies the
duplicated PTY-over-RPC bridge logic from `tddy-daemon` and `tddy-coder` into a shared
`tddy-terminal-rpc` crate. Adds a `GetTerminalHistory` RPC for on-demand older-history fetches
(scroll-up infinite loading).

## Scope

- [x] `TerminalCapture` absolute byte-offset tracking + `replay_last` / `replay_from` (TDD).
- [x] New `tddy-terminal-rpc` crate: proto, traits, bridge, `local_pty_relay`; bridge tests.
- [x] Absorb `tddy-tools` local PTY relay into the new crate.
- [x] `connection.proto` offset fields + `GetTerminalHistory` RPC + forward-chunk messages
  (`from_offset` / `until_offset` / `at_end`).
- [x] Daemon `StreamTerminalOutput` (claude-cli) + `GetTerminalHistory` delegate to the bridge.
- [x] Coder `StreamTerminalOutput` + `GetTerminalHistory` arms delegate to the bridge.
- [x] Web: regen stubs, `TerminalHistoryForwardLoader` + tests, `GrpcSessionTerminal` wiring, Cypress test.
- [x] Viewport integration: progressive forward-fill via a stacked older-history terminal — see
  [2026-07-28-terminal-replay-viewport.md](2026-07-28-terminal-replay-viewport.md).

## Technical Changes

### State A (before)

- `StreamTerminalOutput` replayed the **entire** retained capture buffer on connect, then tailed live output.
- PTY-over-RPC bridge logic (replay, ACK interleave, resize/drain, input forward, exit) was duplicated between `tddy-daemon/src/connection_service.rs` and `tddy-coder/src/session_participant/mod.rs`.
- No lazy history API; no absolute byte offsets on `SessionTerminalOutput`.

### State B (after)

- `StreamTerminalOutput` sends the **current last frame first** (tagged with `start_offset`/`end_offset`/`at_oldest`), then tails live output.
- `GetTerminalHistory(from_offset, until_offset)` returns one forward chunk per call; the client appends each chunk to an older-history terminal and advances `from_offset` to the chunk's `end_offset` until `at_end = true` (reached the anchor).
- One unified bridge in `tddy-terminal-rpc` behind `TerminalSession` / `TerminalSessionStore` traits; daemon and coder adapt their `PtyHandle`s and delegate.
- Proto changes are **not backward-compatible** (the `before_offset` field was replaced by `from_offset`/`until_offset` + `at_end`); `GrpcSessionTerminal` is the only consumer and is updated in lockstep.

### Delta

- New crate `tddy-terminal-rpc` (workspace member).
- `tddy-task::TerminalCapture` offset API.
- `connection.proto`: +3 fields on `SessionTerminalOutput`, +1 RPC, +2 messages.
- Daemon/coder handlers rewritten to delegate; sandbox path preserved.
- Web `TerminalHistoryLoader` + `GrpcSessionTerminal` wiring + Cypress coverage.

## Acceptance Tests

- `tddy-task` `terminal_capture_offsets` — offset tracking.
- `tddy-terminal-rpc` `bridge` + `local_pty_relay` — unified bridge behavior.
- `tddy-daemon` `terminal_session_acceptance` / `terminal_mode_replay_acceptance` / `terminal_input_ack_acceptance` / `terminal_control_acceptance` — replay/ACK/control unchanged after migration.
- `tddy-coder` `coder_serves_connection_service_from_participant` — coder terminal delegation.
- `tddy-web` `terminalHistoryLoader.test.ts` (bun) — forward-loader state machine.
- `tddy-web` `GrpcSessionTerminalLazyHistory.cy.tsx` (Cypress component) — end-to-end forward-fill wiring.

## Technical Debt & Production Readiness

- **Viewport integration**: delivered as a progressive forward-fill via a stacked older-history terminal — see [2026-07-28-terminal-replay-viewport.md](2026-07-28-terminal-replay-viewport.md). The live terminal stays at `scrollback: 0` (preserving the no-duplicate-pane fix); the older terminal accumulates older output forward.
- **Type duplication**: `connection.proto` and `tddy-terminal-rpc/proto/terminal_session.proto` define structurally identical terminal messages; the daemon/coder convert at the bridge boundary. A future cleanup can collapse these via a proto `import` + `extern_path` so both transports share one canonical Rust type.

## Decisions & Trade-offs

- **Additive proto over full split**: kept the terminal RPCs on `ConnectionService` (added fields + one RPC) rather than moving 8 RPCs to a new `TerminalSessionService`. Rationale: a full RPC move would break the web client and tools mid-migration and require a coordinated big-bang across daemon+web+tools; the additive path delivers the same lazy-replay + unified-bridge goals with backward compatibility and far less churn. The unified bridge is the real unification; the proto split is a later cosmetic cleanup.
- **Per-frame type conversion at the daemon/coder boundary** rather than sharing proto types across crates, to avoid a cross-crate proto-codegen dependency and keep the migration additive.

## References

- [Terminal Sessions (feature doc)](../../docs/ft/daemon/terminal-sessions.md)
- `packages/tddy-terminal-rpc/` (crate root)
- `packages/tddy-web/src/lib/terminalHistoryLoader.ts`
