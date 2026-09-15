# PRD — `move_module_to_crate` moves the shapes this workspace actually has

**Date:** 2026-09-15
**Stack:** `#carve` 1/9 — the stack root
**Package:** `packages/tddy-code-restructuring`
**Product area:** [`docs/ft/coder/rust-code-restructuring.md`](../../ft/coder/rust-code-restructuring.md)

## Problem

`move_module_to_crate` is the only cross-crate operation the restructuring tool has, and it is the
operation the `#carve` stack depends on: seven of its nine nodes move code between crates, and the
user's standing requirement is that mechanical moves are planned as `tddy-tools restructure` intents
with hand-written code as the residue.

Measured across the previous stack, it moved **13 of 21** modules on `#unbundle` node 1 and **0 of
~24** on node 2. Every module it refused was hand-moved with `git mv`. Two refusals account for
nearly all of that, and both are still in the tree:

**1. A directory-shaped module is refused before rust-analyzer is ever spawned.**
`source_crate_of` (`crate_move.rs:773`) does `strip_suffix("/src/{module}.rs")`, so only a module the
crate root itself declares can move:

```
Error: plan is malformed: `packages/tddy-daemon/src/model_registry/error.rs` is not
`<crate>/src/error.rs` — `move_module_to_crate` moves a module the crate root itself declares
```

This is documented as a known limitation, but directory-shaped **is the normal shape of a subsystem
worth extracting**. `model_registry/` was chosen as `#unbundle` node 2's opening move precisely
because it was the cleanest extraction available — already directory-shaped, zero outbound `crate::`
edges, zero inline tests — and the operation could not touch a line of it.

**2. A back-compat `pub use` facade in the origin reads as a dependency cycle.**
`refuse_a_dependency_cycle` (`crate_move.rs:573`) compares the moved header's crate names against the
crates still naming the module. When the origin keeps `pub use tddy_daemon_kernel::config;`,
rust-analyzer canonicalises `crate::config::DaemonConfig` as `tddy_daemon::config::DaemonConfig`, and
the check reads a *re-export* as an origin dependency:

```
Error: plan is malformed: `packages/tddy-daemon/src/screen_sharing_service.rs` still names
`tddy-daemon` (tddy_daemon::config::{…}, tddy_daemon::host_desktop_targets::{…}, …), so the
destination would depend on the crate it left …
```

Four of those five paths were the previous stack's own facades. One was worse in kind: it resolved
into *the very crate the module was being moved to*. Leaving a facade behind is what every node of
`#carve` does, so this refusal fires on essentially every planned move.

**3. `restructure check` reports `no findings` on plans that `apply` then rejects outright.**
Observed twice — once on the 13-op nested plan, once on the four-op cluster plan. A static preflight
that cannot tell you the plan is dead is not doing the job `check` exists for, and it is what makes
the remaining manual fallbacks a surprise at apply time instead of a decision at plan time.

## What this PR delivers

### FR1 — a nested module moves

`move_module_to_crate` accepts an anchor at `<crate>/src/<parent>/<module>.rs` and at
`<crate>/src/<parent>/mod.rs`, not only `<crate>/src/<module>.rs`.

The destination's own module path comes from the anchor's `path`, which already carries the nesting
(`model_registry::store`). The parent's `mod` declaration is **located** rather than guessed, by
walking `<crate>/src/<parent>.rs` and then `<crate>/src/<parent>/mod.rs`. The refusal survives for
the case it was written for: neither file existing.

### FR2 — a re-export is not a dependency

`refuse_a_dependency_cycle` resolves each origin-named path to the crate that **defines** the item
before judging it. A path that the origin merely re-exports is attributed to the crate it re-exports
from, so a back-compat facade no longer manufactures a cycle. A path that resolves into the
**destination** crate is likewise not a cycle — it is the move already having happened.

The refusal keeps firing, unchanged, when the origin genuinely defines what the moved module names.

### FR3 — `check` refuses what `apply` would refuse

`restructure check` runs the same preconditions `apply` does, so a plan that cannot run says so
statically, per operation, without spawning rust-analyzer for decisions made before the server is
consulted.

### FR4 — the stale `--indexing-budget` entry is closed, not re-fixed

The backlog records `--indexing-budget` as accepted and then measured against a fixed ~46s ceiling.
**Discovery found this already fixed** in the current tree — `request_timeout` reads the budget
(`restructure_cli.rs:215`), `settle_budget_for` scales the per-operation settle wait
(`backends/rust.rs:476`), and `map_lsp_error(Timeout) → ServerCatchingUp` so the retry loop the
budget governs actually runs. This PR **verifies** that on this workspace and closes the entry. It
does not re-implement it.

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | A plan whose anchor is `<crate>/src/<parent>/<module>.rs` applies, and the moved file lands at the destination with its `mod` declaration removed from `<parent>`'s own module file |
| AC2 | A plan whose anchor is `<crate>/src/<parent>/mod.rs` applies, moving the whole directory-shaped module |
| AC3 | An anchor whose parent module file exists at neither `<crate>/src/<parent>.rs` nor `<crate>/src/<parent>/mod.rs` is still refused, naming both paths it looked for |
| AC4 | A move whose origin re-exports the named items via `pub use <other_crate>::…` is **not** refused, and the destination manifest gains a dependency on the **defining** crate, not the origin |
| AC5 | A move whose named path resolves into the destination crate is **not** refused |
| AC6 | A move whose origin genuinely defines what the moved module names is **still** refused, with the existing message |
| AC7 | `restructure check` reports the FR1 and FR2 refusals on a plan that would fail `apply`, rather than `no findings` |
| AC8 | `--indexing-budget 900` governs a real apply against this workspace; the behaviour with no flag is unchanged |

## Out of scope

- **Multi-module cluster moves** — a set of mutually-referencing modules moving as one unit. That is
  `#carve` 3/9 (`restructure-clusters`), which also takes the plan-scoped journal.
- **Any `#carve` carving node.** This PR changes the tool, not the crates being carved.
- TypeScript operations. `move_module_to_crate` is Rust-only and stays so.
- The cosmetic defects recorded alongside these (one `pub use <crate>::*;` appended per operation
  rather than per destination; `pub mod` lines appended out of order). Recorded, not fixed here.

## Why this is the stack root

Seven of the nine `#carve` nodes move code between crates, and four of them move a **directory**
(`session_actions/`, `session_catalog/`, `pr_stack/`, `orchestrate_pr_stack/`). Every node leaves a
back-compat facade. Without FR1 and FR2 the tool refuses that work and the stack degrades to
`git mv` — which is a supported fallback, but not the one the stack was asked to be built on.
