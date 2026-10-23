# 2026-10-23 — Project-home tools served by the exec-tool handler
**Type:** Feature

`exec_tool/project_home.rs`: the four project-home tools are answered on the daemon, `PERMISSION_DENIED` unless the caller's record carries the `project-home` role, over the host's own list and start (late-bound through `DaemonRpcFamilies::attach_session_start`); a started session is tagged with the home session as orchestrator. `ListExecTools` answers per session (`offered_catalog`). See [architecture.md](../architecture.md) (#700, `#project-home` 5/7). Code issues: `list_exec_tools` 78 → 72 lines, `stream_execute_tool` 81 → 80 (rows added).
