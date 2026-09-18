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

### The same obstruction in `tdd/hooks.rs` — found at `/green`

The plan above named **one** interleaved item. There are **two**, and the second is in the file the
plan called purely mechanical.

`after_interview` sits at **207–228**, *between* `before_interview` (146–206) and
`before_plan_with_interview` (229 onward) — so the `before` half is **two** ranges, 146–206 and
229–532, and `extract_module`'s anchor is one. It is the identical geometry to `impl RedOutput`: the
obstruction is *between* the halves rather than at either end, so no ordering of the two operations
reaches it. A `before` op spanning 146–532 sweeps `after_interview` into the wrong half; one spanning
146–206 abandons nine tenths of the seam.

The fix is the same and just as cheap: move `after_interview` (207–228) down to sit immediately
before `after_plan` (533) by hand, then both halves are one range each. A free function is reached by
name, so like the `impl` this relocation causes **no caller churn** — `impl RunnerHooks` goes on
calling it unchanged.

So phase A′ is **two** relocations, not one, and the lesson generalises past this node: a "same shape,
purely mechanical" file is worth a per-item outline before it is planned as one, because an interleave
of this class is invisible in a summary that only counts `before_*` and `after_*`.

### The grouped test imports that refuse the cut — phase B, found at `/green`

`green` and `red` were the two seams whose operations **refused**, and neither the seam nor the plan
was at fault:

    plan is malformed: lsp: lsp server error -32603: request handler panicked:
    assertion failed: check_disjoint_and_sort(indels)

Two tests carried a **grouped** import of items the seam moves —
`use super::{RedOutput, RedTestInfo, SkeletonInfo};` and
`use super::{GreenOutput, GreenTestResult, ImplementationInfo};`. Re-pointing three names inside one
`use` tree makes rust-analyzer emit three overlapping edits over the same span, and the assist panics
rather than refusing. `acceptance_tests` carries the same kind of import for a **single** name
(`use super::parse_acceptance_tests_response;`) and resolves fine, which is what isolates the group as
the cause rather than the seam's size or its trailing `impl`.

Both imports were already redundant — `use super::*;` at the top of `mod tests` binds all six — so
splitting each into one `use` per line is behaviour-identical, and with that done both operations
resolve. This is the phase the changeset reserved for "any import the restoration pass declines to
reconstruct"; the reality is the inverse, an import shape that makes the assist unable to reconstruct
anything.

**Generalises to the rest of the stack:** before planning a seam, grep the file's test modules for
`use super::{` naming more than one item the seam moves. It is a one-line pre-step and it is the
difference between an operation that runs and one that panics.

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
- [x] Implement production code making tests pass (`/green`) — 10 `extract_module` operations in one
  plan, all four `module_shape` tests green, largest produced file 406 production lines
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

**⚠ Pre-existing failure, not this node's.** It reproduces on `master`:

    pr_stack_artifact_paths_acceptance::a_plan_left_at_the_legacy_session_root_is_still_advertised_to_the_agent

`/green` must not mistake it for this node's red. It is untouched by this node's seams.

Measured, both runs `./test -p tddy-workflow-recipes --no-fail-fast` over 59 suites:

| Run | Passed | Failed |
|---|---|---|
| Baseline (with `tests/module_shape.rs` red) | 560 | 5 — the pre-existing one, plus this node's 4 |
| After `/green` | **564** | **1** — the pre-existing one alone |

The four that turned green are the only change to the count: 560 + 4 = 564, nothing else moved.

**`--no-fail-fast` is not optional for this measurement.** `./test` does not pass it, so a plain run
aborts at the first failing suite — and `module_shape` sorts early enough that the run stops there,
reporting 341 of the 565 tests and none of the suites after it. The earlier "560 passed, 1 failed"
figure in this document was measured before `tests/module_shape.rs` existed; it is consistent with the
table above but was not comparable to a red-phase run.

```bash
./test -p tddy-workflow-recipes --no-fail-fast
cargo clippy -p tddy-workflow-recipes --all-targets -- -D warnings
cargo fmt --all -- --check
tddy-tools restructure verify --against HEAD
```

The test count must match the pre-change baseline exactly — a behaviour-preserving move that changes
the count has moved logic.

### What `restructure verify --against HEAD` reports, and why it is not zero

It reports **205 statements lost, 235 gained**, and every one is accounted for. `verify` compares
trimmed lines as multisets and excuses only the scaffolding a restructure is *supposed* to churn —
`use`, `mod`, `impl` and bare braces (`verify.rs::is_structural`). It does not excuse the two things
`extract_module` always does:

- **134 visibility widenings** — `file: String,` → `pub(crate) file: String,` on the fields of the
  private `…De` deserialization mirrors. The assist writes relocated items `pub(crate)`, and the
  survey that restores visibility does not descend into a struct's fields. Documented in
  `plan-schema.md`, reported by the run.
- **39 reference re-points** — `before_interview(context)?` → `before::before_interview(context)?`,
  which is the assist doing its job.
- **32 further deltas are `rustfmt` reflowing lines the two above made longer**: nine `fn` signatures
  that no longer fit on one line once prefixed `pub(crate) `, six `match` arms that gained a block for
  the same reason, and the continuation lines of `use` groups the moves emptied (only a group's *first*
  line starts with `use`, so `is_structural` does not filter the rest).

`verify`'s own purpose — *"a comment attached to no item"* — **did** fire, and was the one real
finding: `// ── evaluate-changes output types ──` sat between the two halves of the evaluate seam,
belonged to no item, and so was carried nowhere. It was restored by hand in `parser/evaluate.rs` at
the position it held, and no comment is unmatched now. Nothing else this node produced was
hand-written.

So the exit code cannot be zero for any `extract_module` that re-points a reference or widens a field,
and reading the two lists is the gate rather than the status. The follow-up — teach `is_structural` to
normalise a leading `pub(crate) ` on *any* line, and join physical lines into statements before
comparing, which would have left the comment finding standing alone — is filed as
[`2026-09-18-restructure-verify-cannot-exit-zero-for-an-extract-module`](../todo/2026-09-18-restructure-verify-cannot-exit-zero-for-an-extract-module.md)
so it outlives this changeset's wrap.
