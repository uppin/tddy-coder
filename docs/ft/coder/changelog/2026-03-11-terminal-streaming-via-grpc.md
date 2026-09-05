# 2026-03-11 — Terminal Streaming via gRPC

- **StreamTerminal RPC**: Server-streaming RPC on TddyRemote service streams raw ANSI bytes from ratatui/crossterm rendering. Clients receive the exact byte stream a terminal would see.
- **CapturingWriter**: tddy-tui captures terminal writes via custom Write implementation; `run_event_loop` accepts optional `ByteCallback`; no-op when not provided.
- **Wiring**: When `--grpc` is set, tddy-coder creates broadcast channel, passes callback to event loop and `TddyRemoteService::with_terminal_bytes`.
- **Use case**: Remote TUI viewer — pipe received bytes into a terminal emulator to render the TUI remotely.
- **Packages**: tddy-tui (CapturingWriter, event_loop byte_capture), tddy-grpc (StreamTerminal proto, service, daemon stub), tddy-coder (run.rs wiring).
