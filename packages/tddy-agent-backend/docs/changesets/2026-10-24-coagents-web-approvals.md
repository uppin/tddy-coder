# 2026-10-24 — A caller's record of a tool call on the ACP update stream

**Type:** Feature

`#coagents` 8/8 ([#727](https://github.com/uppin/tddy-coder/pull/727)). `AcpUpdate::ToolCallRecord { id, title, kind, status, raw_input, raw_output }` — a whole tool call as a caller keeps it, superseding an earlier record of the same id. A coding subagent records a permission request this way. Not progress: `report_progress` ignores it. See [architecture](../architecture.md).
