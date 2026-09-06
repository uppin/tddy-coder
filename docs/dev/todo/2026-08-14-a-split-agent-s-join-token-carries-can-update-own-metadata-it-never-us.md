# 2026-08-14 — A split agent's join token carries `can_update_own_metadata` it never uses

**Category:** Future enhancement
**Source:** remote-managed-worktree changeset, 2026-08-14

`tddy_livekit::TokenGenerator` (`packages/tddy-livekit/src/token.rs:50-65`) grants the same set to every
participant it mints for, including `can_update_own_metadata: true`. That grant is what a daemon needs
to publish its advertisement; a split session's agent process never calls `set_metadata` and has no use
for it.

It matters because participant metadata is exactly how peer eligibility is decided
(`eligible_daemon_from_participant_fields`), so the grant is the mechanism by which an agent could
advertise itself as a daemon. That path is now closed by reserving the `split-agent-` identity prefix
in discovery — the robust half — but narrowing the grant would remove the capability rather than filter
its one known use.

Not done here because `TokenGenerator` is shared by every LiveKit participant in the repo and a
narrowed variant belongs in `tddy-livekit`, not in a daemon-side feature. Cheap and worth doing: add a
grants parameter (or a `TokenGenerator::for_agent`) and mint the split token without it.

Related, and larger: the same session-token export means the agent process holds the *user's* full
session token in `TDDY_REMOTE_SESSION_TOKEN`, which authenticates every `ConnectionService` RPC on both
daemons — not just `ExecuteTool` on its own worktree. A session-scoped tool token (audience = this
session, exec-tool methods only) would bound that. Recorded in the PRD's trust model as a known
property rather than an oversight.
