# 2026-09-19 — The shared vocabulary leaves `tddy-core`, and its two god-files gain seams

**Type:** Refactor

`#carve` 5/11, PR [#491](https://github.com/uppin/tddy-coder/pull/491).

`tddy-core` is the workspace's god-crate — **35 crates depend on it** — and discovery found the
reason it cannot be decomposed is smaller than its size suggests. Six of its modules formed a
strongly-connected component, but **every edge was one to four symbols wide and every one was a
plain data type living inside the behaviour module that happened to define it**. `stream` needed a
`ClarificationQuestion` and got a dependency on `backend`; the workflow engine needed a
`WorkflowEvent` and got a dependency on `presenter`.

So the data moves out. `tddy-workflow` — an existing crate with **no `tddy-*` dependencies** that
both affected crates already depended on — becomes the shared vocabulary crate:

| Now in `tddy-workflow` | Came from |
|---|---|
| `ids` — `GoalId`, `WorkflowState` | `tddy-core` `workflow/ids.rs` |
| `questions` — `ClarificationQuestion`, `QuestionOption` | `tddy-core` `backend/mod.rs` |
| `progress` — `ProgressEvent` | `tddy-core` `stream/mod.rs` |
| `events` — `WorkflowEvent`, `WorkflowCompletePayload` | `tddy-core` `presenter/events.rs` |

Each origin keeps a glob facade, so every old path still resolves and **no consumer of a moved DTO
was edited**. `ProgressEvent` was not in the plan: `WorkflowEvent` carries one, and a destination
crate cannot reach back into the crate it left, so it travelled too.

**Three of five module cycles are gone** and the SCC drops from six modules to three. What survives
is `backend ↔ workflow` — `WorkflowRecipe` is a *trait* whose methods name `CodingBackend`, so no
data move reaches it — and `workflow ↔ changeset`, which this node never claimed.

`backend/mod.rs` also stops **re-exporting** the workflow's vocabulary. That facade is why the
`backend ↔ workflow` edge read as structural when it was a convenience. Retiring it is the one part
of this change that consumers feel: **25 files in three crates** now name
`tddy_core::workflow::{ids,recipe}::` instead. `backend` still *uses* `GoalId` and `GoalHints`; it
no longer publishes them.

Two god-files gain seams. `changeset.rs` held **two unrelated data models** plus I/O and merge
functions in 964 production lines; it becomes `changeset/{model,stack,io,merge}.rs` behind a facade
that defines nothing, which is what gives `#carve` 10/11 a `stack.rs` to extract a `tddy-pr-stack`
along. `Presenter`'s **37 fields** become five owned sub-structs — `WorkflowRun`,
`PendingQuestions`, `ActivityRecorder`, `ViewChannels`, `BackendSelection` — leaving seven; that is
what `#carve` 9/11 partitions the 46 methods along.

Nothing changes behaviour. Every relocated item is byte-identical after the move, every one of the
~190 `Presenter` field accesses is a faithful rename, and the group defaults match the old
constructor field for field.

## Measurements

| | Before | After |
|---|---|---|
| `changeset.rs` production lines | 964 | **17** (facade) |
| `changeset/{model,stack,merge,io}.rs` | — | 313 / 345 / 274 / 69 |
| `Presenter` fields | 37 | **7** |
| `presenter_impl.rs` production lines | 1,788 | 1,691 |
| Modules in `tddy-core`'s SCC | 6 | **3** |
| Two-node cycles | 5 | **2** |

Closes `oversized-file-changeset`. Narrows `cycle-dto-inside-behaviour-module` (the DTO finding is
closed; a genuine mutual dependency remains) and `god-object-presenter` (fields done, 46 methods are
#495's).

## What the restructuring engine could not do

This node was sequenced as **mechanical → manual → mechanical** and both mechanical phases refused.

- **`restructure anchors` resolves no item at all** — in any file, on either the warm or the cold
  path. The skill mandates it over hand-counted line numbers, so `extract_module` had no usable
  entry point and `changeset.rs` was split by hand.
- **`move_module_to_crate` refuses whenever any file left behind names the module** — 19 findings
  over four moves. For a shared DTO that is every caller, which is the operation's whole purpose.
  The moves were `git mv` plus a hand-written glob facade.

Both are recorded in `packages/tddy-code-restructuring/docs/code-issues/`. This is the third stack
to plan around `move_module_to_crate` and the second to fall back to `git mv`.
