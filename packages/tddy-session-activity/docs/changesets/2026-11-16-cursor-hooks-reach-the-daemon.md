# 2026-11-16 — Cursor tool events map to agent activity, paired by tool_use_id

**Type:** Fix · PR [#933](https://github.com/uppin/tddy-coder/pull/933)

`agent_activity_request` accepts Cursor's `preToolUse`/`postToolUse`/`postToolUseFailure`
and normalizes the wire `event` to the PascalCase pair the daemon matches. It parses the
`tool_output` string into `result_json` (unparseable text degrades to itself), derives errors
from a non-empty `error` string or the `isError` flag (always an error on
`postToolUseFailure`, with the `error_message`), and carries `tool_use_id`.
`report_agent_activity_impl` pairs a named call by its id — the `tool_use_id` *is* the
`call_id` on both rows — so pre and post match by construction across interleaved calls and
daemon restarts; reports without an id pair by arrival order exactly as before. Rows of a
`cursor-cli` session are sourced `cursor-cli` from the session type; every other session
keeps writing `claude-cli`. The status mapping is untouched: both new events are status
no-ops.
