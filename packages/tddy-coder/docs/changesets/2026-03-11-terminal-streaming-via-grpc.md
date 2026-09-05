# 2026-03-11 — Terminal Streaming via gRPC

**Type:** Feature

When --grpc set: create broadcast channel for terminal bytes, ByteCallback that sends to broadcast, pass to run_event_loop and TddyRemoteService.with_terminal_bytes. CapturingWriter captures ratatui output; gRPC clients receive raw ANSI bytes via StreamTerminal. (tddy-coder)
