# 2026-11-21 — Clean, Delete and Restore of a target session act on the target worktree

**Type:** Fix · **Stack:** `#target-parity` 2/7 (PR #970)

`coder_credential_source::worktree_target_shells` builds the port from the daemon's vaults (or `NoCoderAccounts`), and `runtime/mount_host.rs` installs it on the worktree service. Full record: [`docs/dev/changesets/2026-11-21-target-worktree-actions.md`](../../../../docs/dev/changesets/2026-11-21-target-worktree-actions.md).
