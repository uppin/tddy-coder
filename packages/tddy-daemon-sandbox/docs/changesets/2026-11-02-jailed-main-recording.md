# 2026-11-02 — A jailed session's main terminal is recorded to disk

**Type:** Feature

`main_recording.rs`: `SandboxSessionManager::record_main_capture(session_id, path)` and
`with_recording_interval`, over a `JailRecorder` that rewrites the sandbox state's capture every
`RECORDING_INTERVAL` and once more inside `SandboxSessionManager::remove`. `sandbox_session.rs` holds
only the field and the `remove` call (946 → 947 production lines, recorded in
`code-issues/oversized-file-sandbox-session.md`). Tests: `tests/jail_main_recording.rs` (3). See
[task-and-action-services.md § Recording a jailed main](../task-and-action-services.md#recording-a-jailed-main). `#session-tabs` 2/7, cross-package entry `docs/dev/changesets/2026-11-02-session-tabs-agent-history.md`.
