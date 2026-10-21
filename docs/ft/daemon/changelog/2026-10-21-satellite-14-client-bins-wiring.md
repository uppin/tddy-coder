# 2026-10-21 — The git shim and the worktree mirror are built by wiring crates

`#satellite` 14/15. `tddy-remote-git-repo` and `tddy-session-sync` behave as before: same binary
names and paths, flags, environment variables, exit codes and `--help` text. The libraries no longer
depend on LiveKit; each binary is built by its own wiring package (`tddy-remote-git-repo-cli`,
`tddy-session-sync-cli`) that binds LiveKit as the room transport. Building a binary by package name
uses the `-cli` package. See [Remote git repo](../remote-git-repo.md#shipping).
