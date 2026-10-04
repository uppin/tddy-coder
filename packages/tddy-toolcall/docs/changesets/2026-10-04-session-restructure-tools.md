# 2026-10-04 — `toolcall::restructure`: the port for a session's restructure tools

**Type:** Feature

`#live-plan` 14/15, PR [#573](https://github.com/uppin/tddy-coder/pull/573). Cross-package entry:
[2026-10-04-session-restructure-tools.md](../../../../docs/dev/changesets/2026-10-04-session-restructure-tools.md).

`src/toolcall/restructure.rs` adds the `TDDY_RESTRUCTURE_TOOLS` gate (`RESTRUCTURE_TOOLS_ENV`,
`restructure_tools_enabled`), the six tool names (`RESTRUCTURE_TOOL_NAMES`, `is_restructure_tool`), the
`RestructureExecutor` trait and its first-wins registry (`register_restructure_executor`,
`restructure_executor`) — the `LspExecutor` registry's shape, re-exported as `tddy_core::toolcall`.
