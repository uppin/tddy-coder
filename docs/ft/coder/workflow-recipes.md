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

## Related

- [Coder overview](1-OVERVIEW.md) — capabilities and integration points  
- [Planning step](planning-step.md), [Implementation step](implementation-step.md) — goal-level behavior (TDD recipe)  
- [gRPC remote control](grpc-remote-control.md) — string goals/states in RPC  
