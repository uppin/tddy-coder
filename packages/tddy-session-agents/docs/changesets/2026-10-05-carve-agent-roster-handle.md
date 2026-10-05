# 2026-10-05 — `AgentRosterState` carries the admission registry and the model registry

**Type:** Architecture

`AgentRosterState<'a>` has twelve fields, not ten: `session_admissions` (the owning daemons this daemon,
as the facilitating one, has admitted to its session rooms) and `model_registry` (this daemon's model
registry, when one is wired). The agent topic's bodies in `tddy-session-lifecycle` read them (tearing down a
clone, and resolving an agent id to a registry assistant), and they are what lets that topic run over the
state and an owned handle instead of the host. Both field types are already named by this crate's
dependencies; no crate edge was added. Nothing else in the crate changed and nothing moved into it.

`./test -p tddy-session-agents`: 75 passed (47, 3 and 25 across its targets); the change added no test. The
state and the roster methods that use it are described in [session-agent-roster.md](../session-agent-roster.md);
the lifecycle side is in
[`2026-10-05-carve-agent-roster-handle`](../../../tddy-session-lifecycle/docs/changesets/2026-10-05-carve-agent-roster-handle.md).

No `docs/code-issues/` record names `agent_roster_state.rs`, so none was re-measured.
