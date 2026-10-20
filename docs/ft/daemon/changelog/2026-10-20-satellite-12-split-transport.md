# 2026-10-20 — Split placement no longer names LiveKit in its own code

`#satellite` 12/13. Starting a split session (agent on one daemon, checkout on another), its room
over the remote checkout and the agent's room join behave as they did before, and a daemon without
LiveKit still refuses the placement. The split-session code reaches the room through the daemon's
extension ports, and the LiveKit integration implements them. See
[Remote managed worktree](../remote-managed-worktree.md).
