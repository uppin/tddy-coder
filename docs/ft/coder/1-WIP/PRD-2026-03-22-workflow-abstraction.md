# PRD: Workflow Abstraction Layer

**Date**: 2026-03-22
**Status**: WIP
**Type**: Architectural refactoring — introduces workflow abstraction across all layers

## Summary

Replace the hard-coded TDD workflow in tddy-coder with a pluggable workflow abstraction. A new `tddy-workflow-recipes` package will host workflow definitions (starting with the existing TDD workflow and a future bug-fix workflow). Goals, statuses, transitions, permissions, and backend hints become dynamic — defined per-recipe rather than baked into core types.

## Background

Today, tddy-coder is tightly coupled to the TDD workflow:

- `Goal` enum in `backend/mod.rs` is a compile-time list of 10 TDD steps
- `WorkflowEngine` always constructs `build_full_tdd_workflow_graph`
- Hooks (`tdd_hooks.rs`) match on task IDs with TDD-specific state strings
- CLI hard-codes allowed `--goal` values and dispatches with `if/else` chains
- `next_goal_for_state` in `changeset.rs` maps TDD state strings to next goals
- Permission allowlists are per-TDD-step
- Backends match on `Goal` enum variants for model/temperature config
- Output parser expects goal-specific JSON shapes
- Daemon service maps TDD state strings to coarse statuses

Adding any new workflow (e.g., bug-fix: Reproduce → Green) requires modifying core enums, graph builders, hooks, CLI, permissions, backends, parser, and service code — violating the Open-Closed principle.

## Affected Features

- [Planning Step](../planning-step.md) — Plan goal becomes a recipe-defined step
- [Implementation Step](../implementation-step.md) — Red/green/demo/evaluate become recipe-defined
- [gRPC Remote Control](../grpc-remote-control.md) — Goal/state events become string-based (already are in proto)
- [1-OVERVIEW](../1-OVERVIEW.md) — Core capabilities section needs updating

## Proposed Changes

### What changes

1. **New package `tddy-workflow-recipes`** — hosts workflow definitions as Rust structs
2. **`Goal` enum removed** — replaced by string-based goal IDs
3. **Workflow graph construction** — moves from hard-coded `build_full_tdd_workflow_graph` to recipe-provided graph definitions
4. **Hooks** — become recipe-provided (each recipe supplies its own `RunnerHooks`)
5. **Backend config** — driven by recipe-provided hints (metadata) instead of `match Goal`
6. **CLI `--goal` validation** — derived from the active recipe's goal list
7. **State management** — recipes define their own state names and transitions; changeset stores whatever the recipe provides
8. **Permissions** — recipes provide per-goal permission allowlists
9. **Output parsing** — recipes define expected output shapes per goal

### What stays the same

- `CodingBackend` trait — backends remain pluggable
- `Graph` execution engine — the runner/executor is workflow-agnostic
- `PresenterView` trait — TUI rendering stays the same
- `RpcService` / gRPC transport — already string-based for goals/states
- `SessionStorage` — persistence mechanism unchanged
- Changeset YAML format — still stores state as strings

## Requirements

### R1: Dynamic Goals
Goals are strings, not enum variants. Each recipe declares its goal IDs, display names, and metadata. Core code never matches on specific goal strings.

### R2: Recipe-Provided Backend Hints
Each goal in a recipe provides hints that backends interpret: `needs_planning_model`, `temperature`, `max_tokens`, `tool_permissions`, etc. Backends use hints to configure themselves without knowing which workflow they serve.

### R3: Recipe-Provided State Machine
Each recipe defines its state strings and `next_goal_for_state` mapping. Changeset stores whatever state the recipe's hooks produce.

### R4: Recipe-Provided Hooks
Each recipe provides a `RunnerHooks` implementation. The TDD hooks move to `tddy-workflow-recipes`.

### R5: Recipe-Provided Permissions
Per-goal tool/file permission allowlists move from `permission.rs` to each recipe's definition.

### R6: Recipe-Provided Output Parsing
Each recipe defines how to parse LLM output for each goal (what JSON shape to expect, what fields to extract).

### R7: CLI Goal Validation from Recipe
`--goal` accepted values come from the active recipe. No hard-coded `value_parser` array.

### R8: Open-Closed Principle
Adding a new workflow requires only: creating a new recipe in `tddy-workflow-recipes` and registering it. No changes to `tddy-core`, `tddy-coder`, `tddy-service`, etc.

### R9: Rust-First Definition
Workflows are defined as Rust structs/builders. JSON/YAML deserialization is a future concern — the data structures should be serializable but we don't need to implement loading from data files yet.

## Success Criteria

- [ ] TDD workflow works identically after refactoring (all existing tests pass)
- [ ] A second workflow (e.g., minimal bug-fix stub) can be added by only adding code to `tddy-workflow-recipes`
- [ ] `tddy-core` has zero references to TDD-specific goal names or state strings
- [ ] `tddy-coder` CLI derives `--goal` values dynamically from the recipe
- [ ] Backends configure themselves from recipe hints, not goal enum matching
- [ ] All existing integration and E2E tests pass without modification (or with minimal adaptation)

## Impact Analysis

### Technical Impact
- **High** — touches every package in the workspace
- `tddy-core`: Remove `Goal` enum, make `WorkflowEngine` recipe-parameterized, extract hooks
- `tddy-coder`: Dynamic CLI validation, remove goal dispatch chains
- `tddy-service`: Already mostly string-based, minor changes to `status_from_state`
- `tddy-tui`: Minimal impact (already uses `PresenterView` abstraction)
- New `tddy-workflow-recipes`: All TDD-specific code moves here

### User Impact
- **None** — identical CLI behavior, identical TUI, identical gRPC API
- Future benefit: users can select different workflows

## References

- Current workflow graph: `packages/tddy-core/src/workflow/tdd_graph.rs`
- Current hooks: `packages/tddy-core/src/workflow/tdd_hooks.rs`
- Current Goal enum: `packages/tddy-core/src/backend/mod.rs` (lines 178–211)
- Current state machine: `packages/tddy-core/src/changeset.rs` (`next_goal_for_state`)
- Current permissions: `packages/tddy-core/src/permission.rs`
