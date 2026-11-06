# 2026-11-06 — A CLI session's pointer on an ssh target is not git-excluded or removed

**Category:** Gap (found while fixing #823's own regression)
**Owner:** `#agent-tools` n17 `cli-dispatch-collapse` ([#823](https://github.com/uppin/tddy-coder/pull/823)); the sweep node (n20) or the next change to the ssh slot write
**Source:** `packages/tddy-agent-launch/src/cli_session_pointer.rs` (`place_session_pointer`, `TODO(cli-dispatch-collapse)`)

The generic CLI launch writes the `.tddy/session.json` pointer (the managed workflow's socket and the
operator-notify session) into the session's worktree, so a `tddy-tools` the agent runs from its own
shell finds its session with no `TDDY_*` variable in the agent's environment. On this host the file is
added to the repository's `info/exclude` first, written owner-only, and removed when the agent exits
and when the session is deleted.

For a checkout on an ssh target, the pointer is written on the target through the bounded slot-file
write (`slot_file_over_ssh`, `SLOT_FILE_OVER_SSH_DEADLINE`), and nothing else:

1. **Not git-excluded.** `exclude_from_git` runs `git` on this host; the target's `info/exclude` is
   not updated, so the pointer is an untracked file an agent's `git add -A` would commit. The slot
   write only overwrites a file (`cat >`); it cannot append to `info/exclude`.
2. **Not removed.** Neither the agent's exit nor the delete reaches the target.
3. **Owner-only mode not applied.** The ssh write creates the file with the target's umask, and the
   pointer carries the session's hook token.

What would close it: one bounded ssh command (same deadline and keep-alives as the slot write) that
appends the exclude pattern when missing, writes the pointer `0600`, and a matching bounded removal on
exit and on delete.

Two lifecycle choices the fix kept, recorded so they are not mistaken for gaps:

- **tmux without an exit hook keeps its pointer until the delete.** An exit hook makes the tmux pane
  remain after its agent exits (`remain_on_exit`); claude and cursor declare no resume adapter, so
  they have no hook, and adding one only to remove the pointer would change their pane behaviour. The
  pointer is replaced at the next resume and removed at the delete.
- **A jailed session writes no pointer.** Its agent's shell commands run on the host through the Shell
  relay, whose environment (`JailedSession::session_env`) already carries the session; a pointer copied
  into the jail's context would name a host socket the jail cannot reach, and the in-jail MCP server
  would adopt it ahead of its sandbox IPC.
