# 2026-09-19 — The shared vocabulary leaves, and two god-files gain seams

**Type:** Refactor · `#carve` 5/11, PR [#491](https://github.com/uppin/tddy-coder/pull/491)
Cross-package entry: [`docs/dev/changesets/2026-09-19-carve-core-foundations.md`](../../../../docs/dev/changesets/2026-09-19-carve-core-foundations.md)

Six of this crate's modules formed a strongly-connected component, and every edge was a plain data
type living inside the behaviour module that happened to define it. The data moves to
[`tddy-workflow`](../../../tddy-workflow/README.md); each origin keeps a glob facade, so every old
path still resolves.

**Three of five cycles are gone**, SCC six modules → three:

| Cycle | Was |
|---|---|
| `backend ↔ stream` | `stream/{mod,claude,cursor}.rs` needed `ClarificationQuestion`, `QuestionOption` |
| `backend ↔ toolcall` | `toolcall/client_wire.rs` needed `QuestionOption` |
| `presenter ↔ workflow` | `workflow/{controller,recipe}.rs` needed `WorkflowEvent` |

Surviving: `backend ↔ workflow` (the recipe trio — `WorkflowRecipe` is a trait naming
`CodingBackend`, so no move reaches it; see
[the todo](../../../../docs/dev/todo/2026-09-19-backend-cannot-be-extracted-while-workflow-recipe-is-not-a-leaf.md))
and `workflow ↔ changeset`.

`backend/mod.rs` no longer **re-exports** `GoalId` or the recipe trio. It still uses them; it does
not publish them, so `tddy_core::workflow::{ids,recipe}::` are the paths to name — 25 files in three
other crates were re-pointed.

`changeset.rs` — 964 production lines holding two unrelated data models plus I/O and merge
functions — becomes `changeset/{model,stack,io,merge}.rs` behind a 17-line facade that defines
nothing (313 / 345 / 274 / 69 lines, all inside the 400 budget). `Presenter`'s 37 fields become
five owned sub-structs in `presenter/state_groups.rs`, leaving seven.

Behaviour-preserving throughout: every relocated item is byte-identical, and all ~190 `Presenter`
field accesses are faithful renames with defaults matching the old constructor.
