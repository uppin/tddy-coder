# PRD: Session runtime status — full stack (proto → web)

**Date**: 2026-03-22  
**Status**: Implemented  
**Type**: Feature (cross-cutting: `tddy-core`, `tddy-service`, `tddy-web`, contract tests)

## Summary

Deliver **end-to-end session runtime status** for remote terminals: a canonical `SessionRuntimeStatus` message on `tddy.v1.ServerMessage`, emitted from the presenter after each broadcastable event, converted over gRPC / LiveKit `TddyRemote.Stream`, and rendered in the browser (`SessionRuntimeStatusBar` + `GhosttyTerminalLiveKit`).

## Background

The web terminal must show the same workflow/session information as the local TUI status bar. Previously, clients inferred status from separate `goal_started` / `state_changed` events; that is fragile and does not carry elapsed time or a single authoritative `status_line`.

## Requirements

1. **Proto**: Add `SessionRuntimeStatus` with `status_line`, `goal`, `workflow_state`, `elapsed_ms`, `agent`, `model` and `session_runtime_status = 12` on `ServerMessage`.
2. **Rust**: After each non-snapshot `PresenterEvent` broadcast, emit `SessionRuntimeStatus` built from current `PresenterState` (TUI-aligned `status_line` formatting).
3. **Web**: On `TddyRemote` stream, handle `sessionRuntimeStatus` first; prefer `status_line`, else format from structured fields.
4. **Tests**: gRPC integration sees `SessionRuntimeStatus`; TS contract round-trip; Cypress component + Storybook e2e unchanged for UI shell.

## Requirements (Updated: 2026-03-22) — Authoritative live status

**Live** workflow and session-runtime status in the web UI **must** come from the running **`tddy-*` tool instance** via **remote control transport** — i.e. **`TddyRemote`** bidirectional streaming over **gRPC and/or LiveKit** (data-channel RPC), not from reading the on-disk **changeset** manifest (`changeset.yaml`) or inferring state solely from daemon **catalog** RPCs.

- **Subscription**: The browser **subscribes** to `ServerMessage` on `TddyRemote.Stream` (including **`session_runtime_status`** and other presenter-backed events) and **updates the UI in real time** as events arrive.
- **Not the source of truth for live UI**: Parsing or polling **`changeset.yaml`** (or equivalent disk state) to drive the **connected** terminal’s status bar is **out of scope** for authoritative live display; that persistence exists for **resume** and **offline inspection**, not for replacing the stream.
- **Discovery vs live session**: Daemon **`ListSessions`** exposes **session metadata** from **`.session.yaml`** (lifecycle `status`, etc.) for the connection table — **not** workflow state merged from **`changeset.yaml`**. **Once connected**, runtime status for that session is **stream-backed** as above.

## Success criteria

- [x] `cargo test -p tddy-service` includes integration assertion for `SessionRuntimeStatus` on stream.
- [x] `bun test tests/remote-session-runtime-contract.test.ts` passes.
- [x] `tddy-web` build + Cypress session-runtime specs pass.
- [x] `cargo clippy -p tddy-core -p tddy-service -- -D warnings` clean.

## Affected feature docs

- [gRPC remote control](../../coder/grpc-remote-control.md) — `ServerMessage` variants updated.
- [Web changelog](../changelog.md) — release note entry.

## References

- Changeset: `docs/dev/1-WIP/2026-03-22-session-runtime-status-full-stack.md`
- Proto: `packages/tddy-service/proto/tddy/v1/remote.proto`
