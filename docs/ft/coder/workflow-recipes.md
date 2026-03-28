# Workflow recipes (pluggable workflows)

**Product area:** Coder  
**Updated:** 2026-03-28

## Summary

Workflow behavior is defined by **recipes** in the **`tddy-workflow-recipes`** crate. **`tddy-core`** implements a recipe-agnostic engine (`WorkflowRecipe`, `WorkflowEngine`, graph execution, `CodingBackend`). Goals, states, transitions, hooks, backend hints, and permissions are **recipe-provided strings and metadata**, not a fixed enum in core.

The default recipe is **`TddRecipe`** (plan → acceptance-tests → red → green → demo → evaluate → validate → refactor → update-docs). A minimal **`BugfixRecipe`** exists as an **Open-Closed** stub: additional workflows ship as new recipe types without changing core enums.

## Key types

| Concept | Role |
|---------|------|
| **`GoalId`**, **`WorkflowState`** | String-backed identifiers (serde-transparent); core does not match on fixed goal names. |
| **`WorkflowRecipe`** | Trait: graph, hooks, state machine helpers, permissions, artifacts, backend hints (`GoalHints` / `PermissionHint`). |
| **`TddRecipe`** | Full TDD workflow graph, `TddWorkflowHooks`, parsers, plan task wiring. |
| **`BugfixRecipe`** | Stub recipe proving a second workflow can compile alongside `TddRecipe` without core changes. |

## CLI and services

- **`--goal`** accepted values come from the active recipe’s declared goals (no hard-coded enum in the CLI).
- **gRPC / daemon** already use string goals and states; status mapping uses recipe-oriented helpers where applicable.

## Packages

| Package | Responsibility |
|---------|----------------|
| **`tddy-core`** | `WorkflowRecipe` trait, `WorkflowEngine` parameterized by `Arc<dyn WorkflowRecipe>`, session storage, backends, presenter integration. |
| **`tddy-workflow-recipes`** | Concrete recipes (`TddRecipe`, `BugfixRecipe`), hooks moved from TDD-specific core modules. |
| **`tddy-coder`**, **`tddy-service`**, **`tddy-daemon`**, **`tddy-demo`** | Construct or default to `TddRecipe` (or configured recipe). |

## Structured output contracts

JSON Schemas for workflow goals (`plan`, `red`, `green`, etc.) live in **`tddy-workflow-recipes`**; **`tddy-tools`** embeds them for `get-schema`, `list-schemas`, and `submit` validation. See [Workflow JSON Schemas](workflow-json-schemas.md) for the registry (`goals.json`), CLI behavior, and links to package-level technical notes.

## Session artifacts and primary planning documents

**Goal IDs** (e.g. `"plan"`) stay stable as wire/API identifiers. **Filenames and on-disk layout** for the primary planning document and related artifacts are defined by each recipe’s manifest (**`SessionArtifactManifest`**, `default_artifacts` / `known_artifacts`), not by fixed defaults inside **`tddy-core`**.

- **`tddy-core`** exposes **`WorkflowRecipe::uses_primary_session_document`** and **`read_primary_session_document_utf8`** for approval gates, plain CLI, and daemon flows.
- **`tddy-workflow`** provides **`artifact_paths`** helpers (`session_dir/artifacts/`, legacy `sessions/<uuid>/` layouts, resolution order).
- The shipped **TDD** recipe continues to use **`prd` → `PRD.md`** in its manifest; behavior for default TDD is unchanged.

Custom recipes **must** declare a `prd` (or equivalent) key if they rely on planning-specific paths; there is no silent core fallback string for the primary planning basename.

## Session context and conditional transitions

Workflow transitions read **boolean conditions** from the engine `Context` (declarative `goal_conditions` on `WorkflowTransition`). The TDD recipe supplies keys such as **`run_optional_step_x`** so the full graph branches after green without presenter-specific branching for demo vs evaluate.

**Session storage:** **`tddy-tools set-session-context`** merges JSON into **`.workflow/<session-id>.session.json`** under the session directory (`TDDY_SESSION_DIR`, `TDDY_WORKFLOW_SESSION_ID`). Values sync into the workflow engine context via **`Context::merge_json_object_sync`** before transition evaluation.

**Recipe hooks:** Green-completion hooks in the TDD recipe instruct the agent to call **`tddy-tools ask`**, then **`tddy-tools set-session-context`** with `{"run_optional_step_x":true}` or `{"run_optional_step_x":false}` so the next transition predicate matches.

**Authoring:** Declare **`goal_conditions`** on transitions in workflow JSON; use **`merge_json_object_sync`**-compatible keys in session documents. See [Workflow JSON Schemas](workflow-json-schemas.md) for the goals registry and schema layout.

## Related

- [Coder overview](1-OVERVIEW.md) — capabilities and integration points  
- [Workflow JSON Schemas](workflow-json-schemas.md) — schema ownership, `tddy-tools` CLI, `goals.json`  
- [Planning step](planning-step.md), [Implementation step](implementation-step.md) — goal-level behavior (TDD recipe)  
- [gRPC remote control](grpc-remote-control.md) — string goals/states in RPC  
