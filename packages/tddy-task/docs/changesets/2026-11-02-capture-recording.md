# 2026-11-02 — A capture's window on disk: `CaptureRecording`

**Type:** Feature

`capture_recording.rs` adds `CaptureRecording { start_offset, end_offset, data }`: `of(&capture)`,
`replay_from` (paged by the same `terminal_capture/paging.rs::forward_chunk` the live capture now
uses), a versioned file format (`TDDYCAP\0`, version 1) with `write_atomically` / `read`, and
`record(&Mutex<TerminalCapture>, &Path)`, the write every recorder calls. `RECORDING_INTERVAL` (30 s)
is shared by the CLI and jailed recorders. `terminal_capture.rs` went from 712 to 697 production
lines (the paging moved out). Tests: `tests/capture_recording_unit.rs` (7). See
[terminal-capture.md § Recording](../terminal-capture.md#recording-srccapture_recordingrs). `#session-tabs` 2/7, cross-package entry `docs/dev/changesets/2026-11-02-session-tabs-agent-history.md`.
