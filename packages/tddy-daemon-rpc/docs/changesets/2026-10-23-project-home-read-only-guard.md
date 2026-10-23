# 2026-10-23 — Project-home sessions are read-only at ExecuteTool
**Type:** Feature

`exec_tool/role_guard.rs`: `ExecuteTool` and `StreamExecuteTool` refuse every non-read-only tool for a `project-home` session (`PERMISSION_DENIED`), and refuse when the session's metadata cannot be read; the check and the path-traversal check are one call, so `stream_execute_tool` (open code issue) is unchanged at 81 lines. See [architecture](../architecture.md) (#699, `#project-home` 4/7).
