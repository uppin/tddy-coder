# 2026-09-22 — The crate is created from `tddy-core`'s session storage layer

**Type:** Refactor · `#carve` 7/11, PR [#493](https://github.com/uppin/tddy-coder/pull/493)
Cross-package entry: [`docs/dev/changesets/2026-09-22-carve-session-store.md`](../../../../docs/dev/changesets/2026-09-22-carve-session-store.md)

Created from `tddy-core`'s `atomic_file.rs`, `error.rs`, `output/` and `session_actions/`, except
`session_actions/session_dir.rs`, which reads a changeset and stays behind. Every moved file is
byte-identical except for three changes. `error.rs` imports `tddy_workflow::ClarificationQuestion`
and its header comment names the storage layer. `session_actions/mod.rs` loses the `session_dir`
wiring. And `session_actions::runtime` goes from `pub(crate) mod` to `#[doc(hidden)] pub mod`,
with `block_on` and `write_channel_logs` widened from `pub(crate)`, because
`tddy_core::session_action_jobs::runner` calls it across the crate boundary. That is the only
surface change.

Workspace dependencies: exactly `tddy-workflow`, `tddy-actions` and `tddy-task`, none of which
depends on `tddy-core`. `tddy-core/tests/session_store_shape.rs` enforces the allowlist by exact
crate name, and checks that none of the three reaches `tddy-core` transitively. It also takes
`regex`, which `tddy-core` no longer needs. Log targets keep their `tddy_core::session_actions::…`
names. See [architecture.md](../architecture.md).
