# 2026-09-10 — `ensure_owner_only_dir` no longer re-tightens an existing `auth_storage`, and now creates parents `0700`

**Category:** Future enhancement
**Source:** `#unbundle` node 4, [#473](https://github.com/uppin/tddy-coder/pull/473) — the atomic-write fix in `github_token_store`

Routing `github_token_store` through `tddy_core::atomic_file::write_atomic_with_mode` also changed
how the storage directory is created. It was `create_dir_all` followed by an unconditional
`set_permissions(0o700)`; it is now
`std::fs::DirBuilder::new().recursive(true).mode(0o700)`. That changed behaviour **twice**, and only
one half was deliberate.

**Intended.** An *existing* storage directory no longer has `0o700` re-imposed on every write, so
the daemon stops overruling an operator's deliberate `chmod`.

**The cost of the intended half, which deserves a decision rather than a silent accept.** An
`auth_storage` that is currently group- or world-readable **stays that way after upgrade**, where
before, every `put` re-tightened it. A deployment that drifted open stays open, and nothing reports
it. Options: leave it (an operator's `chmod` is their business), warn once at startup when the
directory is more permissive than `0700`, or re-tighten only on the first write after boot.

**Unintended.** The mode now applies to **every** directory the call creates, not just the leaf.
With `auth_storage = /var/lib/tddy/auth` and no `/var/lib/tddy`, that parent is created `0700` and
owned by the daemon user; previously parents took the process umask. That is stricter, so it is not
a security regression — but it can surprise anything else expecting to read `/var/lib/tddy`, and
`./install` creates and chowns that parent only on the root/systemd path.

Detail: [`auth-service.md` § Secrets at rest](../../../packages/tddy-daemon-auth/docs/auth-service.md).
