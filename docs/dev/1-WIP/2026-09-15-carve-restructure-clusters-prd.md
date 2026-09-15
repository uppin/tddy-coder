# PRD — a mutually-referencing cluster of modules moves as one unit

**Date:** 2026-09-15
**Stack:** `#carve` 3/9
**Package:** `packages/tddy-code-restructuring`
**Product area:** [`docs/ft/coder/rust-code-restructuring.md`](../../ft/coder/rust-code-restructuring.md)

## Problem

`move_module_to_crate` moves **one module**. Its vocabulary has no way to say "these four modules
move together", and the subsystems worth extracting are almost never a single module.

`#unbundle` node 3 hit this extracting `spawner.rs`, `spawn_worker.rs`, `supervisor_spawn.rs` and
`supervisor_client.rs` from `tddy-daemon` into `tddy-spawn`. The plan said every move was "a plan the
operation executes". It could not be, and all four were hand-moved. Two mechanics defeat it:

**1. The header pass re-points `crate::` at the source crate.** `repointed_header(text, origin_extern)`
(`crate_move.rs:647`) rewrites every `crate::` path in the moved file to the **origin's** extern name.
So `spawner.rs`'s `use crate::config::DaemonConfig` becomes `tddy_daemon::config::DaemonConfig` — a
`tddy-spawn → tddy-daemon` edge, while `tddy-daemon` already depends on `tddy-spawn`. The operation
authors a dependency cycle, and it has no way to know `config` is itself a re-export whose correct
rewrite points at `tddy-daemon-kernel`.

**2. Moving one module rewrites its not-yet-moved siblings.** Moving `spawner` first turns the other
three modules' `crate::spawner` into `tddy_spawn::spawner` — correct only once those three are
themselves in `tddy-spawn`. **Between the first op and the last, the tree does not compile**, so there
is no intermediate state to verify against.

A third defect makes every multi-plan node in this stack more expensive than it should be:

**3. The journal is repo-scoped, not plan-scoped.** `StatePaths::under(root)` (`runner.rs:804`) builds
`<root>/.restructure/{journal.jsonl,ledger.json}` — one per repository. `open_run` (`runner.rs:779`)
refuses when a non-empty journal exists without `--resume`, so a **completed** plan blocks the next
one, and `--resume` would resume the wrong plan. The backlog calls this *"the single largest tax on a
multi-layer move"*. Every `#carve` node is multi-plan by construction — Phase A carves, Phase C
moves — so every one of them pays it.

## What this PR delivers

### FR1 — one operation, a set of modules

`move_module_to_crate` accepts a set of modules that move together as a single unit, resolved and
applied atomically. Either the whole cluster moves or none of it does; there is no intermediate tree.

### FR2 — a co-moving sibling is destination-local

The header pass is told the co-moving set. A `crate::` path reaching a module **that is in the set**
is re-pointed at the **destination**, not the origin — because by the time the edit lands, that module
is there. A path reaching a module staying behind is re-pointed at the origin as today.

This also removes the cycle the operation used to author: a sibling reference is no longer an
origin dependency, so `refuse_a_dependency_cycle` (as `#carve` 1/9 leaves it) sees the truth.

### FR3 — state is plan-scoped

`.restructure/` state is keyed by the plan, so a completed plan does not block the next one and
`--resume` resumes the plan it was given. A pre-existing repo-scoped journal is migrated or refused
with a message that says which, never silently adopted by a different plan.

### FR4 — `check` reports a cluster it cannot move

Building on `#carve` 1/9's `check`/`apply` precondition parity: a plan that moves a module whose
siblings reference it, without naming them in the set, is reported by `check` — statically, before
any apply. This is the defect's worst half, and the backlog says so: *"a plan that passes `check`
reads as safe"*.

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | A set of N mutually-referencing modules moves in one operation; the tree compiles before and after, never between |
| AC2 | A `crate::<sibling>` path where `<sibling>` is in the moving set is re-pointed at the **destination** |
| AC3 | A `crate::<other>` path where `<other>` stays behind is re-pointed at the **origin**, unchanged from today |
| AC4 | The operation does not add the destination as a dependency of itself |
| AC5 | Two plans applied in sequence need no hand-archiving of `.restructure/`; the second is not refused |
| AC6 | `--resume` resumes the plan it was given, and refuses a journal belonging to a different plan |
| AC7 | `check` reports "module X's siblings reference it and are not in the set" without spawning rust-analyzer |
| AC8 | A single-module move behaves exactly as it does today — pinned by the existing tests |

## Out of scope

- The two cosmetic defects recorded alongside: one `pub use <crate>::*;` appended **per operation**
  rather than per destination, and `pub mod` lines appended out of order. Recorded, not fixed.
- Deciding *which* clusters `#carve`'s later nodes move. That is each node's own plan.
- Any carving. This PR changes the tool.

## Why this is wave 2 and not wave 1

FR2 and FR4 build directly on the defining-crate attribution `#carve` 1/9 puts into
`refuse_a_dependency_cycle`, and FR4 builds on the `check`/`apply` parity 1/9 establishes. Doing
either first would mean writing them twice.

## What it unblocks

| Node | Cluster it needs to move as a unit |
|---|---|
| `#carve` 6/9 `session-store` | `atomic_file`, `error`, `output`, `session_actions`, `session_catalog` — `session_actions → atomic_file, output`; `output → atomic_file, error`; `session_catalog → session_actions` |
| `#carve` 7/9 `telegram` | `telegram_bot → telegram_notifier → telegram_session_control`, plus `telegram_multi_select_shortcuts` and `telegram_session_subscriber` |
| `#carve` 9/9 `pr-stack-crate` | `pr_stack/` with `orchestrate_pr_stack/{git_ops,assess,pr_insight,actions}` |
