# 2026-07-22 — reusable-lsp: register the `TddyLspExecutor` at startup sharing the daemon `TaskRegistry` + a 60s idle-reaper loop; `lsp_tools_env(worktree)` sets `TDDY_LSP_TOOLS` at the sandboxed-session env sites; `relay_idle::IdleTimeoutTracker` now re-exports `tddy_task::IdleTimeoutTracker`. Cross-package [changeset](../../../../docs/dev/changesets/). PR [#310](https://github.com/uppin/tddy-coder/pull/310).

**Type:** Feature


