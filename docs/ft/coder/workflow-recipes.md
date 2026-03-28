# Workflow recipes (pluggable workflows)

**Product area:** Coder  
**Updated:** 2026-03-28

## Summary

Workflow behavior is defined by **recipes** in the **`tddy-workflow-recipes`** crate. **`tddy-core`** implements a recipe-agnostic engine (`WorkflowRecipe`, `WorkflowEngine`, graph execution, `CodingBackend`). Goals, states, transitions, hooks, backend hints, and permissions are **recipe-provided strings and metadata**, not a fixed enum in core.

The shipped recipes are **`TddRecipe`** (default) and **`BugfixRecipe`**. Recipe selection uses a single resolution path in **`tddy-workflow-recipes::recipe_resolve`**: `workflow_recipe_and_manifest_from_cli_name` and `resolve_workflow_recipe_from_cli_name` return the active `WorkflowRecipe` (and, where needed, the paired **`SessionArtifactManifest`** on the same concrete type).

## Selecting a recipe

| Surface | Mechanism |
|---------|-----------|
| **tddy-coder** | `--recipe tdd` or `--recipe bugfix`; optional YAML `recipe:` (CLI overrides). |
| **changeset.yaml** | Optional `recipe:` records the workflow for resume and session lists; empty or absent values behave like **`tdd`**. |
| **tddy-daemon** | Spawns **`tddy-coder`** with `--recipe` when set; **`ConnectionService` `StartSessionRequest`** and **`TddyRemote` `StartSession`** carry a **`recipe`** string. |
| **tddy-web** | **ConnectionScreen** exposes a **Workflow recipe** control (TDD vs Bugfix) per **Start New Session**; the value is sent on **`StartSession`**. |

Allowed names are **`tdd`** and **`bugfix`** (aligned with **`WorkflowRecipe::name()`**). Invalid names fail on the CLI with a clear error; daemon streams report failure via **`WorkflowComplete`** with a descriptive message.

## TddRecipe

- **Start goal:** **`plan`**
- **Pipeline:** plan → acceptance-tests → red → green → demo → evaluate → validate → refactor → update-docs (full TDD graph).
- **Primary session document:** **`prd`** → **`PRD.md`** under the session artifact layout (see **`SessionArtifactManifest`**).

## BugfixRecipe

- **Start goal:** **`reproduce`**
- **Pipeline:** reproduce → green (focused bugfix graph); human approval gates the session document before implementation work that matches **green** / fix semantics.
- **Primary session document:** fix-plan style content (e.g. **`fix-plan.md`**); **`BugfixRecipe::uses_primary_session_document`** is **`true`** so preview / approve / reject / refine flows apply before **green**.
- **Product alignment:** The workflow follows the same discipline as **reproduce-then-fix** and **focused test repair** workflows (deterministic reproduction, small verification loops).

## Key types

| Concept | Role |
|---------|------|
| **`GoalId`**, **`WorkflowState`** | String-backed identifiers (serde-transparent); core does not match on fixed goal names. |
| **`WorkflowRecipe`** | Trait: graph, hooks, state machine helpers, permissions, artifacts, backend hints (`GoalHints` / `PermissionHint`). |
| **`TddRecipe`** | Full TDD workflow graph, `TddWorkflowHooks`, parsers, plan task wiring. |
| **`BugfixRecipe`** | Bugfix workflow graph, hooks, and artifact manifest for reproduce / fix-plan / green. |
| **`recipe_resolve`** | **`workflow_recipe_and_manifest_from_cli_name`**, **`resolve_workflow_recipe_from_cli_name`**, **`unknown_workflow_recipe_error`**, **`WorkflowRecipeAndManifest`**. |

## CLI and services

- **`--goal`** accepted values come from the active recipe’s declared goals (no hard-coded enum in the CLI).
- **gRPC / daemon** use string goals and states; **`DaemonService`** loads **`TddRecipe`** or **`BugfixRecipe`** via **`workflow_recipe_and_manifest_from_cli_name`** when handling **`StartSession`**.

## Packages

| Package | Responsibility |
|---------|----------------|
| **`tddy-core`** | `WorkflowRecipe` trait, `WorkflowEngine` parameterized by `Arc<dyn WorkflowRecipe>`, session storage, backends, presenter integration. |
| **`tddy-workflow-recipes`** | Concrete recipes, **`recipe_resolve`**, hooks, parsers, and backend hints per recipe. |
| **`tddy-coder`**, **`tddy-service`**, **`tddy-daemon`**, **`tddy-demo`** | Resolve the active recipe from CLI, config, changeset, or RPC; default **`tdd`** when unspecified. |

## Structured output contracts

JSON Schemas for workflow goals (`plan`, `red`, `green`, etc.) live in **`tddy-workflow-recipes`**; **`tddy-tools`** embeds them for `get-schema`, `list-schemas`, and `submit` validation. See [Workflow JSON Schemas](workflow-json-schemas.md) for the registry (`goals.json`), CLI behavior, and links to package-level technical notes.

## Session artifacts and primary planning documents

**Goal IDs** (e.g. `"plan"`, `"reproduce"`) stay stable as wire/API identifiers. **Filenames and on-disk layout** for the primary planning document and related artifacts are defined by each recipe’s manifest (**`SessionArtifactManifest`**, `default_artifacts` / `known_artifacts`), not by fixed defaults inside **`tddy-core`**.

- **`tddy-core`** exposes **`WorkflowRecipe::uses_primary_session_document`** and **`read_primary_session_document_utf8`** for approval gates, plain CLI, and daemon flows.
- **`tddy-workflow`** provides **`artifact_paths`** helpers (`session_dir/artifacts/`, legacy `sessions/<uuid>/` layouts, resolution order).
- **TDD** uses **`prd` → `PRD.md`** in its manifest; **Bugfix** uses **`fix_plan` / `fix-plan.md`** for the primary session document.

Custom recipes **declare** artifact keys in manifest; there is no silent core fallback string for the primary planning basename.

## Related

- [Coder overview](1-OVERVIEW.md) — capabilities and integration points  
- [Workflow JSON Schemas](workflow-json-schemas.md) — schema ownership, `tddy-tools` CLI, `goals.json`  
- [Planning step](planning-step.md), [Implementation step](implementation-step.md) — goal-level behavior  
- [gRPC remote control](grpc-remote-control.md) — string goals/states in RPC  
- Developer reference: [workflow-recipes-tdd-bugfix.md](../../../docs/dev/1-WIP/workflow-recipes-tdd-bugfix.md) — TDD vs Bugfix mapping and test commands  
