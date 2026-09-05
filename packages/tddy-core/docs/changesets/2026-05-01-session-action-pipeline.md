# 2026-05-01 — Session action pipeline

**Type:** Feature

**`session_action_pipeline`**: env merge, **`args`/`env`** envelope, **`glob`** resolution, channel manifest (**`stdout`**, **`stderr`**, **`logs`**), mapper/transform/primary subprocess helpers, **`TDDY_SESSION_CHANNEL_MANIFEST_JSON`**, transform output validation (**`jsonschema`**). Feature doc: [session-actions.md](../../../../../docs/ft/coder/session-actions.md) (pipeline section); architecture: [architecture.md](../../architecture.md#session-action-pipeline-session_action_pipeline). Tests: **`session_action_resolve_unit`**, **`session_action_pipeline_integration`** (via **tddy-tools**). (tddy-core, tddy-tools)
