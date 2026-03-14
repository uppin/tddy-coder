# PRD: Per-Connection Virtual TUI

**Status:** WIP
**Date:** 2026-03-14

## Summary

Decouple the Presenter from a single View. Each terminal RPC connection (gRPC or LiveKit) creates its own virtual TUI instance with headless ratatui rendering. The same Presenter instance is shared across all connected TUIs. When a connection closes, its virtual TUI is stopped and cleaned up. New views receive a state snapshot on connect and then subscribe to live events.

## Background

Currently `Presenter<V: PresenterView>` is generic over a single view. In TUI mode, one `Presenter<TuiView>` owns one crossterm/ratatui renderer that writes to stdout. The TerminalService streams these same ANSI bytes to all clients via a shared broadcast channel — every connected client sees the identical output stream.

In daemon mode (`--daemon`), the Presenter runs headless with no TUI at all. The only LiveKit service exposed is EchoService for testing. There is no way to observe the workflow's TUI remotely in daemon mode.

This creates several limitations:

- **No per-client TUI in daemon mode** — remote clients cannot observe the workflow
- **Shared output in TUI mode** — all gRPC/LiveKit clients share one byte stream (no independent scroll/interaction)
- **No connect/disconnect lifecycle** — clients cannot attach/detach without affecting others
- **No initial state for late joiners** — a client connecting mid-workflow sees only events from that point forward

## Affected Features

- [grpc-remote-control.md](../grpc-remote-control.md) — Presenter event bus, daemon mode, terminal streaming
- [web-terminal.md](../../web/web-terminal.md) — GhosttyTerminal component consumes TerminalService stream

## Proposed Changes

### 1. Presenter View Decoupling

**What changes:** The `Presenter` is no longer generic over `V: PresenterView`. Views are fully decoupled — they subscribe to a broadcast channel of `PresenterEvent`s and can be attached/detached at any point.

**What stays the same:** `PresenterView` trait remains. `PresenterState` remains. The workflow engine, intents, and event types are unchanged.

**New capability:** `Presenter` exposes a method to get a state snapshot (`PresenterState` clone or a dedicated `StateSnapshot` struct) so new views can initialize with current state before subscribing to live events.

### 2. Per-Connection Virtual TUI

**What changes:** When a client connects to `TerminalService.StreamTerminalIO`, the service creates a dedicated virtual TUI for that connection:

- A headless ratatui renderer using `CrosstermBackend<CapturingWriter>` (same approach as TUI mode, but writing to an in-memory buffer instead of stdout)
- The virtual TUI subscribes to `PresenterEvent`s, renders state changes, and streams the resulting ANSI bytes to the connected client
- Client keyboard input is processed by the virtual TUI's key mapper into `UserIntent`s sent to the Presenter
- When the client disconnects, the virtual TUI is stopped and resources are cleaned up

**What stays the same:** The rendering code (`draw()`, layout, UI components) is reused unchanged. The `CapturingWriter` pattern already exists.

### 3. Daemon Mode Integration

**What changes:** The daemon creates a Presenter (non-generic, or with a no-op primary view) and exposes the TerminalService over both gRPC and LiveKit. Each connected client gets its own virtual TUI rendering the shared Presenter's state.

**What stays the same:** Session management (DaemonService) is unchanged. The gRPC and LiveKit transport layers are unchanged.

## Technical Constraints

- Virtual TUI rendering happens in a dedicated thread/task per connection (ratatui is not thread-safe; each virtual TUI needs its own Terminal instance)
- `PresenterState` must be cheaply cloneable for snapshots
- The broadcast channel already exists (`broadcast_tx` in Presenter) — no new channel infrastructure needed
- Keyboard input from multiple clients must be serialized through the existing `intent_tx` mpsc channel

## Acceptance Criteria

1. Two clients can connect simultaneously to the daemon's TerminalService (via gRPC or LiveKit) and each receives independent ANSI byte streams
2. A client connecting mid-workflow receives the current TUI state (not a blank screen)
3. Disconnecting one client does not affect the other client's stream
4. Client keyboard input is forwarded to the Presenter as UserIntents
5. E2E test: Two TUI sessions connect and disconnect via gRPC & TerminalService, verifying independent streams and clean lifecycle

## Impact

- **Packages modified:** `tddy-core` (Presenter), `tddy-tui` (virtual TUI factory), `tddy-service` (TerminalService per-connection), `tddy-e2e` (acceptance tests)
- **User impact:** Web clients get independent, interactive terminal sessions in daemon mode
- **Risk:** Presenter architecture change — must ensure backwards compatibility with existing TUI mode
