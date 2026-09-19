# 2026-09-19 — This crate becomes the shared vocabulary

**Type:** Refactor · `#carve` 5/11, PR [#491](https://github.com/uppin/tddy-coder/pull/491)
Cross-package entry: [`docs/dev/changesets/2026-09-19-carve-core-foundations.md`](../../../../docs/dev/changesets/2026-09-19-carve-core-foundations.md)

This crate held one thing — `artifact_paths` — and had **no `tddy-*` dependencies**, which is what
made it the right home for the data types every layer names. Four modules arrive from `tddy-core`:

| Module | Types | Came from |
|---|---|---|
| `ids` | `GoalId`, `WorkflowState` | `workflow/ids.rs` |
| `questions` | `ClarificationQuestion`, `QuestionOption` | `backend/mod.rs` |
| `progress` | `ProgressEvent` | `stream/mod.rs` |
| `events` | `WorkflowEvent`, `WorkflowCompletePayload` | `presenter/events.rs` |

All four are re-exported from the crate root. `serde` is added for `ids` and `questions`; it is the
only new dependency, and the crate still depends on no `tddy-*` crate.

They live here so `tddy-core`'s modules can name a shared DTO **without naming each other** — three
of that crate's five module cycles were nothing but that. `ProgressEvent` was not in the original
plan: `WorkflowEvent` carries one, and a destination crate cannot reach back into the crate it left.

Each origin in `tddy-core` keeps a glob facade, so no consumer of a moved type was edited.
