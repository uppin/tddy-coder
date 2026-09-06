# 2026-08-29 — A session's first delta is numbered 0, which is also the wire's "no tick yet" sentinel

**Category:** Known failing test
**Source:** session-worktree-sync end-to-end suite, 2026-08-29

- `session_room.rs:1563` starts `PollState::next_delta_seq` at **0**, and
  `tddy-session-sync`'s `decide_record` (`sync.rs:250`) reads `activity_seq == 0` as
  `IgnoreReason::NoTickYet` — "no poll tick has measured this call yet".
- So the **first** change any session makes produces a delta no mirror can act on. Its content
  reaches the mirror only at the next reconcile, not from the delta that carries it.
- Both LiveKit suites work around it by warming the room before a client attaches, and
  `session_room_livekit_acceptance.rs` already documents the collision. A warm-up ritual in every
  consumer is the wrong fix: start `next_delta_seq` at 1 and let 0 mean only the sentinel.
