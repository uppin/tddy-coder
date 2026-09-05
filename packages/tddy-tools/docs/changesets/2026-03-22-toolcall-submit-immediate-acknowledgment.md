# 2026-03-22 — Toolcall submit immediate acknowledgment

**Type:** Feature

Relay writes `SubmitOk` before presenter scheduling; integration test `submit_relay_no_poll` (dev-dependency on `tddy-core` for `start_toolcall_listener` only in tests). (tddy-tools, tddy-core)
