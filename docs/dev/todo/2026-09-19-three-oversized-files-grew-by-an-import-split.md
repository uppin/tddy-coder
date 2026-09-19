# Three files already over budget grew by an import split in `#carve` 5/11

**Filed:** 2026-09-19 by `#carve` 5/11 (PR #491)
**Status:** open — **deferral consented by the developer** at wrap, 2026-09-19

`#carve` 5/11 retired `backend/mod.rs`'s re-export of `GoalId` and the recipe trio, so 25 consumer
files had one `use` declaration split in two:

```rust
-use tddy_core::backend::{CodingBackend, GoalHints, GoalId, PermissionHint};
+use tddy_core::backend::CodingBackend;
+use tddy_core::workflow::ids::GoalId;
+use tddy_core::workflow::recipe::{GoalHints, PermissionHint};
```

Three of those files were **already over the 500-production-line budget**, so the split trips
`/pr-wrap`'s step 3.5 "already ≥ 500 and this PR grew it further → decompose now":

| File | was → now | growth |
|---|---|---|
| `packages/tddy-core/src/backend/claude.rs` | 766 → 767 | +1 |
| `packages/tddy-core/src/backend/stub.rs` | 577 → 578 | +1 |
| `packages/tddy-workflow-recipes/src/pr_stack/mod.rs` | 1967 → 1969 | +2 |

## Why it was deferred

- The growth is **entirely line-splitting an import**. No logic was added to any of the three.
- Decomposing a 767-line backend module inside a 69-file refactor would dwarf the change under
  review, and `restructure anchors` is currently broken
  (`packages/tddy-code-restructuring/docs/code-issues/broken-restructure-anchors-empty-outline.md`),
  so every split would be hand-written.
- **`pr_stack/mod.rs` is claimed by #496** (`#carve` 10/11), which moves that whole module into a
  new `tddy-pr-stack` crate. Splitting it here conflicts with that PR and the work is discarded when
  #496 lands. Per AGENTS.md's claim protocol this was put to the developer rather than decided.

## What would close it

Nothing in this entry is a new defect — each file's real oversize predates `#carve` and belongs to
its own `oversized-file-*` record. Close this entry when those three files are next measured, either
by the records that own them or by whichever node decomposes them. `pr_stack/mod.rs` is expected to
be answered by #496 rather than by a split.
