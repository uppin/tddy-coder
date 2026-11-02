# 2026-11-02 — A session's main terminal is recorded to disk

**Type:** Feature

`cli_session_manager/capture_recorder.rs`: `CliSessionManager::record_main_capture(session_id, path)`
rewrites the running `main` capture every `RECORDING_INTERVAL` (`with_recording_interval`
overrides) and once more from the PTY exit hook `spawn_tool` installs for `main`, before the
terminal leaves the registry. `main_capture_recording_path` / `MAIN_CAPTURE_RECORDING`
(`<data dir>/sessions/<id>/terminals/main.capture`) live here. Tests: `tests/capture_recorder.rs` (3,
real PTY). See [module-layout.md § Recording the main terminal](../module-layout.md#recording-the-main-terminal). `#session-tabs` 2/7, cross-package entry `docs/dev/changesets/2026-11-02-session-tabs-agent-history.md`.
