# 2026-11-24 — Close a session: terminate every tab, remove every worktree

**Type:** Feature

`every_agent_clone_of` reads the record as well as the store (an unreadable record is an error); `tear_down_agent_clone` forgets the clone from `agents[]` (`SessionAgentRosterStore::forget_clone`); `peer_does_not_hold_the_session`; `close_every_conversation_of` with each interrupted turn's `ClosedTurnLoop`. Full record: [`docs/dev/changesets/2026-11-24-session-close-and-remove.md`](../../../../docs/dev/changesets/2026-11-24-session-close-and-remove.md) (#1032).
