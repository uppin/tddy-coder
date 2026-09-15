# Changeset: carve-restructure-moves

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Bug Fix (tooling capability)
**Stack**: `#carve` 1/9 — the stack root

PRD: [`2026-09-15-carve-restructure-moves-prd.md`](./2026-09-15-carve-restructure-moves-prd.md)

## Initial Discovery

[`2026-09-15-carve-restructure-moves-initial-discovery.md`](./2026-09-15-carve-restructure-moves-initial-discovery.md)

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md) —
  `move_module_to_crate` accepts nested anchors; the dependency-cycle refusal resolves re-exports;
  `check` runs `apply`'s preconditions.
- **`docs/ft/coder`**: [rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) —
  § Known limitations loses "Only `<crate>/src/<module>.rs` moves".

## Responsibility

- `move_module_to_crate` accepts an anchor at `<crate>/src/<parent>/<module>.rs` and
  `<crate>/src/<parent>/mod.rs`, taking the destination module path from the anchor's own `path` and
  **locating** the parent's `mod` declaration by walking `<crate>/src/<parent>.rs` then
  `<crate>/src/<parent>/mod.rs`.
- `refuse_a_dependency_cycle` attributes an origin-named path to the crate that **defines** the item,
  so a back-compat `pub use` facade — and a path resolving into the destination — stop reading as
  cycles. A genuine origin-defined dependency is still refused, with the existing message.
- `restructure check` runs the same preconditions `apply` does, per operation.
- The stale `--indexing-budget` backlog item is **verified and closed**, not re-implemented.

## Boundaries

- Does **not** implement multi-module cluster moves. A set of mutually-referencing modules still
  cannot move as one unit — that is `#carve` 3/9 (`restructure-clusters`).
- Does **not** make the journal plan-scoped. `.restructure/` still needs archiving by hand between a
  Phase A and a Phase C plan — also `#carve` 3/9.
- Does **not** carve any crate. No `tddy-core`, `tddy-session-lifecycle` or `tddy-workflow-recipes`
  file moves in this PR.
- Does **not** fix the cosmetic defects recorded beside these (one `pub use <crate>::*;` per
  operation rather than per destination; `pub mod` lines appended out of order).
- Does **not** touch TypeScript operations.

## Dependencies

None — this is the stack root, based on `master`.

## Draft PR contract

Published first, so the six dependent nodes can compile against a real signature while the
implementation continues in this same PR:

1. `source_crate_of` replaced by a parent-locating resolver with its real signature, returning the
   source crate **and** the parent module file it found. Unimplemented body marked
   `// TODO(restructure-moves): implement`.
2. `refuse_a_dependency_cycle` gains the defining-crate resolution parameter it needs.
3. Failing tests pinning AC1–AC7, against those signatures.

This PR goes on to implement all of it. **It must not merge in that state.**

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — every test is a unit test over the operation's own preconditions,
plus one live-workspace apply that needs no other `#carve` node
**Concurrent with:** `recipe-parsers` (`#carve` 2/9)
**Blocks:** `restructure-clusters` (3/9), `core-foundations` (4/9), `git-plumbing` (5/9),
`session-store` (6/9), `pr-stack-crate` (9/9) directly; `telegram` (7/9) and `presenter-split` (8/9)
transitively

Real dependency edges, as opposed to the branch line:

    n1 → n3, n4, n5, n6, n9      n3 → n6, n7, n9      n4 → n8, n9      n5 → n9

## Prerequisites

| Entry | Verdict | What this node does with it |
|---|---|---|
| [2026-09-09-restructure-defects-from-the-first-cross-crate-move.md](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md) | ⚠ **DURING** | Fixes two of the recorded defects — the nested-module refusal and the facade-cycle refusal — and **edits the entry down** to what remains (repo-scoped journal, the two cosmetic items). **Does not claim it**: `#carve` 3/9 fixes the journal and claims the file. |
| [2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md](../todo/2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md) | ⚠ **DURING** | Closes its part 2 (`--indexing-budget` not honoured) by **verifying it is already fixed** and editing that section out. Part 1 (entangled cluster) is `#carve` 3/9's, which claims the file. |
| [2026-09-09-restructure-defects-from-the-connection-service-split.md](../todo/2026-09-09-restructure-defects-from-the-connection-service-split.md) | ℹ **Answered** | D6–D9 are recorded as fixed, which is why `extract_module` is trusted as Phase A across this stack. No work here. |
| [2026-09-09-macro-expansion-as-a-restructure-operation.md](../todo/2026-09-09-macro-expansion-as-a-restructure-operation.md) | — Unrelated | Different operation; no `#carve` node needs it. |

**Neither backlog file is deleted by this PR.** Both are claimed ✅ RESOLVED HERE by `#carve` 3/9,
which fixes their remaining halves — one claiming node per entry, and the lowest node that fixes an
entry *completely* is 3/9, not this one.

## State A → State B

### State A

- `crate_move.rs:773` — `source_crate_of` does `strip_suffix("/src/{module}.rs")`. A nested anchor is
  refused before rust-analyzer spawns. Measured: **0 of 13** `model_registry/` modules moved.
- `crate_move.rs:573` — `refuse_a_dependency_cycle` compares `header.crates_named` against
  `keeps_naming_it` by **extern name**, with no notion of where an item is defined. A `pub use`
  re-export in the origin therefore reads as an origin dependency.
- `restructure check` does not run either precondition; both plans that failed `apply` passed `check`
  with `no findings`.
- `--indexing-budget` **is** honoured in the current tree (`restructure_cli.rs:215`,
  `backends/rust.rs:476`, `backends/lsp_bridge.rs`). The backlog entry saying otherwise predates the
  fix.

### State B

- A directory-shaped module moves, and the refusal survives only for a parent that exists nowhere.
- A back-compat facade no longer manufactures a cycle; a genuine origin dependency still does.
- `check` and `apply` refuse the same plans.
- The backlog carries no stale `--indexing-budget` claim.

## Implementation phases

Per the stack's standing requirement, mechanical moves are planned as restructure intents first and
hand-written code is the residue. This node is the exception that proves it: it **is** the tooling,
so it is almost entirely hand-written — and it is what makes phases A and C mechanical everywhere else.

| Phase | Kind | Work |
|---|---|---|
| A | mechanical | `extract_module --to_file` lifting the anchor-resolution helpers out of `crate_move.rs` (1,500+ lines) into `crate_move/anchor.rs`, so the new resolver has somewhere to live that is not the bottom of a long file |
| B | manual | The parent-locating resolver, the defining-crate attribution, and the `check` precondition pass |
| C | mechanical | `extract_module --to_file` for the refusal helpers, once their new shape is settled |
| D | manual | Doc updates: `README.md`, `docs/ft/coder/rust-code-restructuring.md` § Known limitations, and the two backlog entries edited down |

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [ ] Publish the draft-PR contract (owned API surface + failing tests)
- [ ] Failing acceptance tests — **USER REVIEW**
- [ ] Failing unit/integration tests
- [ ] Implement production code making tests pass (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

Scoped to the package this node touches:

```bash
./test -p tddy-code-restructuring
cargo clippy -p tddy-code-restructuring -- -D warnings
cargo fmt --all --check
```

Plus one **live** apply against this workspace for AC1/AC2/AC8 — the refusals are unit-tested only
today, and the backlog records that as the gap that let both defects reach a real stack.
