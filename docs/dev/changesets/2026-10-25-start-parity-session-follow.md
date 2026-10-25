# 2026-10-25 — ReadSessionActivity and ReadSessionTranscript follow a session, paged

**Type:** Feature

`#start-parity` 6/9 ([#755](https://github.com/uppin/tddy-coder/pull/755)). A project-home session
can follow a session of its project on any eligible host: `ReadSessionActivity` pages its tool
calls forward by `activity_seq` (so a poll from the last `next_cursor` returns only what is new)
with the question it waits on, and `ReadSessionTranscript` pages the frames the web's Activities
view pages, newest first, each cut at 4 KiB and marked, or returns one tool call whole by id. Both
share one paging contract — `cursor`, `limit` (50 by default, at most 200, clamped with
`clamped_to`), `items`, `next_cursor`, `has_more` — and neither has an unpaged call. Developer
decision: **the pending question is persisted**, in its own `pending-question.json` beside the
`pending_elicitation` flag rather than as a `SessionMetadata` field, by two writers: a tool
session's process from its presenter's mode, and the daemon from a claude-cli hook's
`WaitingForInput` report, which now carries the event's `message`
(`ReportSessionStatusRequest.message = 5`). Across:

- `tddy-tool-engine` — `session_follow.rs`: the tools, paging and truncation; port methods and dispatch in `project_home_tools`.
- `tddy-daemon-rpc` — `exec_tool/project_home_session_follow.rs`: the reads on the host.
- `tddy-changeset` — `pending_question.rs`; `read_all_agent_activity` (the uncapped read); `HookEvent.message`.
- `tddy-service` — the proto field; `tddy-web` / `tddy-rust-typescript-tests` `activity_pb.ts` regenerated.
- `tddy-tools` — `session_hook::session_status_request` sends the message.
- `tddy-session-activity` — `ReportSessionStatus` records and clears the question.
- `tddy-coder` — `session_participant::pending_question`, started by `run_daemon`.
- `tddy-daemon`, `tddy-session-lifecycle`, `tddy-telegram-control` — tests set the new field.

Code issues (split deferred, stack-shared): `tddy-daemon-rpc/src/exec_tool/project_home.rs`
708 → 730, `tddy-tool-engine/src/project_home_tools.rs` 601 → 632, `tddy-tool-engine/src/lib.rs`
881 → 882; `tddy-coder/src/run.rs` 2,620 → 2,626 (already over; recorded). TODO added: a
claude-cli session's transcript has no conversation text.
