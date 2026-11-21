# 2026-11-21 — Access requests

**Type:** Feature

- `tddy_session_activity::path_access`: `PathAccessRequests`, an orchestrator's requests held for the
  operator; Allow writes a `PathGrant` per path. `ActivityService.{ListPathAccessRequests,
  AnswerPathAccess}` serve them.

See [project-concept.md § Access requests](../../../../docs/ft/daemon/project-concept.md#access-requests). Full record: [`docs/dev/changesets/2026-11-21-access-elicitation.md`](../../../../docs/dev/changesets/2026-11-21-access-elicitation.md).
