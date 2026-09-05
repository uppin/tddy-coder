# 2026-06-26 — **PTY terminal width fix

**Type:** Fix

connection.proto** — `StreamTerminalOutputRequest` gains `initial_cols` (uint32, field 4) and `initial_rows` (uint32, field 5); zero means "use PTY default"; TypeScript codegen regenerated. Feature [terminal-sessions.md](../../../../docs/ft/daemon/terminal-sessions.md). (tddy-service)
