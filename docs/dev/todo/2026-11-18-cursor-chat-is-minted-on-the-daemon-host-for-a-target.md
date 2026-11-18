# 2026-11-18 — A Cursor session on a target mints its chat on the daemon's host, in the target's worktree path

**Category:** Known defect
**Source:** the Coder `remote-dev` smoke test of 2026-11-18 (orchestrator session `01a1ecee-e942-7982-abfc-104c6c4baa98`, diagnostics session `01a1ecfc-0fac-77a0-bb0d-ac20a90593ae`), left open by the fix that made Coder one-shots cross the pty intact ([`2026-11-18-coder-one-shots-cross-the-pty-intact`](../changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md))

Every `cursor-cli` start on a Coder target fails with
`cursor could not mint its conversation with cursor-create-chat: `<mac path>/cursor-agent create-chat` could not be run: No such file or directory (os error 2)`.

`start_conversation` (`tddy-agent-launch/src/cli_agent_steps/conversation.rs`) hands
`CursorCreateChat::mint` (`tddy-agent-backend/src/adapters/conversation_id.rs`) the binary
resolved **on this host** and the session's worktree **on the target**
(`/home/coder/.tddy/repos/<repo>/.worktrees/<id>`) as its `current_dir`, even when the start has a
`target_shell`. The local spawn fails its `chdir` and reports it as the binary not running. Even with
a valid directory it would mint the chat in this host's Cursor store, not the target's. On the
workspace itself, `cursor-agent create-chat` works (exit 0 in 1.2 s, logged in).

**Why deferred:** the fix needs a decision shared with
[2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md](2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md):
which binary runs on a target, and with what `PATH` and `HOME`.

**What would close it:** for a start with a target shell, run the mint through that shell in the
target's worktree, with the binary as the target resolves it. Refuse a local spawn whose working
directory is a target path, and make the error name the directory when `chdir` is what failed.
