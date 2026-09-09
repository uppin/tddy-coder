# Keeping `target/` from overhogging the disk

**Date:** 2026-09-09
**Why:** during the `connection_service.rs` split this machine hit **1.5 GiB free / 100% full**
three times, and `rm -rf target` was used each time. That reclaimed 28–38 GiB but cost a full cold
rebuild *and* a full cold rust-analyzer index (~7–10 minutes each) every single time. The cycle was
the problem, not the disk.

## What actually consumes it — measured, not assumed

The root `Cargo.toml` is **already well tuned**: `[profile.dev]` and `[profile.test]` both set
`debug = "line-tables-only"`, with a comment naming the per-crate test binaries as the biggest
`target/` contributor. So debug symbols are not the lever; there is nothing to win there.

Measured mid-restructure, with only `cargo check` and rust-analyzer having run:

| Path | Size | Nature |
|---|---:|---|
| `target/debug/incremental` | **3.1 G** | **pure cache — disposable, costs nothing to lose** |
| `target/debug/deps` | 1.9 G | real artefacts; deleting forces a rebuild |
| `target/debug/build` | 555 M | build-script output; `./clean` prunes stale copies |
| **total** | **5.5 G** | |

And the peak that caused the crisis:

| What ran | `target/` after |
|---|---:|
| `cargo check -p tddy-daemon --all-targets` + rust-analyzer | ~5 G |
| `cargo test -p tddy-daemon --lib` | ~8 G |
| **`cargo test -p tddy-daemon` (all ~80 integration binaries)** | **~37 G** |

**One command accounts for the whole problem**: building the ~80 integration test binaries in
`packages/tddy-daemon/tests/`. Each links the whole crate graph, and there are eighty of them.

## The discipline

1. **Verify with `--lib`, not the full suite.** `cargo test -p tddy-daemon --lib` is ~8 G and covers
   every moved test module, because all 29 of them are `#[cfg(test)] mod` inside the lib. The
   integration binaries only need to *compile*, which `cargo clippy --all-targets` already proves
   for a fraction of the space. Run the full suite once, at the end, or leave it to CI.
2. **Delete `target/debug/incremental`, not `target/`.** 3 G back, no rebuild cost, no re-index. This
   is the step that was missing, and it is why the `rm -rf target` cycle kept repeating.
3. **`CARGO_INCREMENTAL=0` for restructure runs.** rust-analyzer's `cargo check` does not benefit
   from the incremental cache, so the 3 G is pure waste during a restructure. Nothing in the repo
   sets it today.
4. **`./clean` before reaching for anything destructive.** It keeps the newest artefact per crate in
   `build`, `deps` and `incremental` and drops the rest — written for exactly this, and it does not
   cost a cold index.
5. **Never a second `CARGO_TARGET_DIR`.** Pointing verification at a separate target directory to
   dodge a cargo lock duplicated 4.5 G of dependency builds. Wait for the lock instead.

Escalation order, cheapest first: `rm -rf target/debug/incremental` → `./clean` →
`rm -rf target/debug/deps` → `rm -rf target`. Only the last one costs a cold index, and it should be
the rarest rather than the reflex.

## The one-line fix worth landing

`.cargo/config.toml` does not exist in this repo. Adding it with:

```toml
[build]
incremental = false
```

would remove 3 G of cache the restructure workflow never reads. The trade is slower incremental
*rebuilds* during ordinary editing, so it belongs behind an env var for restructure sessions rather
than as a repo-wide default — `CARGO_INCREMENTAL=0 ./dev …` gets it per invocation with no cost to
anyone else.
