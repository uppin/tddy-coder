# 2026-11-16 — A target's checkout under its user's home, cut and removed like a local one

**Type:** Fix · `#remote-shell` 10/11, PR [#926](https://github.com/uppin/tddy-coder/pull/926)

`remote_checkout.rs` resolves a target's checkout as `$HOME/.tddy/repos/<repository>/.worktrees/<session>` on the target through its shell, cuts it from the requested branch and base ref, and reports failures as `RemoteCheckoutError` carrying the target's words. `ssh_worktree.rs` and the old default remote repo root helper are deleted; their replacement is `setup_remote_worktree` / `remote_checkout_root`. See the cross-package entry in `docs/dev/changesets/2026-11-16-remote-shell-remote-checkout-root.md`.
