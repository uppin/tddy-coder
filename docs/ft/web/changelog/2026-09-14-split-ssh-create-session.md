# 2026-09-14 — Split create-session SSH destinations

When agent and codebase daemons differ, the SSH host picker lists OpenSSH aliases from the codebase
host and forwards the chosen alias on the workspace start; the agent host does not open SSH.
Co-located behaviour is unchanged. Product detail:
[remote-managed-worktree.md](../../daemon/remote-managed-worktree.md) § Split SSH.
