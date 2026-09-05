# 2026-08-15 — A split agent addresses the daemon that is actually in its room

- **A split session's agent is wired to the RPC identity hosting the room it joins** — the facilitating daemon — instead of the codebase daemon, which hosts no room and joins none. Since the move to per-session rooms, every tool call a split agent made waited out its timeout for a participant that would never arrive. See [remote-managed-worktree.md](../remote-managed-worktree.md).
- **`TDDY_REMOTE_DAEMON_INSTANCE_ID` is unchanged and still names the codebase daemon**: it is the forwarding hint the room's host routes on to reach the checkout, not the identity to call. Naming the codebase host in both places is what conflated the two.
- **The room's host identity now travels with the room name** in the value the daemon resolves once and both the start and resume paths read, so an agent cannot be pointed at a participant that is not in the room it was given.
