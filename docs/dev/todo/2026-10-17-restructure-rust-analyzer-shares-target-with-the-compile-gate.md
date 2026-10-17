# 2026-10-17 — rust-analyzer's build-script run holds `target/`'s lock while the compile gate waits

**Category:** Future enhancement (engine performance)
**Source:** #reshape 9/19 (`apply-robust`), M1 reproduction of the 2026-10-05 apply hang (decision F5, deferred by the developer 2026-10-17)

## What was seen

The engine starts rust-analyzer with no `cargo.targetDir`, so it shares the workspace's `target/`
with the compile gates `apply` runs (`packages/tddy-code-restructuring/src/runner/compile_gate.rs`).
When a `Cargo.toml` changes, rust-analyzer reloads and re-runs build scripts
(`cargo check --workspace --message-format=json`, a child of the rust-analyzer pid), holding
`target/debug/.cargo-lock` for 60–80 s. A gate's `cargo check --all-targets -p <pkg>` started in
that window sits at 0.0% CPU on the lock.

M1 (three attempts against a warm `tddy-index-daemon`, `tddy-code-restructuring` with seven
hand-edited files, a one-op `rename_symbol`): with a `Cargo.toml` edit in the tree the gate took
50 s and 100 s against 11–13 s uncontended — about 47 s and 60 s of lock wait (`lsof` showed the
rust-analyzer child holding the lock; spawn records `62291-7`, `62291-13`). Neither wait was
permanent: rust-analyzer finished and released the lock.

Unproven hypothesis: each rust-analyzer build-script run did 60–80 s of real work and the next
engine gate 50–59 s, which would fit the two cargos invalidating each other's fingerprints in the
shared `target/`.

## Why it was deferred

Out of #reshape 9/19's boundaries (it bounds silent waits; this is a slow, not a silent, wait). A
second target directory costs disk and a cold build the first time rust-analyzer starts on a
checkout, which is the trade-off to decide.

## What would close it

Start rust-analyzer with its own `cargo.targetDir` (e.g. `target/rust-analyzer`) in the
initialization options, and measure the gate with a `Cargo.toml` edit in the tree again: no lock
wait, and whether the fingerprint churn disappears.
