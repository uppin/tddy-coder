# Changeset: Per-Connection Virtual TUI

**Date**: 2026-03-14
**Status**: 🚧 In Progress
**Type**: Feature

## Affected Packages

- **tddy-core**: [README.md](../../packages/tddy-core/README.md) — Presenter view decoupling, ViewConnection, NoopView
- **tddy-tui**: [README.md](../../packages/tddy-tui/README.md) — VirtualTui, CapturingWriter headless mode
- **tddy-service**: [README.md](../../packages/tddy-service/README.md) — TerminalServiceImplPerConnection, DaemonService/TddyRemoteService with view_connection_factory
- **tddy-coder**: [README.md](../../packages/tddy-coder/README.md) — Daemon mode LiveKit TerminalService integration
- **tddy-e2e**: [README.md](../../packages/tddy-e2e/README.md) — E2E helpers and acceptance tests for per-connection streams

## Related Feature Documentation

- [PRD: Per-Connection Virtual TUI](../../ft/coder/1-WIP/PRD-2026-03-14-per-connection-virtual-tui.md)
- [grpc-remote-control.md](../../ft/coder/grpc-remote-control.md)
- [web-terminal.md](../../ft/web/web-terminal.md)

## Summary

Decouple the Presenter from a single View. Each terminal RPC connection (gRPC or LiveKit) creates its own virtual TUI instance with headless ratatui rendering. The same Presenter instance is shared across all connected TUIs. When a connection closes, its virtual TUI is stopped and cleaned up. New views receive a state snapshot on connect and then subscribe to live events.

## Background

Currently `Presenter<V: PresenterView>` is generic over a single view. In TUI mode, one `Presenter<TuiView>` owns one crossterm/ratatui renderer. In daemon mode, the Presenter runs headless with no TUI. This PRD-driven change enables per-connection virtual TUIs so remote clients can observe the workflow independently.

## Scope

- [x] **Implementation**: Presenter connect_view(), ViewConnection, NoopView
- [x] **Implementation**: VirtualTui in tddy-tui (headless ratatui, CapturingWriter)
- [x] **Implementation**: TerminalServiceImplPerConnection, DaemonService/TddyRemoteService with_view_connection_factory
- [x] **Implementation**: Daemon mode LiveKit exposes TerminalService (replaces EchoService when livekit enabled)
- [x] **Testing**: E2E gRPC two clients (virtual_tui_sessions.rs)
- [x] **Testing**: E2E LiveKit two clients (terminal_service_livekit.rs, with livekit feature)
- [ ] **Package Documentation**: Update package READMEs and dev docs
- [ ] **Integration**: Cross-package integration verified in CI

## Technical Changes

### State A (Before)

- Presenter generic over single View; no connect_view
- TerminalService streams shared broadcast bytes
- Daemon mode: EchoService only for LiveKit
- No per-connection virtual TUI

### State B (Target)

- Presenter exposes connect_view() → ViewConnection (state snapshot + event_rx + intent_tx)
- NoopView for headless/daemon mode
- VirtualTui: headless ratatui, CapturingWriter, per-connection thread
- TerminalServiceImplPerConnection creates VirtualTui per StreamTerminalIO call
- Daemon with LiveKit: TerminalService (per-connection VirtualTui) instead of EchoService
- E2E: two_grpc_clients_get_independent_terminal_streams, two_livekit_clients_get_independent_terminal_streams

### Delta

#### tddy-core
- **ViewConnection**: New struct (state_snapshot, event_rx, intent_tx)
- **Presenter**: with_intent_sender(), connect_view()
- **NoopView**: New PresenterView impl for headless mode
- **PresenterState**: Derive Clone for snapshots

#### tddy-tui
- **VirtualTui**: New module run_virtual_tui() — headless ratatui, event subscription, key parsing
- **CapturingWriter**: headless() constructor for callback-based output

#### tddy-service
- **TerminalServiceImplPerConnection**: Per-connection factory-based implementation
- **DaemonService/TddyRemoteService**: with_view_connection_factory(), stream_terminal_io uses VirtualTui when factory set

#### tddy-coder
- **run_daemon**: When livekit_enabled, creates Presenter with NoopView, view_connection_factory, TerminalServiceImplPerConnection; exposes TerminalService over LiveKit

#### tddy-e2e
- **spawn_presenter_with_view_connection_factory**: Helper for LiveKit tests (cfg livekit)
- **spawn_presenter_with_terminal_service**: Helper for gRPC per-connection tests
- **virtual_tui_sessions.rs**: two_grpc_clients_get_independent_terminal_streams
- **terminal_service_livekit.rs**: two_livekit_clients_get_independent_terminal_streams

## Implementation Progress

**Last Synced with Code**: 2026-03-14 (via @validate-changes)

**Core Features**:
- [x] Presenter ViewConnection, connect_view, NoopView — ✅ Complete (presenter_impl.rs, view.rs, presenter_events.rs)
- [x] VirtualTui run_virtual_tui — ✅ Complete (virtual_tui.rs)
- [x] TerminalServiceImplPerConnection — ✅ Complete (terminal_service.rs)
- [x] DaemonService/TddyRemoteService with_view_connection_factory — ✅ Complete (daemon_service.rs, service.rs)
- [x] Daemon LiveKit TerminalService — ✅ Complete (run.rs)
- [x] E2E gRPC two clients — ✅ Complete (virtual_tui_sessions.rs)
- [x] E2E LiveKit two clients — ✅ Complete (terminal_service_livekit.rs)

**Testing**:
- [x] Unit tests connect_view — ✅ Complete (presenter_impl.rs)
- [x] Unit tests parse_key_from_buf — ✅ Complete (virtual_tui.rs)
- [x] E2E virtual_tui_sessions — ✅ Complete
- [x] E2E terminal_service_livekit (with livekit feature) — ✅ Complete

## Validation Results

### Change Validation (@validate-changes)

**Last Run**: 2026-03-14
**Status**: ✅ Passed
**Risk Level**: 🟢 Low

**Changeset Sync**:
- ✅ Changeset created and synced with actual code state
- All PRD acceptance criteria implemented

**Build Validation Results**:

| Package   | Status | Notes                    |
|-----------|--------|--------------------------|
| tddy-core | ✅ Pass | Built successfully       |
| tddy-tui  | ✅ Pass | 1 warning: unused ActivityEntry in render test |
| tddy-service | ✅ Pass | Built successfully    |
| tddy-coder | ✅ Pass | Built successfully     |
| tddy-e2e  | ✅ Pass | Built successfully       |

**Analysis Summary**:
- Packages built: 5 (all success)
- Build warnings in changed code: 1 (tddy-tui render test — unused import)
- Files analyzed: 21
- Critical issues: 0
- Warnings: 1 (minor)

**Risk Assessment**:
- Build validation: Low
- Test infrastructure: Low
- Production code: Low
- Security: Low
- Code quality: Low

## Refactoring Needed

### From @validate-changes (Code Quality)
- [x] tddy-tui render.rs:411 — Remove unused `ActivityEntry` import in test module ✅

## References

- [PRD: Per-Connection Virtual TUI](../../ft/coder/1-WIP/PRD-2026-03-14-per-connection-virtual-tui.md)
