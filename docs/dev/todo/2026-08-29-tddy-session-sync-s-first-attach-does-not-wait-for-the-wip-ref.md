# 2026-08-29 — `tddy-session-sync`'s first attach does not wait for the WIP ref

**Category:** Known failing test
**Source:** session-worktree-sync end-to-end suite, 2026-08-29

- `sync::run` calls `first_attach_if_empty` once (`sync.rs:512`); it fetches
  `refs/tddy/session/{id}/wip` immediately, and against a room that has just opened git answers
  "couldn't find remote ref", so `run` returns `SyncError::Git` and exits.
- The daemon's in-process sibling handles exactly this case —
  `session_agent_clone.rs::restore_once_the_session_has_published_its_state`, with a documented
  `FIRST_RESTORE_WAIT` of 45 s. The standalone client has no equivalent.
- An operator starting the CLI against a session whose room is still warming sees the same exit.
  The two consumers of one contract should not disagree about whether it needs a retry.
