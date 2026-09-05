# 2026-07-25 — terminal-capture-hardening: `EscapeParser::feed` now reports `StreamEvent::{PrivateMode, FullReset}` so RIS (`ESC c`) clears the sticky mouse-mode set instead of letting it survive a hard terminal reset; eviction's escape-sequence chase is bounded by the new public `MAX_SEQUENCE_CHASE_BYTES` (1 KiB), so an unterminated OSC/DCS straddling the trim point can no longer drain the ring to empty (retained output never falls below `CAPTURE_LIMIT_BYTES - MAX_SEQUENCE_CHASE_BYTES`

**Type:** Fix

a fragment beats a blank screen). Docs [terminal-capture.md](../terminal-capture.md). Tests: `terminal_capture_replay` 12. Cross-package [changeset](../../../../docs/dev/changesets/). (tddy-task)
