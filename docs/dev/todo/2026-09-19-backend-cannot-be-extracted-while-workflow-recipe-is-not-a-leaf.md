# `backend/` cannot be extracted to a crate while `workflow/recipe.rs` is not a leaf

**Filed:** 2026-09-19 by `#carve` 5/11 (PR #491)
**Status:** open, unclaimed

`#carve` 5/11 removed three of `tddy-core`'s six-module SCC by moving the DTOs that formed them into
`tddy-workflow`. The fourth edge — `backend ↔ workflow` — is thinner than it was but still real, and
it is what stops `backend/` becoming a crate of its own.

## What holds it

`backend/` reaches `workflow::recipe` for the recipe trio, and `workflow/recipe.rs` is not a leaf:
it names `crate::` in four places, so it cannot travel to `tddy-workflow` the way `ids.rs` did.

- `WorkflowRecipe` is a **trait**, not data. Its methods name `CodingBackend`, which lives in
  `backend/` — so the two genuinely reference each other and neither is a DTO one of them can
  simply stop naming.
- `GoalHints` and `PermissionHint` *are* plain data and could move, but moving two of three leaves
  the trait behind and the edge intact, so it buys nothing on its own.

`#carve` 5/11 did retire `backend/mod.rs`'s **re-export** of the trio (AC3), which is a different
thing: `backend` no longer *publishes* the workflow's vocabulary, it only *uses* it. That is what
made the edge legible as a real dependency rather than a convenience. The 25 files outside
`tddy-core` that imported the trio through `tddy_core::backend::` now name
`tddy_core::workflow::recipe::` instead.

## What would unblock it

Breaking the `WorkflowRecipe ↔ CodingBackend` mutual reference — most likely by giving the trait a
narrower port type it can name without naming the backend module. That is a design change, not a
move, so it is not something the restructuring operations can express.

Recorded rather than attempted: `#carve` 5/11's `## Boundaries` rule out extracting `backend/`.
