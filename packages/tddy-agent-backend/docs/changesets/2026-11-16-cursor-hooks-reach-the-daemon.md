# 2026-11-16 — Cursor installs its tool-call hooks

**Type:** Fix · PR [#933](https://github.com/uppin/tddy-coder/pull/933)

`CURSOR_HOOK_EVENTS` installs `preToolUse` and `postToolUseFailure` beside the five previous
events: the pre opens each tool call's activity row, and the failure hook — which fires
*instead of* `postToolUse`, never beside it — closes a failed one. The shell, MCP, file-edit
and response hooks stay uninstalled on purpose: each fires alongside the pre/post pair for
the same call, or carries no tool payload at all, so wiring one would duplicate a row the
pair already reports. The settings test pins the seven literal events.
