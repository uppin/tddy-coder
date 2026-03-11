# Changeset: Terminal Streaming via gRPC

**Date**: 2026-03-11
**Status**: ✅ Complete
**Type**: Feature

## Affected Packages

- **tddy-tui**: [README.md](../../packages/tddy-tui/README.md) - CapturingWriter, event loop byte capture
- **tddy-grpc**: [README.md](../../packages/tddy-grpc/README.md) - StreamTerminal RPC, proto definition
- **tddy-coder**: [README.md](../../packages/tddy-coder/README.md) - Wiring in run_full_workflow_tui

## Related Feature Documentation

- [PRD Document](../../docs/ft/coder/1-WIP/PRD-2026-03-11-terminal-streaming-via-grpc.md)
- [gRPC Remote Control](../../docs/ft/coder/grpc-remote-control.md)

## Summary

Add a `StreamTerminal` server-streaming RPC to the existing `TddyRemote` gRPC service. A `CapturingWriter` in tddy-tui intercepts raw bytes from ratatui/crossterm rendering and broadcasts them to gRPC clients, enabling remote TUI viewing.

## Background

The existing gRPC `Stream` RPC provides structured `PresenterEvent`s but not the rendered terminal output. Remote clients cannot reconstruct the visual TUI. Use case: remote TUI viewer — a developer wants to observe the TUI from a different machine. The ratatui/crossterm stack produces ANSI escape sequences; by capturing these via a custom Write implementation and broadcasting to gRPC, clients receive the exact byte stream a terminal would see.

## Scope

- [x] **Package Documentation**: Changeset complete; wrap to apply to package docs
- [x] **Implementation**: Complete code changes across tddy-tui, tddy-grpc, tddy-coder
- [x] **Testing**: All acceptance tests passing
- [x] **Integration**: Cross-package integration verified
- [x] **Technical Debt**: Production readiness gaps addressed
- [x] **Code Quality**: Linting, type checking, and code review complete

## Technical Changes

### State A (Current)

- tddy-tui: `run_event_loop` uses `CrosstermBackend<Stdout>` directly; no byte capture
- tddy-grpc: `TddyRemote` has Stream, GetSession, ListSessions; no terminal streaming
- tddy-coder: When `--grpc` is set, starts gRPC server with TddyRemoteService; no terminal byte channel

### State B (Target)

- tddy-tui: `CapturingWriter` wraps stdout and invokes callback on each write; `run_event_loop` accepts optional `ByteCallback`
- tddy-grpc: `StreamTerminal` RPC streams `TerminalOutput` (raw bytes); `TddyRemoteService` accepts optional `terminal_byte_tx`
- tddy-coder: When `--grpc` is set, creates broadcast channel, passes callback to event loop, passes sender to service

### Delta

#### tddy-tui
- **New**: `capturing_writer.rs` — `CapturingWriter` with `Clone`, `Write`, `ByteCallback` type
- **Modified**: `event_loop.rs` — add `byte_capture: Option<ByteCallback>` parameter; use `CapturingWriter` when provided
- **Modified**: `lib.rs` — re-export `ByteCallback`

#### tddy-grpc
- **Modified**: `proto/tddy/v1/remote.proto` — add `StreamTerminal`, `StreamTerminalRequest`, `TerminalOutput`
- **Modified**: `service.rs` — add `terminal_byte_tx`, `with_terminal_bytes`, implement `stream_terminal`

#### tddy-coder
- **Modified**: `run.rs` — in `run_full_workflow_tui`, when `--grpc` set: create broadcast channel, build callback, pass to `run_event_loop` and `TddyRemoteService`

## Implementation Milestones

- [x] Milestone 1: Add proto definition and regenerate
- [x] Milestone 2: Create CapturingWriter with unit tests
- [x] Milestone 3: Integrate CapturingWriter into run_event_loop
- [x] Milestone 4: Implement stream_terminal in TddyRemoteService
- [x] Milestone 5: Wire broadcast channel in tddy-coder run_full_workflow_tui
- [x] Milestone 6: Add gRPC integration tests for StreamTerminal

## Acceptance Tests

### tddy-tui
- [x] **Unit**: CapturingWriter write captures bytes, clone shares callback, flush delegates (capturing_writer.rs)

### tddy-grpc
- [x] **Integration**: stream_terminal_returns_bytes — connect, receive non-empty TerminalOutput
- [x] **Integration**: streamed_bytes_contain_ansi_sequences — verify ANSI CSI codes
- [x] **Integration**: multiple_clients_receive_same_stream — two simultaneous subscribers
- [x] **Integration**: stream_terminal_returns_empty_stream_when_no_terminal_bytes — stream ends when no capture

## Technical Debt & Production Readiness

(To be filled during implementation)

## Decisions & Trade-offs

- **ByteCallback over tokio in tddy-tui**: Generic `Box<dyn Fn(&[u8]) + Send>` avoids tokio dependency; wiring layer (tddy-coder) bridges to broadcast
- **Broadcast for fan-out**: Multiple clients subscribe; slow clients may miss frames (acceptable for terminal streaming)
- **Clone-based CapturingWriter**: `Arc<Mutex<Inner>>` enables use in both CrosstermBackend (takes ownership) and execute! calls

## Refactoring Needed

(To be filled by validation rules)

## Validation Results

### Change Validation
**Last Run**: 2026-03-11
**Status**: Passed
**Summary**: cargo fmt, cargo clippy -- -D warnings passed. No production threats identified in new code.

## References

- [Plan document](/Users/mantasi/.cursor/plans/terminal_streaming_via_grpc_a0c139ab.plan.md)
- [ratatui CrosstermBackend](https://docs.rs/ratatui/latest/ratatui/backend/struct.CrosstermBackend.html)
