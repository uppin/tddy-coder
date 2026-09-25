# 2026-09-25 — `move_module_to_crate` re-points a `crate::` path through the origin's facade to the destination's own extern name

**Category:** Future enhancement (engine defect; the tree does not compile after the move)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), plan
`docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/01a-relay-idle-local-token-to-kernel.jsonl`,
op 1 (`local_token_tonic_adapter` → `tddy-daemon-kernel`)

## What happened

The moved module named a lifecycle module by `crate::`. That module is a facade over the crate the
file was moving into:

```rust
// packages/tddy-session-lifecycle/src/lib.rs
pub use tddy_daemon_kernel::config;

// packages/tddy-session-lifecycle/src/local_token_tonic_adapter.rs, before the move
use crate::config::DaemonConfig;
```

The header pass resolved `crate::config` through that re-export to the crate that **defines** it —
the destination — and wrote the destination's extern name into the destination. Its manifest pass
then added the destination to its own `[dependencies]`:

```rust
// packages/tddy-daemon-kernel/src/local_token_tonic_adapter.rs, as the engine left it
use tddy_daemon_kernel::config::DaemonConfig;
```

```toml
# appended to packages/tddy-daemon-kernel/Cargo.toml
tddy-daemon-kernel = { path = "" }
```

```text
error: cyclic package dependency: package `tddy-daemon-kernel v0.1.0 (…/packages/tddy-daemon-kernel)` depends on itself.
```

`apply` reported `2 of 2 operation(s) were applied, and the tree no longer compiles`.

## How this differs from the extern-name cause already recorded

[`2026-09-25-restructure-move-to-crate-leaves-the-destinations-own-extern-name.md`](./2026-09-25-restructure-move-to-crate-leaves-the-destinations-own-extern-name.md)
is a moved file that **already** wrote `tddy_daemon_sandbox::…` before the move, and the engine
left it alone. Here the moved file never named the destination. The engine **produced** the
destination's extern name itself, from a `crate::` path, by following the origin's re-export to its
defining crate and not noticing that crate is the one being moved into.

The fix proposed there (rewrite a destination-extern-name path to `crate::`) would repair both,
provided it runs **after** the re-export resolution, not only over the file's original text.

## What was fixed by hand

```rust
// packages/tddy-daemon-kernel/src/local_token_tonic_adapter.rs
use crate::config::DaemonConfig;
```

and the `tddy-daemon-kernel = { path = "" }` line removed from its own manifest.

## Also seen on this plan (cause already recorded)

Ops 0 and 1 each replaced their `pub mod` line in lifecycle's `lib.rs` with an identical
`pub use tddy_daemon_kernel::*;`
([`2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md`](./2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md)).
The second was deleted by hand, since rustc reports it as an unused import and `clippy -D warnings`
fails on it.
