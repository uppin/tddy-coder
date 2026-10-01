# 2026-10-01 — The jail relay carries a tool call's conversation id and the `ConversationWorktree` RPC

**Type:** Feature

`#agent-worktree` 1/4, PR [#560](https://github.com/uppin/tddy-coder/pull/560). Cross-package entry:
[2026-10-01-agent-worktree-isolated-edits.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-isolated-edits.md).

`HostToolHandler::execute(session_id, conversation_id, tool, args)` takes the conversation id (seven
implementations), and the jail relay's `call_tool(conversation_id, …)` and the host relay no longer
drop it; the runner forwards `ConversationWorktree` to the host. Inside the jail,
`conversation_root::tool_root` finds a conversation's worktree under the jail's own mount and
`bind_to_this_session` rewrites a bridge request to the jail's own session id (the host does not yet
check it: see `docs/dev/todo/2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md`).
The runner never runs git and has no dependency on `tddy-subagent-worktree`; it restates the
directory name as `CONVERSATION_WORKTREES_DIR`, pinned equal by a test in `tddy-daemon-sandbox`.
The git crate is nevertheless linked transitively
(`docs/dev/todo/2026-10-01-the-sandbox-runner-links-git-code-through-a-types-re-export.md`).
