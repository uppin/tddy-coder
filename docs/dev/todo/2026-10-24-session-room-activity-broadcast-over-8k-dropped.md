# 2026-10-24 — Agent-activity records over 8 KiB are dropped from the session room

**Category:** Bug (medium)
**Source:** 2026-10-24-project-home-agent-tools (seen while diagnosing main session `01a16dc6-…` on Tddy Desktop)

- Daemon log, same session, twice in two minutes:
  `session_room: session-01a16dc6-… could not broadcast an activity record: broadcast payload of
  21660 bytes exceeds the 8192 byte limit` (`tddy_daemon_livekit::session_room`).
- An activity record carries the tool call's input and result (`agent-activity.jsonl` lines here
  reach tens of KB for a `Bash` whose output is a source excerpt). Any such record is not broadcast,
  so a live Activity pane misses exactly the larger tool calls. The record is still on disk.
- **Why deferred:** unrelated to the main's tool wiring this changeset fixes.
- **What would close it:** broadcast a bounded summary (truncate `result`/`input` past a budget, with
  a `truncated` flag and the `call_id` for a fetch of the full record), or chunk the payload. Test:
  a record whose result is 20 KB is broadcast, under the limit, with `truncated: true`, and the full
  record is fetchable by `call_id`.
