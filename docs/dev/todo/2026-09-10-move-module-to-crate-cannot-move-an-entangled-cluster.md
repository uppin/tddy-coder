# 2026-09-10 — `move_module_to_crate` cannot move a mutually entangled cluster, and ignores `--indexing-budget`

**Category:** Defect
**Source:** `#unbundle` node 3, [#472](https://github.com/uppin/tddy-coder/pull/472). The operation is
node 1's surface, shipped in [#470](https://github.com/uppin/tddy-coder/pull/470).

Node 3's M2 extracted four mutually entangled modules — `spawner.rs`, `spawn_worker.rs`,
`supervisor_spawn.rs`, `supervisor_client.rs` — from `tddy-daemon` into `tddy-spawn`. The plan says
every move in that node is "a plan the operation executes". It could not be, and the move was done by
hand with `git mv`. Two separate problems.

## 1. One-module-at-a-time cannot express an entangled cluster

`restructure check` reported **no findings** on a four-op plan, so the defect surfaces only at apply
time. Two mechanics defeat it:

- **The header pass re-points `crate::` at the _source_ crate.** `spawner.rs` holds
  `use crate::config::DaemonConfig`, which becomes `tddy_daemon::config::DaemonConfig` — a
  `tddy-spawn → tddy-daemon` edge, and `tddy-daemon` already depends on `tddy-spawn`. The operation
  authors a dependency cycle. It has no way to know that `config` is itself a re-export of
  `tddy_daemon_kernel::config` and that the correct rewrite points at the kernel.
- **Moving one module rewrites its not-yet-moved siblings.** Moving `spawner` first turns the other
  three modules' `crate::spawner` into `tddy_spawn::spawner`, which is correct only once those three
  are themselves in `tddy-spawn`. Between the first op and the last, the tree does not compile, so
  there is no intermediate state to verify against.

A cluster whose members reference each other has to move as one unit. The operation's vocabulary has
no way to say that, and `check` does not detect it — which is the worse half, because a plan that
passes `check` reads as safe.

## 2. `--indexing-budget` is not honoured

```
tddy-tools restructure apply --dry-run --indexing-budget 900
```

ran roughly 20 minutes of rust-analyzer indexing, reached `working (100%)`, then failed with
**"rust-analyzer had not finished indexing after 46s"**. The 900-second budget was accepted on the
command line and then measured against what looks like a fixed 46-second ceiling. On a workspace this
size the flag exists precisely for this case, so as it stands the operation cannot be applied to
`tddy-daemon` at all.

## Why it is recorded rather than fixed

`## Boundaries` for node 3 assigns `move_module_to_crate` to node 1 — *"implementing, extending or
fixing the operation"* is explicitly not node 3's. Both findings are reported upward here rather than
patched from a consuming node, so the fix lands with its own tests in the crate that owns it.
