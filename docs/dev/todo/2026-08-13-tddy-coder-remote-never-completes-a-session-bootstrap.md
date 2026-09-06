# 2026-08-13 — `tddy-coder --remote` never completes a session bootstrap

**Category:** Future enhancement
**Source:** remote-managed-worktree changeset, 2026-08-13

`packages/tddy-coder/src/run.rs:4000-4003` contacts the relay daemon successfully and then bails:

```rust
// TODO: implement full session bootstrap (start-session → connect-session → run_goal)
anyhow::bail!("remote mode: successfully contacted relay at {} but full session bootstrap is not yet implemented", daemon_url)
```

`RemoteContextDir` (`packages/tddy-coder/src/remote.rs:27-59`) is referenced only from tests. So the
CLI entry point for remote-codebase mode has never worked end to end, despite
`docs/ft/daemon/remote-codebase-mode.md` criteria 23–28 describing it as shipped. The
remote-managed-worktree changeset delivers the daemon/UI path instead and leaves this alone; either
implement the bootstrap or retire the flag, but the current state advertises a capability that does not
exist.
