# PRD: Ghostty Terminal Integration via LiveKit

**Status:** WIP
**Date:** 2026-03-13

## Summary

Integrate the ghostty-web terminal emulator into the tddy-web dashboard, streaming terminal output from tddy-demo over LiveKit and sending keyboard/mouse input back. Includes a standalone Storybook component for development/demo and a Cypress E2E test that validates rendered terminal content through the full stack (tddy-demo + LiveKit testkit + Storybook).

## Background

- **ghostty-web** (`ghostty-web` npm package): WASM-compiled VT100 terminal emulator with xterm.js-compatible API. Analyzed in `tmp/ghostty-web/`.
- **StreamTerminal RPC** (`remote.proto`): Server-streaming gRPC RPC that broadcasts raw ANSI terminal bytes from tddy-coder/tddy-demo to clients.
- **LiveKitTransport** (`tddy-livekit-web`): ConnectRPC Transport implementation over LiveKit data channels. Already supports server-streaming RPCs.
- **tddy-demo**: Same app as tddy-coder with StubBackend — produces deterministic TUI output without a real AI agent.
- **tddy-web**: React dashboard with Storybook (v9) and Cypress (component + e2e).

Currently, `StreamTerminal` exists as a gRPC RPC but has no web consumer. The web terminal feature doc (`docs/ft/web/web-terminal.md`) describes a WebSocket-based approach for a generic shell — this PRD replaces that with LiveKit-based streaming of the tddy TUI.

## Affected Features

- [web-terminal.md](../web-terminal.md) — Superseded by this LiveKit-based approach
- [grpc-remote-control.md](../../coder/grpc-remote-control.md) — Uses StreamTerminal RPC
- [PRD-2026-03-11-livekit-participant.md](../../coder/1-WIP/PRD-2026-03-11-livekit-participant.md) — LiveKit infrastructure

## Requirements

### 1. Proto: New StreamTerminalIO Bidirectional RPC

Add a new bidirectional streaming RPC to `remote.proto` that combines terminal output and input in a single connection:

```protobuf
rpc StreamTerminalIO(stream TerminalInput) returns (stream TerminalOutput);

message TerminalInput {
  bytes data = 1;  // raw keyboard/mouse bytes (escape sequences)
}
```

This allows the web client to send keyboard/mouse input back to the running terminal session.

### 2. GhosttyTerminal React Component (packages/tddy-web)

A standalone React component wrapping ghostty-web:

- **Props**: terminal content (ANSI strings), dimensions (cols/rows), theme, fontSize
- **Events**: `onData` (keyboard/mouse input), `onResize`, `onBell`, `onTitleChange` — exposed as Storybook actions
- Initializes ghostty-web WASM, creates Terminal, renders into a container
- Provides imperative handle for `write()` and `clear()`
- Does NOT depend on LiveKit — purely a terminal rendering component

### 3. Storybook Stories

Stories in `packages/tddy-web` demonstrating:

- Terminal with static ANSI content (passed as args)
- Terminal with colored output, cursor movement examples
- Keyboard/mouse events logged as Storybook actions

### 4. E2E Test (Cypress)

A Cypress e2e test that:

1. Starts a reusable LiveKit testkit server instance
2. Starts tddy-demo (StubBackend) producing terminal output
3. Loads a Storybook story that connects to the terminal stream
4. Asserts that specific text content is visible in the ghostty terminal canvas (extracted from the terminal buffer, not pixel-level)

### 5. LiveKit Bridge for StreamTerminalIO (Rust side)

Implement `StreamTerminalIO` handler in `tddy-livekit` RpcService that:

- Subscribes to terminal byte broadcast (same as existing StreamTerminal)
- Forwards incoming `TerminalInput` bytes back to the PTY/terminal process

## Acceptance Criteria

1. `GhosttyTerminal` component renders ANSI content passed as props
2. Storybook story shows terminal with example content; keyboard events appear as actions
3. E2E test passes: tddy-demo output is visible in ghostty terminal through LiveKit
4. `StreamTerminalIO` RPC works over LiveKit transport (server-streaming output + client-streaming input)
5. No new external dependencies beyond `ghostty-web` npm package in tddy-web

## Out of Scope

- Authentication / access control
- Session persistence / reconnection
- Multi-terminal support
- Terminal resize negotiation over LiveKit (deferred)
