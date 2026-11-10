# Move the Linux-only spawn_broker ranges (namespaces, resolve) on a Linux target

**Detected:** 2026-11-10, `#agent-tools` sweep (#876, 6e) — plans
`09-spawn-broker-namespaces` and `10-spawn-broker-resolve` in
`docs/dev/1-WIP/2026-11-10-agent-tools-sweep-plans/`.

**Deferred by:** developer consent, 2026-11-10 — 09/10 deferred by the user, 2026-11-10 (#876 6e): Linux-only ranges the engine cannot move on macOS; follow-up on a Linux target. `tddy-tools restructure check --deep` refuses both groups on this macOS host: the ranges are mostly `#[cfg(target_os = "linux")]`, and rust-analyzer on Darwin does not analyse cfg'd-out code, so their names do not resolve. Plans 11 and 12 (with their `-b` files) were applied as planned in #876; `spawn_broker.rs` is 984 → 534 production lines, still over the 500 budget by the two deferred ranges (~420 lines).

**Status:** Open — partially fixed by #876 (plans 11/12); the namespaces and resolve ranges remain in `spawn_broker.rs`.

**What would close it:** on a Linux host (or with rust-analyzer's `cargo.target` set to a Linux triple), re-run `restructure check --deep` for plans 09 and 10, apply them (re-snapshotting the v1 plan after each), and verify every file in `spawn_broker/` is ≤ 500 production lines. If the engine still refuses there, stop and ask — never hand-move or hand-merge the cfg pairs.
