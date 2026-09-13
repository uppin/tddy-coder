# 2026-08-16 — The daemon's secret stores still truncate in place

**Category:** Future enhancement
**Source:** atomic-session-file-writes changeset, 2026-08-16
**Narrowed:** 2026-09-10 — `github_token_store.rs` is fixed; two stores remain

`tddy_core::atomic_file` carries every session- and daemon-state write, but three files were left
out. They are the ones that are **correct about mode `0600` on creation**, and `write_atomic` only
carries permission bits over from an *existing* target — so a first write through it would create
the swap file at the process umask and publish a world-readable secret store. The fix is the
mode-aware variant, `write_atomic_with_mode(path, contents, mode)`, which sets the swap file's mode
before writing rather than copying it from the target.

**Two remain:**

| Call site | Owner |
|---|---|
| `packages/tddy-daemon/src/vnc_vault.rs:153` | node 2 of the `#unbundle` stack (`tddy-screen-sharing`) |
| `packages/tddy-daemon/src/screen_sharing_vault.rs:171` | node 2 of the `#unbundle` stack (`tddy-screen-sharing`) |

Both still carry `.truncate(true)`. Until they are converted, a full disk can empty either store: an
empty secrets file reads as "no credential", which surfaces as a re-auth prompt rather than as the
write failure it is.

**Converted:** `github_token_store.rs`, by `#unbundle` node 4
([#473](https://github.com/uppin/tddy-coder/pull/473)), as part of moving it into
`tddy-daemon-auth`. Doing it there was a deliberate exception to that node's "move only" rule —
relocating verbatim would have carried a known data-loss path into the crate whose whole purpose is
to be the identity boundary, and hand-rolling an atomic write beside the existing helper would have
deepened the duplication this entry is about.

Two notes for whoever converts the remaining two:

- The unit test written for `github_token_store` **does not discriminate** the change. That base
  implementation already staged to a `.tmp` file and renamed, and the staged create also fails in a
  `0o555` directory, so reverting the refactor leaves the test green. It guards a *future*
  truncate-in-place. Check whether the vault under conversion has the same property before trusting
  a similar test.
- Switching the directory creation to `DirBuilder::recursive(true).mode(0o700)` at the same time
  changes behaviour twice — see
  [2026-09-10-ensure-owner-only-dir-no-longer-re-tightens-an-existing-auth-storage.md](./2026-09-10-ensure-owner-only-dir-no-longer-re-tightens-an-existing-auth-storage.md).
