# Changeset: carve-recipe-parsers

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Refactor
**Stack**: `#carve` 2/9

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

1. The six `parser/` module files and the four hooks files, created by their `extract_module`
   operations, with the facade lines in place.
2. A failing `restructure verify --against HEAD` assertion pinning AC4, and the file-budget
   assertion pinning AC6.

There is no new API surface to declare — every symbol already exists and keeps its path. What lands
first is the shape.

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
| **A** | mechanical | `extract_module` + `to_file`, `reexport: "glob"`, one operation per seam. **Order matters — see below.** One plan, since non-cross-crate Rust operations compose |
| **B** | manual | Nothing expected. Any import the restoration pass declines to reconstruct is hand-bound here, per the D8 alias case recorded in the connection-service-split entry |
| **C** | mechanical | `extract_module` for the two hooks files — a second plan, because `.restructure/` is repo-scoped and must be archived between plans until `#carve` 3/9 lands |
| **D** | manual | `README.md` module table |

### The ordering constraint in `parser.rs`

`impl RedOutput` sits at **887–974**, *between* the Evaluate DTOs at 808–882 and the rest of them at
975–1006. So the Evaluate seam is **non-contiguous**, and `extract_module`'s anchor is a range over a
selection of items.

**Extract `red` before `evaluate`.** Lifting `impl RedOutput` out with the Red seam closes the gap and
leaves Evaluate contiguous, expressible as one range. The reverse order needs two operations for
Evaluate and leaves `impl RedOutput` stranded between them.

Moving a whole `impl` is free of caller churn — a method is reached through its type — so
`impl RedOutput` carries no rewrite cost wherever it lands.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [ ] Publish the draft-PR contract (module shape + failing verify/budget assertions)
- [ ] Failing acceptance tests — **USER REVIEW**
- [ ] Failing unit/integration tests
- [ ] Implement production code making tests pass (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

```bash
./test -p tddy-workflow-recipes
cargo clippy -p tddy-workflow-recipes -- -D warnings
cargo fmt --all --check
tddy-tools restructure verify --against HEAD
```

The test count must match the pre-change baseline exactly — a behaviour-preserving move that changes
the count has moved logic.
