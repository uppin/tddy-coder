# 2026-08-13 — `session_deletion` leaks the worktree for every session type except claude-cli

**Category:** Future enhancement
**Source:** remote-managed-worktree changeset, 2026-08-13

`packages/tddy-daemon/src/session_deletion.rs:166-169` gates worktree removal on
`session_type == "claude-cli"`:

```rust
let claude_cli_worktree = metadata
    .as_ref()
    .filter(|m| m.session_type.as_deref() == Some("claude-cli"))
    .and_then(|m| m.repo_path.clone());
```

So deleting a `cursor-cli` or `workspace` session removes the session directory but leaves both the
directory and the `git worktree` registration behind. The remote-managed-worktree changeset widens this
to include `"workspace"` because split sessions would otherwise leak a worktree on the codebase host on
every delete. **`cursor-cli` is deliberately left leaking** — fixing it changes behaviour for sessions
that changeset does not touch, and deserves its own change with its own tests.

Note `docs/ft/daemon/remote-codebase-mode.md` criterion 3 asserted that `DeleteSession` for a workspace
session "removes the session directory and the worktree". It did not. That line is corrected by the same
changeset; the rest of that document's criteria are worth re-verifying against the code rather than
trusted, since at least one was aspirational.
