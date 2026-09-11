# 2026-09-12 — `tddy-session-activity` has no tests, and nothing checks the moved ACP replay output

**Category:** Known gap
**Source:** `#unbundle` node 7, [#476](https://github.com/uppin/tddy-coder/pull/476), changeset
[`2026-09-09-unbundle-session-agent-services`](../changesets/2026-09-09-unbundle-session-agent-services.md)

`cargo test -p tddy-session-activity` runs **zero** tests. The crate has no `tests/` directory and
no `#[cfg(test)]` module, across 1,573 production lines in five modules — `service.rs` (819),
`session_notifications.rs` (403), `streams.rs` (238), `session_notification_subscribers.rs` (76) and
`lib.rs` (37).

Its sibling `tddy-session-agents`, extracted in the same PR, has 47.

The coverage is real but it is all in `tddy-daemon`: `session_notification_bus_unit`,
`session_notification_presenter_unit`, `session_notification_label_unit`,
`session_notifications_acceptance`, `session_notifications_stream_acceptance`,
`session_activity_delta_acceptance`, `stream_agent_activity_delta_rpc_acceptance`,
`session_activity_attribution_acceptance`, `session_activity_wiring_acceptance` and
`agent_activity_stamping_acceptance`. Each is pinned by `ConnectionServiceImpl` or
`test_util::{test_service, TEST_TOKEN}`, which is why they did not move — the same measurement node 6
made for `tddy-session-files` (5 of 14 suites movable) and node 4 before it (5 of 18).

## The specific hole, which is not merely "no tests here"

`acp_replay_parity` has **no equivalent anywhere**. For a move-only change, the check that matters is
that the new coordinate's output equals the old one's, and nothing asserts it:

- `packages/tddy-coder/tests/two_server_parity_acceptance.rs` compares the daemon and the coder
  participant **as they are now**. It cannot see a difference both sides acquired in the move.
- The daemon's ACP suites drive `activity.ActivityService` directly. They assert the *current*
  frames are right, not that they are the frames `connection.ConnectionService` produced.
- `restructure verify --against <pre-move ref>` compares statement multisets, not output.

So a framing change introduced by the relocation — a dropped field on `AcpReplayFrame`, a `seq`
stamped differently, a snapshot boundary moved — would pass every gate this PR ran.

## Why it was deferred

Writing the parity oracle means capturing the old coordinate's output before the move, and the move
had already landed by the time the gap was found at wrap. Node 6 faced the same shape for the sandbox
terminal branch and solved it by carrying the deleted loop verbatim into
`sandbox_terminal_parity_acceptance.rs` as its oracle — that is the pattern, and it has to be set up
*before* the deletion.

## What closing it would take

1. A `tests/` directory in `tddy-session-activity` covering what needs no `ConnectionServiceImpl`:
   the notification classification table, `streams.rs`'s four relays against fake receivers, and
   `DeltaLookup`'s four answers against a stub `SessionDeltaStores`. That is most of the crate.
2. A replay-parity oracle built from the pre-move ref's recorded output, compared frame for frame
   against `StreamAcpReplay`, `GetAcpReplayPage` and `GetAcpToolCallDetail` at the new coordinate.
