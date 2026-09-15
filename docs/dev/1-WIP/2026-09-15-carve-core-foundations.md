# Changeset: carve-core-foundations

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Refactor
**Stack**: `#carve` 4/9

PRD: [`2026-09-15-carve-core-foundations-prd.md`](./2026-09-15-carve-core-foundations-prd.md)

## Initial Discovery

[`2026-09-15-carve-core-foundations-initial-discovery.md`](./2026-09-15-carve-core-foundations-initial-discovery.md)

## Affected Packages

- **`tddy-core`**: [README.md](../../../packages/tddy-core/README.md) — three cycles gone;
  `changeset.rs` becomes `changeset/`; `Presenter` holds five sub-structs.
- **`tddy-workflow`**: [README.md](../../../packages/tddy-workflow/README.md) — becomes the shared
  vocabulary crate: `GoalId`, `WorkflowState`, `WorkflowEvent`, `ClarificationQuestion`,
  `QuestionOption`.

## Responsibility

- Move five DTO groups (~172 lines, all with **zero** `crate::` dependencies) from `tddy-core` into
  the existing `tddy-workflow` crate, re-exporting every old path.
- Retire `backend/mod.rs:230-231`'s re-export facade.
- Split `changeset.rs` (964 prod) into `changeset/{stack,model,io,merge}.rs` behind a facade.
- Group `Presenter`'s 37 fields into five owned sub-structs.

## Boundaries

- Does **not** split `Presenter`'s 46 methods — `#carve` 8/9 does, consuming these sub-structs.
- Does **not** extract `backend/` to a crate. It stays in the SCC via `workflow::recipe`, and
  `workflow/recipe.rs` is **not** a leaf. Recorded as a todo, not attempted here.
- Does **not** touch `worktree.rs` (`#carve` 5/9), `session_actions/` or `session_catalog/`
  (`#carve` 6/9).
- Does **not** edit any consumer crate. All 35 dependents of `tddy-core` keep compiling unchanged.
- Does **not** change any behaviour. FR3 moves field access sites; the existing tests prove it.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `1/9` restructure-moves | nested anchors accepted; facade-aware cycle refusal | Phase C moves `workflow/ids.rs` and `presenter/events.rs` — **both nested** — into `tddy-workflow`, and the origin keeps a `pub use` facade that would otherwise read as a cycle | touch `tddy-code-restructuring` |
| `3/9` restructure-clusters | cluster moves; plan-scoped journal | **not consumed as behaviour** — the five DTO groups do not reference each other, so they move one at a time. The plan-scoped journal is convenience, not a dependency | rely on cluster moves |
| `2/9` recipe-parsers | nothing this node consumes | — | touch `tddy-workflow-recipes` |

> **Sequencing note, not a licence:** this node is *based on* 3/9 because the stack is a line. Its
> only real predecessor is 1/9.

## Draft PR contract

Published first:

1. The five sub-struct type definitions (`WorkflowRun`, `PendingQuestions`, `ActivityRecorder`,
   `ViewChannels`, `BackendSelection`) with their real field sets — this is what `#carve` 8/9
   compiles against.
2. `tddy-workflow`'s new public surface for the five moved DTO groups.
3. `changeset/{stack,model,io,merge}` module shape with the facade.
4. Failing tests pinning AC1–AC7.

This PR goes on to implement all of it. **It must not merge in that state.**

## Green wave

**Wave:** 2 of 4
**Greenable independently:** **no** — Phase C moves two **nested** modules and leaves a `pub use`
facade behind. Both refusals `#carve` 1/9 fixes are on this node's path, and they must exist as
behaviour before the moves apply. The `git mv` fallback exists but is what this stack was built to
avoid
**Concurrent with:** `#carve` 3/9 `restructure-clusters`, 5/9 `git-plumbing`
**Blocks:** 8/9 `presenter-split` (needs FR3's sub-structs), 9/9 `pr-stack-crate` (needs FR2's
`changeset/stack.rs`)

Real dependency edges, as opposed to the branch line:

    n1 → n3, n4, n5, n6, n9      n3 → n6, n7, n9      n4 → n8, n9      n5 → n9

## Prerequisites

| Entry | Verdict | What this node does with it |
|---|---|---|
| [2026-07-01-tddy-core.md](../todo/2026-07-01-tddy-core.md) | ⚠ **DURING** | Pre-existing `tddy-core` notes. Read before splitting `changeset.rs`; this node must not re-introduce what it records. Not claimed. |
| [2026-07-22-tddy-core-tddy-coder-tddy-daemon.md](../todo/2026-07-22-tddy-core-tddy-coder-tddy-daemon.md) | ⚠ **DURING** | Same — cross-crate concerns touching `tddy-core`'s surface. Not claimed. |
| [2026-09-12-the-two-new-service-rs-files-are-over-budget.md](../todo/2026-09-12-the-two-new-service-rs-files-are-over-budget.md) | ℹ **Answered** | Establishes the file-budget convention this node's AC4 applies, and records that "a split for line count alone cuts cohesive units". FR2's seams are cohesion-led, not size-led. Not claimed — it names different files. |

A new todo is filed by this node for **`backend/` cannot be extracted while `workflow/recipe.rs` is
not a leaf**, recording the four `crate::` dependencies that hold it in the SCC.

## State A → State B

### State A

- Six-module SCC in `tddy-core`: `backend`, `stream`, `toolcall`, `workflow`, `changeset`,
  `presenter`. Each edge is 1–4 symbols wide and every one is a DTO.
- `workflow/ids.rs` (107) and `presenter/events.rs` (40) have **zero** `crate::` dependencies;
  `ClarificationQuestion`/`QuestionOption` (`backend/mod.rs:510`, `:523`) are plain serde structs.
- `tddy-workflow` is 384 lines — `artifact_paths.rs` plus a 10-line `lib.rs` — with **no `tddy-*`
  dependencies**, and both affected crates already depend on it.
- `changeset.rs` — 964 prod lines holding two unrelated data models plus I/O and merge functions.
- `presenter/presenter_impl.rs` — 1,788 prod lines; one struct, **37 fields**; one impl, **46 methods**.

### State B

- Three cycles gone; the fourth thinned to the recipe trio.
- `tddy-workflow` is the shared vocabulary crate; no consumer edited.
- `changeset.rs` under 200 prod lines behind a facade, with `Stack` in its own module.
- `Presenter` holds seven fields: five sub-structs, `state`, `tddy_data_dir`.

## Implementation phases

This node is the stack's clearest **mechanical → manual → mechanical** case.

| Phase | Kind | Work |
|---|---|---|
| **A** | mechanical | `extract_module --to_file` × 4 on `changeset.rs` → `changeset/{stack,model,io,merge}.rs`, `reexport: "glob"`. Also lifts `ClarificationQuestion`/`QuestionOption` out of `backend/mod.rs` into a flat `backend_questions` module — **the flat shape `move_module_to_crate` requires** |
| **B** | manual | `tddy-workflow`'s `lib.rs` and `Cargo.toml` — there is **no `create_file` operation**, by design, so a destination crate's skeleton is always hand-written. Re-point `backend/mod.rs:230-231`'s facade at real paths before Phase C, so the cycle refusal sees the truth |
| **C** | mechanical | `move_module_to_crate` × 3 — `workflow/ids.rs` (nested), `presenter/events.rs` (nested), `backend_questions` (flat) → `tddy-workflow`, `reexport: "glob"`. **The two nested ones are why this node needs `#carve` 1/9** |
| **D** | manual | FR3: the five sub-structs and every field access site. Hand-written — grouping fields into owned types is a type-level change no assist expresses |
| **E** | mechanical | `extract_module --to_file` on whatever `presenter_impl.rs` Phase D leaves oversized — measured, not assumed |
| **F** | manual | `README.md` × 2, the new `backend/`-extraction todo |

Phase D is the only substantial hand-written code in this node, and it is the one part that is not a
move. Everything else is an intent.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [ ] Publish the draft-PR contract (sub-struct types + `tddy-workflow` surface + module shape)
- [ ] Failing acceptance tests — **USER REVIEW**
- [ ] Failing unit/integration tests
- [ ] Implement production code making tests pass (`/green`)
- [ ] File the `backend/`-extraction todo
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

```bash
./test -p tddy-core -p tddy-workflow
cargo clippy -p tddy-core -p tddy-workflow -- -D warnings
cargo fmt --all --check
tddy-tools restructure verify --against HEAD
```

`tddy-core` has **35 dependents**, so a `cargo build -p` of two or three of them — `tddy-coder`,
`tddy-tools` — is the cheap proof that AC2 holds and no consumer needed editing. The whole-workspace
answer comes from CI.
