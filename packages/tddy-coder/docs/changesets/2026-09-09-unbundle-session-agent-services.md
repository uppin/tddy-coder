# 2026-09-09 — The session participant gains a third coordinate

`session_participant` served families M and N — `StreamSessionActivity`, `StreamAcpReplay`,
`GetAcpToolCallDetail`, `GetAcpReplayPage` — as arms of its `connection.ConnectionService` dispatch.
They now answer at `activity.ActivityService`, in a module of their own
(`session_participant/activity_service.rs`), registered as a third `ServiceEntry` by
`session_service_entries`.

It had to move in the same PR, for the reason
[2026-08-02-activities-tail-first-autoscroll.md](./2026-08-02-activities-tail-first-autoscroll.md)
records: when the two servers of this family drift, the same session opens tail-first when reached
over HTTP and head-first when reached over LiveKit.

A session on the participant now has **three** coordinates — `connection.ConnectionService`,
`terminal_session.TerminalSessionService` (node 6's) and `activity.ActivityService` — over one
service object. Two comments in `run.rs` still said "two" and were corrected at wrap.

**This is not the shape node 6 used, and the difference is worth knowing.** Node 6's participant
registers `tddy-terminal-rpc`'s *own* entry constructor, so the terminal family has one
implementation and the two servers cannot drift. This one keeps its own replay handlers. Both sides
share `tddy-service`'s paging helpers (`tail_page`, `page_before`, `strip_tool_body`), but the
framing and the `seq` stamping around them are written twice, and
`tests/two_server_parity_acceptance.rs` is what holds them in line — see
[`docs/dev/todo/2026-09-12-the-acp-replay-framing-is-written-twice.md`](../../../../docs/dev/todo/2026-09-12-the-acp-replay-framing-is-written-twice.md).

`coder_activity_entry` is asserted through the public `session_service_entries` seam rather than
directly, so deleting the registration can no longer leave the replay family dark with every test
still green.

Full record: [../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md).
