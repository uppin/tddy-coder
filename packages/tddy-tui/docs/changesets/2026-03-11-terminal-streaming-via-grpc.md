# 2026-03-11 — Terminal Streaming via gRPC

**Type:** Feature

CapturingWriter with ByteCallback for capturing ratatui/crossterm output. run_event_loop accepts optional byte_capture; uses CapturingWriter (no-op when None) for all terminal writes. Unit tests: write_captures_bytes, clone_shares_callback, flush_delegates_to_stdout. (tddy-tui)
