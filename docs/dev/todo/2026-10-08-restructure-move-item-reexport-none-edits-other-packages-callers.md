# 2026-10-08 — `move_item` with `reexport: none` re-points callers in other packages, which a node that keeps consumers unedited must revert by hand

**Category:** Engine behaviour and a manual fix
**Source:** #carve 21/21 (PR #536), R5b step one: `move_item` of `DemoVmServiceImpl` and its two impl blocks into a new lifecycle module `demo_vm_service`. Documented in
`packages/tddy-code-restructuring/docs/same-crate-moves.md`: "`none` or absent **re-points every caller** found by the server" — across packages too.

## What happened

The engine rewrote `tddy-daemon/src/runtime.rs` (the `use` and the call) and `tddy-daemon/tests/local_token_uds.rs` to
`tddy_session_lifecycle::connection_service::demo_vm_service::DemoVmServiceImpl`. The node's rule is that consumers keep their public paths through facades.

## Hand fix

Reverted the two path edits with `sed` (`connection_service::demo_vm_service::DemoVmServiceImpl` → `connection_service::DemoVmServiceImpl`); lifecycle's facade
`pub use …::DemoVmServiceImpl` keeps that path resolving. (`reexport: outside` would have left outside callers alone and re-pointed only the library's own; it was not used because
nothing inside the crate needed re-pointing and the old facade line was meant to be rewritten, not kept.)

## What the engine should do

Nothing is wrong, strictly; but a plan that wants "re-point the crate's own callers, keep a facade for everyone else" is `outside`, and the changeset's recipe should say to use it. The next plan of this kind should use `reexport: outside`.
