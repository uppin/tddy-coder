# 2026-11-16 — A session's agent runs on its SSH host

A session can name a target — an SSH host with no tddy software on it — and its agent runs
there: the worktree is created on the target, the agent starts in an interactive remote shell
with its spec's environment, and the session's terminal, stop, exit status and resume work as
for a local session. The session pointer on the target names the remote agent as its owner.
A target combined with a managed codebase, a sandbox, a split placement or the project-home
main is refused with the reason. See [claude-cli-session.md](../claude-cli-session.md) and
[remote-managed-worktree.md](../remote-managed-worktree.md).
