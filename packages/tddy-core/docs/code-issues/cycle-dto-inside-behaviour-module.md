# cycle: two module cycles remain, no longer caused by a shared DTO

**Location:** `packages/tddy-core/src/` — `backend`, `stream`, `toolcall`, `workflow`, `changeset`, `presenter`
**Category:** cycle
**Detected:** 2026-09-15 by structural audit
**Metrics:** **3-module SCC** (was 6) · **2 cycles** (was 5) · 35 dependent crates
**Restructure:** the DTO half is done; what remains is a design change, not a move
**Status:** Open — **partially fixed by #491** (3 of 5 cycles gone, SCC 6 → 3). The remainder is not DTO movement.
**Claimed by:** nobody — see `docs/dev/todo/2026-09-19-backend-cannot-be-extracted-while-workflow-recipe-is-not-a-leaf.md`
**Lands after:** #488, #489, #490, #498

## Measurement history

| Run | SCC size | Cycles | Widest edge | Note |
|---|---|---|---|---|
| 2026-09-15 | 6 modules | 5 | 4 symbols | first detection |
| 2026-09-19 | **3 modules** | **2** | recipe trio | #491 moved `GoalId`/`WorkflowState`, `ClarificationQuestion`/`QuestionOption`, `ProgressEvent` and `WorkflowEvent` into `tddy-workflow` and retired `backend/mod.rs`'s re-export facade. `backend ↔ stream`, `backend ↔ toolcall` and `presenter ↔ workflow` are gone. |

## What the tool found

Production-only `crate::` census with comments stripped and each file truncated at its first
`#[cfg(test)]`. Six modules form a strongly-connected component, and **every** edge is a plain data
type living inside the behaviour module that happens to define it:

| Cycle | The entire edge |
|---|---|
| `backend ↔ stream` | `stream/mod.rs:9` — `use crate::backend::{ClarificationQuestion, QuestionOption};` |
| `backend ↔ toolcall` | `toolcall/client_wire.rs` — `crate::backend::QuestionOption` |
| `presenter ↔ workflow` | `workflow/` needs `crate::presenter::WorkflowEvent`, twice |
| `backend ↔ workflow` | `backend/mod.rs:230-231` — a **re-export facade** for `GoalId` and the recipe trio |
| `changeset ↔ workflow` | `workflow/` needs `crate::changeset::Changeset`, once |

The DTOs themselves are leaves: `workflow/ids.rs` (107 lines) and `presenter/events.rs` (40 lines)
have **zero** `crate::` dependencies, and `ClarificationQuestion`/`QuestionOption`
(`backend/mod.rs:510`, `:523`) are plain serde structs.

## Why it matters here

The cycles are why `tddy-core` cannot be decomposed, and its size is why that matters — **35 crates
depend on it**. Any attempt to extract `backend/`, `presenter/` or `workflow/` pulls the whole SCC.
The cycles are *also* almost free to break, which makes this the highest-leverage issue in the crate:
about 172 lines of pure-data movement removes three of five outright.

## What remains, measured 2026-09-19

| Cycle | Why it survives |
|---|---|
| `backend ↔ workflow` | `backend/` needs `GoalHints` and `PermissionHint` from `workflow::recipe`, and `workflow/` needs `backend` throughout. `workflow/recipe.rs` is not a leaf, and `WorkflowRecipe` is a **trait** whose methods name `CodingBackend` — so no DTO move reaches it. |
| `workflow ↔ changeset` | `workflow/{controller,recipe}.rs` need `changeset::Changeset`; `changeset/{model,merge,stack}.rs` need `workflow::{ids,context,recipe}`. Never claimed by #491. |

Neither is a shared DTO in a behaviour module any more, so **this record's original finding is
closed**. What is left is a genuine mutual dependency that a move cannot express.

## What would close it

Move `GoalId`/`WorkflowState` (`workflow/ids.rs`), `WorkflowEvent` (`presenter/events.rs`) and
`ClarificationQuestion`/`QuestionOption` into **`tddy-workflow`** — an existing 384-line crate with
**no `tddy-*` dependencies** that both this crate and `tddy-workflow-recipes` already depend on.
Re-export every old path so no consumer is edited. Retire the `backend/mod.rs:230-231` facade.

Breaks `backend ↔ stream`, `backend ↔ toolcall` and `presenter ↔ workflow`. Thins
`backend ↔ workflow` to the recipe trio. `/code-restructuring` job.

**What it does not close:** `workflow/recipe.rs` is **not** a leaf — it needs `backend`,
`changeset::Changeset`, `presenter::WorkflowEvent` and four `workflow::` submodules — so
`WorkflowRecipe` cannot travel with the others and the `backend ↔ workflow` edge survives. A
`tddy-agent-backend` crate stays out of reach until that is addressed separately.

## If you are about to change this code

#491 moves DTOs and rewires imports across `backend/`, `stream/`, `toolcall/`, `workflow/`,
`presenter/` and `changeset.rs`. It changes **no behaviour** and **no public path** — every old path
re-exports — so a change that merely *uses* these types is unaffected and needs no coordination.

Coordinate if you are **adding a type** to any of those modules that another would import, since
that is a new cycle #491 has not accounted for. Put it in `tddy-workflow` instead.

## Verified by hand

2026-09-15: the first census was built with a naive `grep -rhoE 'crate::[a-z_]+'` and was **wrong** —
it matched doc comments and test code, reporting a `changeset → worktree` edge that does not exist in
production (the hit was ``[`crate::worktree`]`` in a doc comment at `changeset.rs:473`). Re-measured
with comments stripped and files truncated at the first `#[cfg(test)]`. The five cycles above are
production-only and each was confirmed by opening the file at the cited line.
