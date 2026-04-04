# Evaluation Report

## Summary

Bugfix recipe gains an analyze→reproduce→end graph with embedded analyze schema, hooks that seed changeset and persist submit output, StubBackend analyze JSON, and broad test updates. cargo check --workspace passed. Minor gaps: optional analyze summary not merged into reproduce prompt; docs/dev/1-WIP product doc update still pending; untracked .red-phase-submit.json should not ship.

## Risk Level

medium

## Changed Files

- packages/tddy-coder/tests/cli_recipe.rs (modified, +14/−2)
- packages/tddy-core/src/backend/stub.rs (modified, +10/−0)
- packages/tddy-integration-tests/tests/common/mod.rs (modified, +36/−1)
- packages/tddy-integration-tests/tests/workflow_graph.rs (modified, +29/−1)
- packages/tddy-tools/tests/cli_integration.rs (modified, +2/−0)
- packages/tddy-tools/tests/schema_validation_tests.rs (modified, +11/−0)
- packages/tddy-workflow-recipes/generated/proto_basenames.rs (modified, +1/−0)
- packages/tddy-workflow-recipes/generated/schema-manifest.json (modified, +5/−0)
- packages/tddy-workflow-recipes/goals.json (modified, +1/−0)
- packages/tddy-workflow-recipes/src/bugfix/hooks.rs (modified, +61/−4)
- packages/tddy-workflow-recipes/src/bugfix/mod.rs (modified, +74/−9)
- packages/tddy-workflow-recipes/src/lib.rs (modified, +10/−9)
- packages/tddy-workflow-recipes/src/parser.rs (modified, +46/−0)
- .red-phase-submit.json (added, +142/−0)
- packages/tddy-workflow-recipes/generated/tdd/analyze.schema.json (added, +14/−0)
- packages/tddy-workflow-recipes/proto/analyze.proto (added, +10/−0)
- packages/tddy-workflow-recipes/src/bugfix/analyze.rs (added, +90/−0)
- packages/tddy-workflow-recipes/tests/bugfix_analyze_persistence.rs (added, +39/−0)

## Affected Tests

- packages/tddy-coder/tests/cli_recipe.rs: updated — cli_recipe_bugfix_start_goal_is_analyze and status assertions
- packages/tddy-tools/tests/cli_integration.rs: updated — REGISTERED_GOALS + analyze URN; list-schemas parity
- packages/tddy-tools/tests/schema_validation_tests.rs: updated — analyze_goal_schema_embedded
- packages/tddy-integration-tests/tests/workflow_graph.rs: updated — stub_or_mock_backend_analyze_submit_valid
- packages/tddy-integration-tests/tests/common/mod.rs: updated — bugfix_stub_invoke_request / recipe helpers for analyze
- packages/tddy-workflow-recipes/tests/bugfix_analyze_persistence.rs: created — bugfix_analyze_persists_branch_and_worktree
- packages/tddy-workflow-recipes/src/bugfix/mod.rs: updated — unit tests bugfix_graph_orders_* , bugfix_recipe_is_valid_plugin

## Validity Assessment

The diff substantively implements the PRD: analyze is first in the graph and start_goal; analyze schema is registered in goals.json and consumed by tddy-tools; goal_requires_tddy_tools_submit is true for analyze only; hooks persist branch_suggestion/worktree_suggestion/name and transition state to Reproducing; StubBackend emits valid analyze submit JSON; CLI and integration tests cover resolver, schema, persistence, and stub path. Remaining gaps are non-blocking: optional summary→reproduce merge, product documentation via docs/dev/1-WIP, and cleanup of .red-phase-submit.json.

## Build Results

- workspace: pass (./dev cargo check --workspace finished dev profile in ~10m; exit 0)

## Issues

- [info/prd_gap] Optional `summary` from analyze JSON not merged into reproduce prompt.
- [low/hygiene] .red-phase-submit.json untracked artifact.
- [low/workflow] `next_goal_for_state` broad default to analyze.
- [info/documentation] docs/dev/1-WIP product doc update pending.
