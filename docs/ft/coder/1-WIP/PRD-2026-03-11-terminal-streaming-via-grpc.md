# PRD: Terminal Streaming via gRPC

**Date**: 2026-03-11
**Status**: Draft
**Type**: Feature (new capability)

## Affected Features

- [gRPC Remote Control](../grpc-remote-control.md) — adds a new RPC alongside the existing Stream RPC

## Summary

Add a server-streaming gRPC RPC (`StreamTerminal`) to the existing `TddyRemote` service that streams raw PTY bytes of the ratatui TUI output. A remote client receives the exact ANSI escape sequences that the terminal would see, enabling remote TUI rendering in any terminal emulator.

## Background

The existing gRPC `Stream` RPC provides structured `PresenterEvent`s (mode changes, activity logs, state transitions). While useful for programmatic control and E2E testing, it does not provide the actual rendered terminal output. A remote client cannot reconstruct the visual TUI from these events alone.

Use cases that require the rendered terminal:

- **Remote TUI viewer**: A developer wants to observe the TUI from a different machine or process without sharing the physical terminal
- **Future multi-session streaming**: A daemon managing multiple sessions needs to let clients attach to any session's TUI output

The ratatui/crossterm stack produces ANSI escape sequences when rendering. By routing these through a virtual PTY (or equivalent capture mechanism), the raw byte stream can be forwarded to gRPC clients.

## Requirements

### R1: New `StreamTerminal` RPC

Add a server-streaming RPC to the `TddyRemote` service:

```protobuf
rpc StreamTerminal(StreamTerminalRequest) returns (stream TerminalOutput);
```

- **StreamTerminalRequest**: Initially empty (reserves space for future fields like session ID, terminal size)
- **TerminalOutput**: Contains raw bytes (`bytes data`) representing ANSI terminal output, plus optional metadata (sequence number, timestamp)

### R2: Capture ratatui output as raw bytes

Instead of (or in addition to) rendering to the physical terminal, ratatui renders to a capture mechanism that produces raw bytes:

- Use a PTY pair, in-memory pipe, or custom `Write` implementation as the CrosstermBackend target
- Capture the exact bytes crossterm/ratatui writes (cursor movements, colors, text, etc.)
- The capture mechanism must not require a real terminal — enabling headless and multi-session use in the future

### R3: Stream captured bytes to gRPC clients

- Each `TerminalOutput` message carries a chunk of raw bytes from the capture buffer
- Multiple gRPC clients can subscribe to the same terminal stream simultaneously (broadcast pattern)
- Clients that lag behind may miss frames (acceptable — raw bytes are a continuous stream, not discrete frames)

### R4: Terminal size coordination

- The initial `StreamTerminalRequest` may include desired terminal dimensions (rows, cols)
- The server communicates the active terminal size so clients can configure their terminal emulator
- Size changes are communicated via `TerminalOutput` messages (metadata field or dedicated message variant)

## Success Criteria

1. `StreamTerminal` RPC is defined in the proto file and compiled via tonic
2. A gRPC client calling `StreamTerminal` receives a stream of `TerminalOutput` messages containing raw ANSI bytes
3. Piping the received bytes into a terminal emulator renders the same visual output as the local TUI
4. Multiple clients can subscribe simultaneously
5. The capture mechanism works without a physical terminal (headless-capable)
6. Tests verify byte streaming end-to-end using an in-process gRPC client

## Testing Plan

### Acceptance Tests

1. **`stream_terminal_returns_bytes`** — Connect to `StreamTerminal`, receive at least one `TerminalOutput` with non-empty bytes
2. **`streamed_bytes_contain_ansi_sequences`** — Verify received bytes include ANSI escape sequences (CSI codes)
3. **`multiple_clients_receive_same_stream`** — Two clients subscribe simultaneously and both receive output
4. **`stream_terminal_ends_on_quit`** — After the TUI quits, the stream completes
5. **`capture_works_without_physical_terminal`** — Run in headless mode (no real PTY), verify bytes are still produced

## References

- [gRPC Remote Control](../grpc-remote-control.md) — existing gRPC service design
- [ratatui TestBackend](https://docs.rs/ratatui/latest/ratatui/backend/struct.TestBackend.html) — ratatui's built-in test backend (cells, not bytes)
- crossterm `CrosstermBackend<W: Write>` — accepts any `Write` implementor
