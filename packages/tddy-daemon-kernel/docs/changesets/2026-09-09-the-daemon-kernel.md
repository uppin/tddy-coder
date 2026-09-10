# 2026-09-09 — The daemon kernel

**Type:** Architecture

New crate, added by the root node of the `#unbundle` stack
([#470](https://github.com/uppin/tddy-coder/pull/470)). Full story in the cross-package entry:
[2026-09-09-unbundle-host-worktree-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-host-worktree-services.md).

It exists because **`pub(crate)` does not cross a crate boundary**. Widening the shared surface buys
nothing once the consumer is a different crate, so the surface has to be a crate of its own.

Contents: `AgentActivityHub`, `now_unix_ms`, `HOST_DOCUMENT_FRAME_BYTES`, `SessionUserResolver`,
`SessionsBaseResolver`, `trim_to_option`, the `spawn_as_user` preamble, `privilege_drop`,
`user_paths`, `peer_forwarding`, `daemon_identity` — and `config.rs` whole.

**`config.rs` is the only whole-module move, and it is deliberate.** Four moving modules and every
handler in both new services take `&DaemonConfig` and read disjoint parts of it, so there is no
smaller cut: the symbol is the file. A narrow value struct per consuming crate would be authoring
rather than moving, and nodes 2–8 would each repeat it. `tddy-daemon` keeps
`pub use tddy_daemon_kernel::config;`, so no caller in it changed.

**`now_unix_ms` saturates.** It replaced three implementations with three different overflow
behaviours, one of which **truncated** — the wrong behaviour for a timestamp, because it turns a
clock far in the future into one in the past, silently and indistinguishably from correct data.
Callers needing the pre-1970 diagnostic keep their own check; `host_registry::now_unix_ms` survives
as an `i64` adapter over this one, not as a fourth copy.

**Everything else is a symbol lift, measured before it was made** — `spawn_as_user` is 179 of
`spawner.rs`'s 2,539 lines, `privilege_drop` 63 of 400, `user_paths` 34 of 210. `pty_registry.rs` was
moved and then retracted untouched, because nothing in the moving families reaches it. Every origin
module re-exports every lifted name, so no caller in `tddy-daemon` changed.

85 tests, including `tests/kernel_surface_acceptance.rs`, which resolves all five symbols from a
crate that does not depend on `tddy-daemon`.
