# 2026-11-21 — Access requests

**Type:** Feature

- `tddy_tool_engine::path_access_tools`: the `RequestPathAccess` tool, the `PathAccessPort` the daemon
  implements (`paths_outside_reach`, `request_path_access`) and `execute_path_access_tool`, which asks
  only for the paths outside the caller's reach and answers `in_reach` when nothing needed asking.

See [project-concept.md § Access requests](../../../../docs/ft/daemon/project-concept.md#access-requests). Full record: [`docs/dev/changesets/2026-11-21-access-elicitation.md`](../../../../docs/dev/changesets/2026-11-21-access-elicitation.md).
