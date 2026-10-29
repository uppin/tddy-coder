# 2026-10-29 — A conversation spawner may answer its child's permission prompt but cannot see it

**Category:** Gap (medium) — found at `/validate-changes` of `#home-inbox` 6/8 (#771)
**Source:** `packages/tddy-tool-engine/src/project_home_tools.rs` (`is_conversation_spawner_tool`)

#771 serves a session that can spawn conversations (a `grill-me` session, `#start-parity` 9/9)
`AnswerSessionPermission` beside its four start tools, because the PRD lets any session a child's
changeset names as `orchestrator_session_id` answer it. But the `request_id` the tool needs is read
with `ReadSessionActivity`, which is a project-home tool only. So a spawner's claude-cli child, on
every permission prompt, waits the whole `claude_cli.answer_wait_secs` (default 120 s) for an
answer its orchestrator has no way to address, and only then shows Claude's own prompt.

**Why deferred:** widening a spawner's read surface (or pushing the pending question to it) is a
decision about what a grill-me session may see of other sessions, not part of #771's answer path.
`#home-inbox` 7/8 (home-wait) delivers a home's children's pending questions to the home; whether a
spawner gets the same is open.

## What would close it

Either serve a spawner `ReadSessionActivity` for the sessions it started (its own project, its own
`orchestrator_session_id`), or have the child not wait when its orchestrator is a spawner that cannot
be told — decided by the developer. Test: a grill-me spawner reads its child's pending
`request_id` and answers it; or the child of a spawner gets `NO_ORCHESTRATOR`.
