# Split `remote_agent_launch.rs` once `#target-tmux` lands

**Category:** Refactor (deferred decomposition)
**Source:** `docs/dev/changesets/2026-11-23-target-tmux-agent.md` (#1019)

#1019 grew `packages/tddy-agent-launch/src/remote_agent_launch.rs` from 529 to 623 production lines.
It was not split in that PR because #1020 (`#target-tmux` 2/2) edits the same file, and a split under
a dependent PR turns its diff into conflicts. Do the split after both land; seams and measurements in
`packages/tddy-agent-launch/docs/code-issues/oversized-file-remote-agent-launch.md`.
