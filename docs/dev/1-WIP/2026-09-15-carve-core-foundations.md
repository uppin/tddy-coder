# Changeset: carve-core-foundations

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Refactor
**Stack**: `#carve` 4/9
**PR**: [#491](https://github.com/uppin/tddy-coder/pull/491)

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

**Published** (commit 2): `src/presenter/state_groups.rs` — the five sub-structs with their **real
field sets and real types**, exported from `presenter/mod.rs`. This is what `#carve` 8/9 compiles
against, and it is the one substantial hand-written part of this node: grouping fields into owned
types is a type-level change no assist expresses.

`PendingToolCallResponse` and `RecipeResolverFn` move with the groups that hold them, since a
private type cannot be a field of a published one.

`tddy-workflow`'s surface and the `changeset/` module shape are **moves**, not new API, so they are
pinned by `tests/core_foundations_shape.rs` rather than declared.

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
- [x] Publish the draft-PR contract (sub-struct types + `tddy-workflow` surface + module shape)
- [x] Failing acceptance tests — **USER REVIEW** (approved 2026-09-15, gates delegated)
  - `tests/core_foundations_shape.rs` — 6 failing: the three DTO cycles still exist, `backend/mod.rs`
    still re-exports the workflow vocabulary, `changeset.rs` is not split, and `Presenter` still
    holds 37 fields rather than seven.
- [x] Failing unit/integration tests — the same suite; three of this node's four claims are about *structure*, which the type system cannot observe once the code compiles (a cycle between two modules of one crate compiles perfectly well — that is why `tddy-core` has six)
- [x] Implement production code making tests pass (`/green`) — **7/7 shape tests green**
- [x] File the `backend/`-extraction todo
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## What green actually did

Three things the plan did not anticipate. Each is recorded here rather than absorbed silently.

### 1. AC3 and `## Boundaries` could not both hold — the facade was retired

`## Responsibility` says "retire `backend/mod.rs:230-231`'s re-export facade" and AC3 asserts it.
`## Boundaries` says "does not edit any consumer crate". **25 files outside `tddy-core`** (14 in
`tddy-workflow-recipes`, 10 in `tddy-session-lifecycle`, 1 integration test) import the recipe trio
through `tddy_core::backend::`, so retiring the facade necessarily edits them.

Put to the developer, who chose **retire it and re-point the 25 files**. Retiring the facade *is*
the removal of a path consumers use; a renamed facade would satisfy the assertion's string while
leaving the misdirection AC3 exists to remove. The edits are one-line import changes; `backend`
still *uses* `GoalId`/`GoalHints` privately.

The boundary as written was reachable only for the **moves**, where glob facades genuinely cost no
consumer a diff — not for the facade retirement.

### 2. `ProgressEvent` had to move too

The plan lists five DTO groups. `WorkflowEvent` carries a `ProgressEvent`, which lived in
`stream/mod.rs`, and a destination crate cannot reach back into the crate it left — so
`ProgressEvent` moved with it, as a sixth. `tddy-workflow` gains `serde` for the same reason.

### 3. Phases A and C were mechanical in the plan and manual in fact

Both restructuring operations this node was sequenced around refused. Written up as standing
records under `packages/tddy-code-restructuring/docs/code-issues/`:

| Phase | Planned | What happened |
|---|---|---|
| **A** | `extract_module --to_file` × 4 | [`restructure anchors` resolves no item at all](../../../packages/tddy-code-restructuring/docs/code-issues/broken-restructure-anchors-empty-outline.md) — warm path returns an empty outline in 3–89 ms, cold path exits with `lsp server exited`. Split by hand. |
| **C** | `move_module_to_crate` × 3 | [refuses whenever any file left behind names the module](../../../packages/tddy-code-restructuring/docs/code-issues/refusal-move-module-to-crate-any-caller-left-behind.md) — 19 findings over 4 moves. That is every caller of a shared DTO, which is the operation's whole purpose. `git mv` + hand-written glob facade. |

A prerequisite the plan missed either way: `extract_module`'s anchor is a **single range over
contiguous items**, and not one seam here was contiguous — `Stack` sat at 41–274 *and* 841–937, the
question DTOs at 508–528 with their `default_allow_other` helper stranded at 484. Making the items
contiguous is itself a hand move no operation expresses, so Phase A was never four operations.

This is the third stack to plan around `move_module_to_crate` and the second to fall back to
`git mv` (`#unbundle` node 2 moved 0 of ~24).

### 4. Two published draft-contract types were fabricated

`state_groups.rs` shipped `tokio::sync::mpsc` where the presenter uses `std::sync::mpsc`, and
`PendingToolCallResponse::Ask(mpsc::Sender<String>)` / `Approve(mpsc::Sender<bool>)` where the real
shape is `Ask(oneshot::Sender<ToolCallResponse>)` / `Approve(oneshot::Sender<ToolCallResponse>)`.
As published the contract could not have compiled against the presenter. **Field and type names are
unchanged**; the two types were corrected to reality.

**`#carve` 9/11 (PR #495) should re-check against the corrected `state_groups.rs`** — the variant
payloads in particular are not a mechanical substitution.

### Phase E: measured, not done

`presenter_impl.rs` is **1,691 production lines** after Phase D. Splitting its 46 methods is
`#carve` 9/11's responsibility and this node's `## Boundaries` forbid it, so Phase E is a no-op
here rather than an omission. `changeset/` is well inside budget: model 313, stack 345, merge 274,
io 69, parent 17.

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
