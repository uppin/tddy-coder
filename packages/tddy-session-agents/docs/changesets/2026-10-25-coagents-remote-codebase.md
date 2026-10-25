# 2026-10-25 — Workspace roster admission

**Type:** Feature

`workspace_roster::workspace_roster_admits(record, local_daemon_instance_id)`: a workspace session's roster
holds coding agents of the serving daemon only; a model agent or another daemon's agent is
`INVALID_ARGUMENT` naming it. `#coagents` 6/8, [#725](https://github.com/uppin/tddy-coder/pull/725);
see [remote-codebase mode](../../../../docs/ft/daemon/remote-codebase-mode.md#coding-subagents-on-the-codebase-host).
