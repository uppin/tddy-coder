# 2026-11-02 — The agent terminal's output outlives its process

**Type:** Feature

`#session-tabs` 2/7 ([#805](https://github.com/uppin/tddy-coder/pull/805)). A session's `main`
terminal is recorded to `<data dir>/sessions/<id>/terminals/main.capture` — its retained window with
its absolute offsets, rewritten every 30 s and once more at exit — and `GetTerminalHistory(main)` of a
stopped session pages over that file exactly as over the live capture, also from a restarted daemon.
Across:

- `tddy-task` — `CaptureRecording` (`capture_recording.rs`): `of`, `replay_from` (the live capture's
  paging, now `terminal_capture/paging.rs`), a versioned file with atomic replace, `record` (the one
  write path), `RECORDING_INTERVAL`.
- `tddy-pty` — `PtyExitHook` / `PtyRuntime::spawn_with_exit_hook`: work run after exit and before the
  terminal leaves the `PtyRegistry`. `tddy-terminal-rpc`'s `pty_runtime` passes it through.
- `tddy-cli-sessions` — `CliSessionManager::record_main_capture` (`capture_recorder.rs`), periodic
  plus the exit hook; `main_capture_recording_path`.
- `tddy-daemon-sandbox` — the jailed counterpart (`main_recording.rs`), periodic plus a final write in
  `SandboxSessionManager::remove`.
- `tddy-agent-launch` — the claude-cli start and resume and every jailed start/relaunch register the
  recording.
- `tddy-terminal-rpc` — the `RecordedTerminalHistory` port (`recorded_history`), read only when no
  terminal is running; `NoRecordedHistory`.
- `tddy-session-lifecycle` — `SessionDirRecordedHistory`, the daemon's port over the session
  directory. `tddy-coder`'s participant passes `NoRecordedHistory`.

Tests: 7 unit (`capture_recording_unit`), 3 PTY integration (`capture_recorder`), 3 jailed
(`jail_main_recording`), 2 service (`recorded_history_acceptance`), 6 + 2 daemon acceptance
(`agent_terminal_history_recording_acceptance`, `agent_terminal_recording_launch_acceptance`).

Decisions: the latest window, not a log — fixed disk per session and identical paging; periodic plus
at-exit writes, since at-exit alone loses everything on a daemon crash; the final write sits in the
PTY exit path because writing at task-terminal status raced the registry removal (`GetTerminalHistory`
answered `NOT_FOUND` for a moment after exit); a jailed main's teardown point is its state's removal,
a jail that dies on its own is covered by the periodic write; the recording is consulted only when
nothing runs, so a running `main` never reads it.

Code issues: `oversized-file-terminal-capture` 712 → 697 (partially fixed);
`oversized-file-sandbox-session` 946 → 947 (the `remove` call; the rest of the jailed recorder is in
`main_recording.rs`); `tddy-terminal-rpc` `service.rs` 500 → 522, recorded as
`oversized-file-service` — not split, stack-shared with #806 and #810;
`complexity-svc-resume-claude-cli-session` 141 → 142 and `crap-svc-start-sandboxed-cursor-cli-session`
432 → 433 (one call each, no branch); `oversized-file-bridge` unchanged (file not touched).
