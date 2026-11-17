# 2026-11-16 — Cursor hooks reach the daemon and map to status and activity

**Type:** Fix · PR [#933](https://github.com/uppin/tddy-coder/pull/933)

Implements ranked fixes 1–3 of [2026-11-16 Cursor drops tddy's whole `.cursor/hooks.json`](../todo/2026-11-16-cursor-hooks-udoo.md)
(fixes 4–5 and the `turn_input` side finding stay open there; the entry is narrowed to them, not deleted).

**The rendered hooks file parses again.** Cursor's `parseJSONC` strips `//.*$` and `/*…*/`
blind to string literals, and every command tddy rendered carried `--daemon http://…` — so
each command's string was cut at `http:`, the file failed to parse, and Cursor ran the
session with no project hooks at all, silently. `render_cursor_hooks_json` escapes every `/`
as `\/` (valid JSON parsing back to `/`; `/` never stands outside a string in the serialized
output), after which neither regex can match. A regression test replicates Cursor's transform
over the rendered file and requires every installed event's command intact.

**Cursor's tool events map to agent activity.** `CURSOR_HOOK_EVENTS` installs `preToolUse`
and `postToolUseFailure` beside the five previous events; the shell, MCP, file-edit and
response hooks stay uninstalled (each fires alongside the pre/post pair for the same call, or
carries no tool payload, so wiring one would duplicate a row). The mapping accepts the three
Cursor names, normalizes the wire `event` to the PascalCase pair, parses the string
`tool_output` into `result_json`, and carries `tool_use_id` (`ReportAgentActivityRequest`,
field 10; TS bindings regenerated). The daemon pairs a named call by its id — the `tool_use_id`
*is* the `call_id` on both rows — so interleaved calls and daemon restarts pair correctly;
unnamed reports pair by arrival order as before. `cursor-cli` rows are sourced `cursor-cli`.
The status mapping is untouched.

**A started session records `Started` at spawn.** Every tddy Cursor launch resumes its chat,
and a resumed launch never fires `sessionStart`; both CLI-harness starts write
`activity_status: Started` into the first `.session.yaml` instead of waiting for the hook.
[Cursor Agent CLI Session](../../ft/daemon/cursor-cli-session.md) maps all seven events to
status *and* activity and records the resume truth.

Measurement noted at wrap: `start_cli_harness_session` 368 → 378 (worse by 10), recorded in
its complexity record; restructuring stays with the `#agent-tools` sweep as deferred.
