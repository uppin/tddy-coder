# 2026-11-16 — One remote-shell port behind every SSH call

Every command the daemon runs on an SSH target — exec-catalog tools, worktree setup, slot
files, the session pointer, and git pack verbs — runs through one port, `RemoteShell`, with
OpenSSH as its first transport. All SSH calls share one connection option set
(`BatchMode=yes`, `ConnectTimeout=15`, `ServerAliveInterval=5`, `ServerAliveCountMax=3`) and
one typed error, so an unreachable target fails within the connect timeout and names the
cause instead of hanging or reporting a generic failure. Behaviour on a healthy host is
unchanged. See [remote-managed-worktree.md](../remote-managed-worktree.md) and
[remote-git-repo.md](../remote-git-repo.md).
