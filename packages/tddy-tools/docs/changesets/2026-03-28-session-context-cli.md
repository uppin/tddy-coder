# 2026-03-28 — Session context CLI

**Type:** Feature

`set-session-context` merges JSON into `.workflow/<id>.session.json` (`TDDY_SESSION_DIR`, `TDDY_WORKFLOW_SESSION_ID`); aligns with `Context::merge_json_object_sync` for `goal_conditions`. See `docs/ft/coder/workflow-json-schemas.md` and this file’s CLI table. (tddy-tools, tddy-core)
