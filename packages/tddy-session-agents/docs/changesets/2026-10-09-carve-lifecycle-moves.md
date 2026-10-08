# 2026-10-09 — The agent roster's daemon side moved in

**Type:** Architecture

`#carve` 21/21 ([#536](https://github.com/uppin/tddy-coder/pull/536)): `AgentRoster`, `AgentHostCallbacks`,
the clone provisioning, hosted-clone start and room modules, the seeded-clone guard and `peer_session_answer`
arrived from `tddy-session-lifecycle`; 4,132 to 6,606 production lines, 75 tests (unchanged). New edge:
`tddy-subagent-worktree`. See [agent-clone-roster.md](../agent-clone-roster.md).
