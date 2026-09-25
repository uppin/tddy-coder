# 2026-09-25 — `move_module_to_crate` leaves a path that names the destination by its extern name, and makes the destination depend on itself

**Category:** Future enhancement (engine defect; the tree does not compile after the move)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), plan
`docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/02b-task-action-services-to-daemon-sandbox.jsonl`,
op 1 (`action_service` → `tddy-daemon-sandbox`)

## What happened

`tddy-session-lifecycle/src/action_service.rs` already depended on the crate it was moving into:

```rust
// before, in tddy-session-lifecycle
use tddy_daemon_sandbox::sandbox_runtime::{attach_sandbox_request, SandboxRuntime};
```

After the move the file lives *in* `tddy-daemon-sandbox`. The engine left the path as it was, and
its manifest pass then added the crate the path names — the destination itself — to the
destination's own `[dependencies]`:

```toml
# what the engine wrote into packages/tddy-daemon-sandbox/Cargo.toml
tddy-daemon-sandbox = { path = "" }
```

```text
error: cyclic package dependency: package `tddy-daemon-sandbox v0.1.0 (…/packages/tddy-daemon-sandbox)` depends on itself.
```

`apply` reported `4 of 4 operation(s) were applied, and the tree no longer compiles`, so the compile
gate caught it.

## What was fixed by hand

```rust
// packages/tddy-daemon-sandbox/src/action_service.rs
use crate::sandbox_runtime::{attach_sandbox_request, SandboxRuntime};
```

and the `tddy-daemon-sandbox = { path = "" }` line removed from its own manifest.

## How this differs from the self-dependency already recorded

[`2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`](./2026-09-09-restructure-defects-from-the-first-cross-crate-move.md)
records the same symptom for a different cause: a moved module naming a **sibling moved to the same
crate earlier in the plan**. Here nothing else in the plan was involved. The moved module named a
module the destination has **always** defined, through the destination's extern name.

## What would fix it

In the moved file's header pass, a path whose first segment is the **destination's** extern name
becomes `crate::…`. Bodies need the same rewrite as headers, since `tddy_daemon_sandbox::x::y(…)` is
just as valid in a body. The manifest pass must never add the destination to its own manifest; it
should be an assertion, not a filter.
