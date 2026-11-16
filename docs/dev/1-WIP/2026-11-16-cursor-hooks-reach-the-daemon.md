# Cursor hooks reach the daemon and map to status and activity

**Type:** Fix
**Packages:** `tddy-coding-agent-specs`, `tddy-agent-backend`, `tddy-session-activity`, `tddy-service`, `tddy-client-web`, `tddy-rust-typescript-tests`, `tddy-tools`, `tddy-tools-cli`, `tddy-changeset`, `tddy-session-lifecycle`, `tddy-agent-launch`

## Problem

A `cursor-cli` session's lifecycle hooks never reached the daemon, for three stacked reasons
(investigated in `../todo/2026-11-16-cursor-hooks-udoo.md`, ranked fixes 1–3 implemented here;
fixes 4–5 and the `turn_input` side finding stay open there):

1. Cursor drops the whole `.cursor/hooks.json`. Its `parseJSONC` strips `//.*$` and `/*…*/`
   blind to string literals, and every command tddy renders carries `--daemon http://…` —
   so each command's string is cut at `http:` and the file fails to parse. Cursor runs the
   session with no project hooks at all, silently.
2. Even past that, a Cursor session's tool calls never appear in `ReadSessionActivity`:
   `agent_activity_request` accepted only PascalCase `PreToolUse`/`PostToolUse`, while Cursor
   sends camelCase `preToolUse`/`postToolUse` with `tool_output` as a JSON *string* and a
   `tool_use_id` — and `preToolUse` was not installed at all.
3. tddy always launches Cursor with `--resume <chat>`, and a resumed launch never fires
   `sessionStart` — so the session never read `Started`, and the PRD promised it anyway.

## Change

**The rendered hooks file escapes every `/` as `\/`.** In `render_cursor_hooks_json`
(`tddy-coding-agent-specs/src/slots/cursor.rs`) the serialized bytes go through one
`replace('/', "\\/")`. `\/` is valid JSON parsing back to `/`, `/` never stands outside a
string in the serialized output, and after the replace neither of Cursor's regexes can
match (no `//` can occur; a `*/` closer needs `*` immediately before `/`, which the
escape makes impossible). A regression test replicates Cursor's `parseJSONC` over the
rendered file and requires it to parse with every installed event's command intact.

**Cursor's tool events map to agent activity.** `CURSOR_HOOK_EVENTS`
(`tddy-agent-backend/src/cursor_hooks.rs`) installs `preToolUse` and `postToolUseFailure`
beside the five previous events. The other Cursor hooks stay uninstalled on purpose:
`beforeShellExecution`/`afterShellExecution`, `beforeMCPExecution`/`afterMCPExecution` and
`afterFileEdit` fire *alongside* the pre/post pair for the same call, so each would
duplicate a row; `afterAgentResponse` carries text, not a tool payload. `postToolUseFailure`
fires *instead of* `postToolUse` (success-only), so it closes the row as an error without
duplicating.

`agent_activity_request` (`tddy-session-activity/src/hook_request.rs`) accepts the three
Cursor names and normalizes the wire `event` to the PascalCase pair the daemon matches —
the wire contract is unchanged. It parses the `tool_output` string into `result_json`
(unparseable text degrades to itself, as on the daemon), derives errors from a non-empty
`error` string or the `isError` flag (always an error on `postToolUseFailure`, with the
`error_message`), and carries `tool_use_id` in a new `ReportAgentActivityRequest.tool_use_id`
field (`activity.proto`, field 10; TS bindings regenerated, drift gate clean).

The daemon (`report_agent_activity_impl`) pairs a named call by its id: the `tool_use_id`
*is* the `call_id` on both rows, so pre and post match by construction — across
interleaved calls and across a daemon restart, where arrival-order pairing answers wrong
or mints a fresh id. Reports without an id (Claude Code) pair by arrival order exactly as
before. Rows of a `cursor-cli` session are sourced `cursor-cli` (from the session type);
every other session keeps writing `claude-cli`. The status mapping is untouched:
`preToolUse`/`postToolUseFailure` are status no-ops, as `preToolUse` always was.

**A started session records `Started` at spawn.** Both CLI-harness starts — the host start
(`opencode_cli_spawn.rs`) and the jailed start (`jail_session_files.rs`) — write
`activity_status: Started` into the first `.session.yaml`, generically across agents: a
Claude start's `SessionStart` hook re-records the same a moment later, while a Cursor
start (whose hook never fires) reads a status from its own first record until its first
prompt's `beforeSubmitPrompt` moves it to `Running`. Daemon-level resume is unchanged
(its status stays whatever the hooks last recorded; resetting it there would clobber a
live tmux re-attach's status, which relaunches nothing).

## Tests

Failing first, then green (TDD):

- `slots::cursor`: the `parseJSONC` regression test (red: the strip broke the parse, as in
  Cursor) plus the byte-exact render updated for the escapes and the seven events.
- `tddy-agent-backend`: the installed-events test pins the seven literal events.
- `hook_request` (new test module): Cursor pre/post/failure mapping, the string
  `tool_output` parse, the `isError` and unparseable-output degradations, the malformed
  payload, and the six excluded hooks staying unreported.
- Daemon pairing (`agent_activity_unit_tests`): a named pair shares one call id, an
  order-breaking interleave pairs by id (red under the stack), an unknown id gets a fresh
  id, cursor-cli rows are sourced `cursor-cli`.
- `tddy-tools-cli`: a Cursor `postToolUse` through the real binary posts both reports with
  the id and parsed result (red: no activity report).
- `tddy-changeset`: the new events stay status no-ops.
- Started-at-spawn: a cursor-cli start records `Started` with a stub agent that runs no
  hook (red: none recorded), and the jailed metadata writer's unit test.
- Live check (this Mac has Cursor's `agent` CLI): a scratch workspace whose escaped
  `.cursor/hooks.json` chains a marker with the debug `tddy-tools` per event, reporting to
  a recording HTTP endpoint. `agent -p "Run the shell command ls, then reply hi" --trust`
  fired `sessionStart preToolUse postToolUse sessionEnd`; the recorded protobufs decode to
  `Started`, a `PreToolUse` row and a `PostToolUse` row sharing one `tool_use_id`,
  `ExecutingTool`, `Ended`. Scratch dir and Cursor side-state removed afterwards.

Companions of the behavior changes: the lifecycle stub decodes `\/` as Cursor's
`JSON.parse` does (it reads the file with sed), the socket extractor in
`agent_tool_socket_per_user_acceptance` decodes the same escape, and the remote-hooks
render list pins all seven Cursor events.

## Docs

- `docs/ft/daemon/cursor-cli-session.md` fixed directly (this PR's scope): the hook table
  now maps all seven events to status *and* activity, records that a resumed launch never
  fires `sessionStart` (the daemon records `Started` at spawn), and the hook-wiring bullet
  records the `\/` escaping and why.
- Package docs for the wrap (not edited here, per the changeset workflow):
  `tddy-coding-agent-specs/docs/coding-agent-specs.md` ("the five Cursor events" → seven,
  plus the escaping), `tddy-session-activity/docs/activity-service.md` (the pairing rule)
  and `tddy-session-activity/docs/agent-activity.md` (a cursor-cli row beside claude-cli's).
- `../todo/2026-11-16-cursor-hooks-udoo.md` narrowed to what is still open (fixes 4–5, the
  `turn_input` side finding).
