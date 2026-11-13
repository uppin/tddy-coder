# 2026-11-13 — A spawner's scope check treats an unreadable changeset as "not yours"

**Category:** Defect (low). Recorded for the `#orchestrators` sweep node by `#orchestrators` 4/11
(#881) during its `/fix-pr` loop; the developer approved fixing only the 4/11 paths there.
**Source:** `packages/tddy-daemon-rpc/src/exec_tool/project_home/caller.rs`
(`refuse_a_spawner_outside_its_own_sessions`), pre-existing since `#home-inbox` 11/11.

The check reads the target's changeset with `.ok()`. So a changeset that exists but cannot be read
looks the same as one naming no orchestrator. The spawner is then refused with "it did not start
<target>", which is a false statement. The read error behind it is never logged or surfaced.

#881 fixed the same pattern in `ProjectNotificationLog::project_of`
(`packages/tddy-session-activity/src/project_notification_log.rs`). There, a missing changeset
still means "no orchestrator", while any other read error is a delivery error.

## What would close it

Match on `read_changeset` as `answers.rs::require_orchestrator` does:
- `Ok` → compare `orchestrator_session_id`.
- `Err(WorkflowError::ChangesetMissing(_))` → the refusal as today.
- Any other error → a refusal (or internal error) that names the unreadable changeset.

Add a test with a corrupt `changeset.yaml`.
