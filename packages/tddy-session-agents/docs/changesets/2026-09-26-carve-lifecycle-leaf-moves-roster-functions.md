# 2026-09-26 — Holds the roster and clone functions the session host delegates to

**Type:** Refactor

`#carve` 15/21 ([#526](https://github.com/uppin/tddy-coder/pull/526)); cross-package entry:
[2026-09-26-carve-lifecycle-leaf-moves.md](../../../../docs/dev/changesets/2026-09-26-carve-lifecycle-leaf-moves.md).

From `tddy-session-lifecycle`: `clone_readiness`, `exec_tool_caller` (`authorize_exec_tool_caller`),
`agent_records`, and the `self`-free tails of eleven `DaemonSessionHost` methods as free functions
(`agent_clone_lookup`, `agent_clone_worktree`, `conversation_open_forward`,
`conversation_cancel_forward`, `departed_daemon`, `session_room_participants`, `roster_broadcast`,
`opened_session_room`, `spawn_agent_def`, `hosted_clone_start`). Where they read host fields they
take `AgentRosterState<'a>` (`agent_roster_state`), a hand-written view of ten fields borrowed for
one call. New dependencies `tddy-model-registry` and `tddy-projects`; no path to lifecycle.
Production lines 3,592 → 4,132; tests 72 passed, unchanged (none of the moved code had tests). See
[session-agent-roster.md](../session-agent-roster.md#roster-and-clone-code-the-session-host-calls).
The two code issues (`service.rs`) were not touched.
