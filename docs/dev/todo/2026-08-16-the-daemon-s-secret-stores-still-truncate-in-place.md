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
| ~~`packages/tddy-daemon/src/screen_sharing_vault.rs`~~ | **Done** on node 10 (#482) — `write_atomic_with_mode` |
| ~~`packages/tddy-daemon/src/vnc_vault.rs`~~ | Removed with dead VNC service (see `2026-09-10-vnc-proto-has-no-server-and-a-live-web-client.md`) |

**Status:** Closed on node 10 for the remaining live vault (`tddy-screen-sharing`).
