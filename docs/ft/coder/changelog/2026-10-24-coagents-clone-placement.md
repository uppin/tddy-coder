# 2026-10-24 — A subagent can work on its own clone of the session worktree

`#coagents` 5/8 ([#724](https://github.com/uppin/tddy-coder/pull/724)).

- **Attach takes a placement**: `shared` (default, the session worktree) or `clone`. Both pickers
  offer it per agent and the Agents-tab row shows it; re-attaching under the other placement is
  refused naming the one it holds.
- **A `clone` conversation the daemon runs works on a worktree and branch of its own**, cut at open
  and synced with the main agent's files before every turn.
- **A coding agent in a clone** starts in that worktree and its exec tools run there; each turn's
  changes are one commit, and a merged sync is announced on its next prompt.
- **`subagent_diff`, `subagent_pull` and `subagent_end`** read and take a daemon-run `clone`
  conversation's work; a `shared` one has nothing of its own to take, and the descriptions say so.

Feature doc: [Managed-codebase subagents](../managed-codebase-subagents.md#a-daemon-run-conversation-of-a-clone-placed-agent).
