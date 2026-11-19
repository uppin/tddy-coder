# 2026-11-19 — A target start names no host path, and a target cut has a real base

**Type:** Fix · **Packages:** `tddy-agent-launch`, `tddy-remote-shell`

`write_rendered_session_files` copies an argv file of this host's session directory into the target
worktree's `.tddy/` (owner-only) and rewrites the argument; `remote_base_ref` resolves a target cut's
default base as the local cut does, and `remote_worktree_through` no longer defaults to `HEAD`. Full
record: [`docs/dev/changesets/2026-11-19-target-start-host-paths.md`](../../../../docs/dev/changesets/2026-11-19-target-start-host-paths.md).
