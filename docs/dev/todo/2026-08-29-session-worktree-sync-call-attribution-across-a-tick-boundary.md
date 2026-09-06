# 2026-08-29 — Session worktree sync — call attribution across a tick boundary

**Category:** Future enhancement
**Source:** activity-log-cut-before-measurement changeset, 2026-08-29

- **A tick boundary between a write and the record describing it still mis-attributes that call.**
  The write is measured by tick N, the row is cut by tick N+1, and N+1's diff no longer names the
  path, so the call's `DELTA_SCOPE_CALL` patch is empty and its bytes reach clients only as tick N's
  residual. The window is now the microseconds between a tool's write and its log append (it was a
  large fraction of every poll interval). Closing it means attributing a record to the most recent
  retained tick whose patch **names one of its declared paths**, falling back to the current tick for
  a call that genuinely changed nothing — which changes what `activity_seq` means on the wire and is
  the feature owner's call.
- **The per-tick ordering is not reachable from a test.** Reproducing the window means appending to
  the activity log *during* a measurement, and the poll loop cannot be built from outside the crate:
  `SessionRoomRegistry::register` is private and a `BroadcastPublisher` cannot be constructed there
  (the wall `session_room_livekit_acceptance.rs`'s module note describes). Extracting the per-tick
  body into a function that takes its measurement and its log cut would make the ordering pinnable
  without LiveKit, a checkout or a timer — the same move that made `tick_activity` testable.
