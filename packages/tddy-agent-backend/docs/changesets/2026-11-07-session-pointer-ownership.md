# 2026-11-07 — The session pointer records its owner and is not overwritten while held

**Type:** Feature

`#agent-tools` 21/28 ([#829](https://github.com/uppin/tddy-coder/pull/829)).

- `session_pointer.rs`: `SessionPointer.owner: Option<SessionPointerOwner { pid, host }>` (omitted when `None`; an ownerless pointer reads as before), `SessionPointerOwner::{this_process, is_on_this_host, is_running}` (signal 0, `EPERM` counts as running), `this_host()` (`gethostname`, asserts on failure), `SessionPointerHeld`.
- `turn_files.rs`: `write_session_pointer` refuses a pointer for another socket whose owner is alive on this host, as `BackendError::InvocationFailed(held.to_string())`; a stale, ownerless, foreign-host or same-socket pointer is replaced.
- `SpecHeadlessBackend` writes its turn pointer owned by this process.
- `McpServerSpec.cwd` and `HeadlessMcpServer.cwd: Option<PathBuf>`; the `TODO(headless-driver)` on `HeadlessCommand::env` is gone with the tool-server env it described.
- Tests: `tests/session_pointer.rs`.

## Restructuring (engine-driven, `tddy-tools restructure move_item`)

| File | Production lines (to the first `#[cfg(test)]`) | Moved out |
|---|---|---|
| `backend/acp_driver.rs` | 507 → 451 | the permission types — `PermissionOptionView`, `PermissionRequest`, `PermissionDecision`, `PermissionHandler`, `AllowOnce` — into `backend/acp_driver/permission.rs` (58), re-exported by glob |
| `backend/spec_headless.rs` | 654 → 451 | `read_turn`, `open_conversation_log` and the stderr reader into `backend/spec_headless/turn_read.rs`; `turn_pointer` builds a turn's session pointer |

## Code issues

- **Closed** `oversized-file-acp-driver` — 504 at detection (#822), 507 at this PR's base-plus-change, **451** after the split. Record deleted.
- **Closed** `oversized-file-spec-headless` — 653 on `origin/master`, **451** after the split. Record deleted.
- Narrowed `complexity-spec-headless-invoke` — `invoke` 104 → 91 (`turn_for` 67 unchanged).
- Moved `complexity-spec-headless-read-turn` → `complexity-spec-headless-turn-read-read-turn` (141, unchanged).
- `oversized-file-headless-driver` 509 → 505, still open.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-session-pointer-ownership.md).
