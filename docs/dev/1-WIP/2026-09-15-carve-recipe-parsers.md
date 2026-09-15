# Changeset: carve-recipe-parsers

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Refactor
**Stack**: `#carve` 2/9
**PR**: [#489](https://github.com/uppin/tddy-coder/pull/489)

PRD: [`2026-09-15-carve-recipe-parsers-prd.md`](./2026-09-15-carve-recipe-parsers-prd.md)

## Initial Discovery

[`2026-09-15-carve-recipe-parsers-initial-discovery.md`](./2026-09-15-carve-recipe-parsers-initial-discovery.md)

## Affected Packages

- **`tddy-workflow-recipes`**: [README.md](../../../packages/tddy-workflow-recipes/README.md) —
  `parser.rs` becomes `parser/` with one module per phase; the two TDD hooks files split by
  lifecycle half.

## Responsibility

- `src/parser.rs` (1,216 prod) → `src/parser/{planning,acceptance_tests,analyze,green,red,evaluate}.rs`,
  with `ParseError` and a facade left behind so every existing `parser::` path resolves.
- `src/tdd/hooks.rs` (1,002 prod) → `tdd/hooks/{before,after}.rs`; `TddWorkflowHooks` and
  `impl RunnerHooks` stay.
- `src/tdd_small/hooks.rs` (697 prod) → `tdd_small/hooks/{before,after}.rs`, same shape.
- Behaviour-preserving throughout, proven with `restructure verify --against HEAD`.

## Boundaries

- Does **not** move anything out of `tddy-workflow-recipes`. No new crate, no `Cargo.toml` change.
- Does **not** touch `pr_stack/` or `orchestrate_pr_stack/` — `#carve` 5/9 and 9/9 own those.
- Does **not** change any signature, any parse behaviour, or any hook's semantics.
- Does **not** add tests beyond the ones moving with their code.
- Does **not** depend on `#carve` 1/9's tooling fix — every operation here is in-crate
  `extract_module`, which already works.

## Dependencies

This node consumes **no** predecessor surface. It is based on `#carve` 1/9 because `gh stack`
models a line, not because it needs anything from it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `1/9` restructure-moves | nested-module and facade-cycle support in `move_module_to_crate` | **not consumed** — this node makes no cross-crate move | touch `tddy-code-restructuring` at all |

## Draft PR contract

Published first:

**Published** (commit 2): `tests/module_shape.rs`, four failing assertions that pin the layout this
node delivers — the six parser phases as modules, the parent keeping `ParseError` behind a facade,
the four hook halves, and no produced file over 500 production lines.

There is no new API surface to declare — every symbol already exists and keeps its path — so what
lands first is the **shape**, asserted against the tree rather than the type system. A module that
exists but is never named would satisfy a compile-time check; what this node promises is a layout a
reader can navigate.

The restructure plans themselves are transient working artifacts and live in `tmp/` (gitignored),
not in the repository. Their intents are recorded below.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — no test here touches any other node's surface, and the node makes
no cross-crate move, so `#carve` 1/9's fix is not on its path
**Concurrent with:** `#carve` 1/9 `restructure-moves`
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    n1 → n3, n4, n5, n6, n9      n3 → n6, n7, n9      n4 → n8, n9      n5 → n9

This node appears in none of them. It sits at position 2 because wave-1 membership puts it there and
blockers lead the wave — not because anything waits on it.

## Prerequisites

The Step 2b scan found nothing in this node's path. `parser.rs` and the two hooks files carry no
backlog entry.

## State A → State B

### State A

- `src/parser.rs` — 1,216 production lines, six phase parsers, one file. The only symbol crossing
  every seam is `ParseError`. Nine external `tddy_workflow_recipes::parser::…` reference sites.
- `src/tdd/hooks.rs` — 1,002 production lines: struct + inherent impl, eleven `before_*` and nine
  `after_*` **free** functions, then `impl RunnerHooks` calling them.
- `src/tdd_small/hooks.rs` — 697 production lines, same shape.

### State B

- One module per parser phase, each self-contained; `parser.rs` holds `ParseError` and the facade.
- Each hooks file holds only its struct, inherent impl and `impl RunnerHooks`.
- No file among them over 500 production lines.

## Implementation phases

Almost entirely mechanical — this is the node that demonstrates the pattern the rest of the stack
follows.

| Phase | Kind | Work |
|---|---|---|
| **A′** | manual | Relocate `impl RedOutput` (887–974) to immediately after line 807, so both the `red` and `evaluate` seams become contiguous. One `impl` block, moved within one file |
| **A** | mechanical | `extract_module` + `to_file`, `reexport: "glob"`, one operation per seam, snapshot hashed **after** A′. One plan, since non-cross-crate Rust operations compose |
| **B** | manual | Nothing expected. Any import the restoration pass declines to reconstruct is hand-bound here, per the D8 alias case recorded in the connection-service-split entry |
| **C** | mechanical | `extract_module` for the two hooks files — a second plan, because `.restructure/` is repo-scoped and must be archived between plans until `#carve` 3/9 lands |
| **D** | manual | `README.md` module table |

### The ordering constraint in `parser.rs` — corrected at wave 2

The plan first written for this node was **wrong**, and writing it is what surfaced why.

`impl RedOutput` sits at **887–974**, *between* the Evaluate DTOs at 808–882 and the rest of them at
975 onward. `extract_module`'s anchor is a **single range** over a selection of items, so:

- an `evaluate` op spanning 808–1216 sweeps `impl RedOutput` into `evaluate`, where it does not belong;
- a `red` op spanning 553–807 leaves `impl RedOutput` behind in the parent;
- and **no ordering fixes it**, because the obstruction is *between* the two seams rather than at
  either end. The earlier note claiming "extract red before evaluate" closes the gap is incorrect:
  extracting red at 553–807 does not move 887–974 at all.

**The fix is a manual reorder first.** Move the `impl RedOutput` block (887–974) up to sit
immediately after `validate_red_marker_source_paths` (ends 807). That is a pure relocation of one
`impl` within one file — no behaviour change, and **moving a whole `impl` is free of caller churn**
because a method is reached through its type. Both seams are then contiguous and expressible as one
range each.

This makes the node **manual → mechanical**, not mechanical-only, and the snapshot hash must be taken
**after** the reorder — a plan written against the pre-reorder file will not verify.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [x] Publish the draft-PR contract (module shape + failing verify/budget assertions)
- [x] Failing acceptance tests — **USER REVIEW** (approved 2026-09-15)
  - `tests/module_shape.rs` — 4 failing: the six phases are not yet modules, the parent declares no
    facade, the four hook halves do not exist, and three files are over budget
    (`parser.rs` 1216, `tdd/hooks.rs` 1002, `tdd_small/hooks.rs` 697 production lines).
- [x] Failing unit/integration tests — the shape assertions above are the whole contract here; there
  is no new API surface to unit-test, every symbol keeping its name, signature and path
- [ ] Implement production code making tests pass (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

**⚠ Pre-existing failure, not this node's.** The baseline on this branch is **560 passed, 1 failed**,
and the failure reproduces on `master`:

    pr_stack_artifact_paths_acceptance::a_plan_left_at_the_legacy_session_root_is_still_advertised_to_the_agent

`/green` must not mistake it for this node's red. It is untouched by this node's seams.

```bash
./test -p tddy-workflow-recipes
cargo clippy -p tddy-workflow-recipes -- -D warnings
cargo fmt --all --check
tddy-tools restructure verify --against HEAD
```

The test count must match the pre-change baseline exactly — a behaviour-preserving move that changes
the count has moved logic.
