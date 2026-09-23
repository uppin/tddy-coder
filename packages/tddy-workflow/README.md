# tddy-workflow

The vocabulary a workflow is described in, plus session artifact layout.

## Quick Start

```bash
cargo build -p tddy-workflow
cargo test -p tddy-workflow
```

## Architecture

Two responsibilities, and no `tddy-*` dependencies — this crate sits under everything else.

**The shared vocabulary.** The data types every layer names, kept here rather than inside whichever
behaviour module happened to define them:

| Module | Types |
|---|---|
| `ids` | `GoalId`, `WorkflowState` — semantic newtypes for workflow goals and persisted state strings |
| `questions` | `ClarificationQuestion`, `QuestionOption` — what a backend asks an operator |
| `progress` | `ProgressEvent` — what an agent CLI's stream reports as it runs |
| `events` | `WorkflowEvent`, `WorkflowCompletePayload` — what the workflow thread sends the presenter |
| `hints` | `GoalHints`, `PermissionHint` — what a recipe tells a backend about one goal: the permission it runs under and its CLI hints |

They live here because `tddy-core`'s modules need to name them **without naming each other**.
`stream` and `toolcall` both need a `ClarificationQuestion` and neither needs a backend; the
workflow engine needs a `WorkflowEvent` and does not need the presenter. While those types lived in
`backend/` and `presenter/`, each of those needs was a module cycle. Moving the data out removed
three of `tddy-core`'s six (`#carve` 5/11).

`GoalHints` and `PermissionHint` are here for the same reason. They are the only thing a backend
takes from a recipe, so with them in this crate `tddy-agent-backend` does not depend on
`tddy-workflow-engine`, which is where `WorkflowRecipe` lives. `WorkflowRecipe` names
`CodingBackend`, not the reverse, so the remaining edge runs one way, engine → backend.

Every origin keeps a glob facade, so `tddy_core::backend::ClarificationQuestion` and friends still
resolve and **no DTO consumer was edited**. (Retiring `backend`'s re-export of the *recipe trio* was a
separate change, and that one did re-point 25 files.)

**Artifact layout** (`artifact_paths`) — session artifact roots and manifest-driven layout paths,
which is what decouples the workflow engine from fixed PRD paths.

## Documentation

- [Tech Stack](../../docs/dev/guides/tech-stack.md) — Workspace layout, toolchain
