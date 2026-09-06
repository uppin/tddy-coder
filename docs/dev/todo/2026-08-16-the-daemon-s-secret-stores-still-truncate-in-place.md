# 2026-08-16 — The daemon's secret stores still truncate in place

**Category:** Future enhancement
**Source:** atomic-session-file-writes changeset, 2026-08-16

`tddy_core::atomic_file` now carries every session- and daemon-state write, but three files were left
out: `github_token_store.rs`, `vnc_vault.rs` and `screen_sharing_vault.rs`. They are the ones that are
**correct about mode `0600` on creation**, and `write_atomic` only carries permission bits over from an
*existing* target — so a first write through it would create the swap file at the process umask and
publish a world-readable secret store.

The fix is a mode-aware variant (`write_atomic_with_mode(path, contents, mode)`) that sets the swap
file's mode before writing rather than copying it from the target, then converting the three call
sites. Until then, a full disk can still empty a token store: an empty secrets file reads as "no
credential", which surfaces as a re-auth prompt rather than as the write failure it is.
