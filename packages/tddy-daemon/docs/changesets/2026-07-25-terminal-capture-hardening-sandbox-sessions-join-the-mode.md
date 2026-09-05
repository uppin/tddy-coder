# 2026-07-25 — terminal-capture-hardening: sandbox sessions join the mode-replay contract

**Type:** Fix

`SandboxSessionState.capture` becomes `Arc<Mutex<TerminalCapture>>` (was a raw, **unbounded** `Vec<u8>` that grew for the session's life), the stdio bridge in `sandbox_session.rs` calls `append`, and the sandbox branch of `stream_terminal_output` replays via the new `sandbox_replay_frames` helper (`chunk_terminal_output(&capture.replay(), …)`) so a browser attaching to a long-running sandbox session gets the mouse-tracking prologue instead of silently dropping clicks. Docs [connection-service.md](../connection-service.md#terminal-mode-replay-mouse-tracking). Tests: `sandbox_replay_tests` 1. Cross-package [changeset](../../../../docs/dev/changesets/). (tddy-daemon)
