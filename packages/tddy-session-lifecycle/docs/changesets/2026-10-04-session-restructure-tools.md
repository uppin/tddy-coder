# 2026-10-04 — The jail env carries `TDDY_RESTRUCTURE_TOOLS` when the host can serve the tools

**Type:** Feature

`#live-plan` 14/15, PR [#573](https://github.com/uppin/tddy-coder/pull/573). Cross-package entry:
[2026-10-04-session-restructure-tools.md](../../../../docs/dev/changesets/2026-10-04-session-restructure-tools.md).

`restructure_tools_env` (`jail_env_builders.rs`) exports `TDDY_RESTRUCTURE_TOOLS=1` only when an executor is
registered, and is merged beside `lsp_tools_env` in the sandboxed Claude and Cursor starts and the runner
relaunch.
