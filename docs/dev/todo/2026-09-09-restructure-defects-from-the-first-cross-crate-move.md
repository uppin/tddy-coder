# 2026-09-09 — Restructure defects found by the first real cross-crate move

**Category:** Future enhancement
**Source:** `#unbundle` node 1, [#470](https://github.com/uppin/tddy-coder/pull/470) — applying
`move_module_to_crate` to 21 `tddy-daemon` modules on its first live use

The operation moved **13 of the 21**; the other 8 were hand-moved. What stopped it, and what made the
13 more expensive than they should have been. The user-visible limitations are documented in
[`docs/ft/coder/rust-code-restructuring.md` § Known limitations](../../ft/coder/rust-code-restructuring.md#known-limitations);
what follows is the operational debt behind them.

- **The journal is repo-scoped, not plan-scoped** (`.restructure/journal.jsonl`). A *completed* plan
  blocks the next one with *"a journal already exists for this plan — pass `--resume`"*, and
  `--resume` would resume the wrong plan. Every layer of a hand-layered plan needed the journal
  archived by hand first. This is the single largest tax on a multi-layer move.
- **The operation adds the destination as a dependency of itself** when the moved module names a
  sibling that has already moved to the same crate: `tddy-host-service` and `tddy-worktree-service`
  each came out of layer 2 with `tddy-<self> = { path = "" }` in their own manifest, which cargo
  rejects as a cyclic package dependency.
- **A `pub use` facade in the origin makes the dependency-cycle refusal fire spuriously.** After
  `config` moved to `tddy-daemon-kernel`, `tddy-daemon` kept `pub use tddy_daemon_kernel::config;`,
  so rust-analyzer canonicalises `crate::config::DaemonConfig` as `tddy_daemon::config::DaemonConfig`
  and the refusal reads it as an origin dependency the destination would depend back on. Eight moving
  modules had to be re-pointed at `tddy_daemon_kernel::config::` by hand first.
- **Cosmetic**: the operation appends one `pub use <crate>::*;` to the origin's `lib.rs` **per
  operation** rather than one per destination — ten identical lines after a ten-op plan — and appends
  `pub mod` lines after whatever the destination's `lib.rs` already said, rather than in order.
- **The refusals are unit-tested only** (cycle, undeclared module, nested module). Proving a refusal
  against a live server costs a rust-analyzer spawn for a decision made before the server is ever
  consulted, which is why it was not done; it does mean no test asserts the refusal survives a real
  workspace.
