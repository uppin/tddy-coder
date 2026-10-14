# 2026-10-04 — a cross-crate move does not copy a crate the origin declares only under a target table (`libc`)

**Category:** Restructure engine defect
**Source:** `#live-plan` 12/15 (#571), moving `tddy-daemon`'s `index_daemon` cluster to `tddy-lsp-executor`. The entry first recorded four findings; the directory children, the dry-run count and the missing static `check` report are closed, and this is the one that remains.

## What happens

The moved `index_daemon_body.rs` names `libc`, which the origin declares under
`[target.'cfg(unix)'.dependencies]` (`packages/tddy-daemon/Cargo.toml`). The path survey drops
`libc::…` as "not a crate" because the origin's dependency lookup reads `[dependencies]` and
`[dev-dependencies]` only, so the destination's manifest gains no `libc` and the compile gate reports
`E0433`. The same shape is in `packages/tddy-session-lifecycle/Cargo.toml`.

Hand fix so far: the destination's `Cargo.toml` gets the `libc` line, with its target table, by hand.

## Reproduce

In a two-crate workspace, declare `libc` under `[target.'cfg(unix)'.dependencies]` of crate `origin`, name
`libc::getpid()` in a body of `origin/src/a.rs`, and move `a` to `destination` with `move_module_to_crate`.
`check --deep` and `apply --dry-run` pass; the compile gate fails with `E0433` for `libc`.

## What the engine should do

Read the target tables of the origin's manifest as a source of dependencies, and write the destination line
under the same table. The destination's `[dependencies]` gains what the moved code names, from any table the
origin declares it in.

## Why deferred

Manifest reading is the `new-crate` node's work (`feature/reshape/new-crate`, `#reshape` 8/19), which also
owns creating a destination crate and the crate named only by an attribute macro. `#reshape` 5 only feeds the
crates named by carried children into the same manifest pass. Delete this entry when that node lands.
