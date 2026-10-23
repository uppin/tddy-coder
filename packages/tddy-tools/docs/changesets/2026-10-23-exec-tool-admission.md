# 2026-10-23 — tools/list advertises the exec tools the daemon admits
**Type:** Feature

`exec_tool_admission`: the project-home tools are registered; each `tools/list` asks the daemon which exec tools this session is offered and advertises only those, and a failed ask fails `tools/list` with the reason. See the [README](../../README.md#the-mcp-surface) (#700, `#project-home` 5/7). Code issue `oversized-file-server`: 2,832 → 2,856 production lines (row added).
