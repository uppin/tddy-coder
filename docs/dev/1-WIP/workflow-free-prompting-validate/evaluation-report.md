# Evaluation Report

## Summary

Review of workflow free-prompting + approval policy work: new recipe registration, policy module, presenter bootstrap writes recipe to changeset, CLI/TUI allowlists, presenter and workflow-recipes tests, and grpc e2e UTF-8 preview fix. cargo check on affected crates passed. Gaps: product docs (workflow-recipes.md) not updated; untracked red test output file should not be committed; approval_policy list must stay in sync with recipe_resolve.

## Risk Level

medium

## Changed Files

- packages/tddy-coder/src/run.rs (modified, +4/−4)
- packages/tddy-coder/tests/cli_args.rs (modified, +3/−3)
- packages/tddy-coder/tests/presenter_integration.rs (modified, +189/−6)
- packages/tddy-core/src/backend/mod.rs (modified, +6/−1)
- packages/tddy-core/src/presenter/workflow_runner.rs (modified, +6/−0)
- packages/tddy-e2e/tests/grpc_terminal_rpc.rs (modified, +9/−4)
- packages/tddy-integration-tests/tests/green_new_agent_session_contract.rs (modified, +4/−12)
- packages/tddy-workflow-recipes/src/lib.rs (modified, +3/−0)
- packages/tddy-workflow-recipes/src/recipe_resolve.rs (modified, +30/−3)
- packages/tddy-workflow-recipes/src/approval_policy.rs (added, +22/−0)
- packages/tddy-workflow-recipes/src/free_prompting/mod.rs (added, +138/−0)
- packages/tddy-workflow-recipes/src/free_prompting/hooks.rs (added, +50/−0)
- packages/tddy-workflow-recipes/tests/recipe_policy_red.rs (added, +46/−0)
- packages/tddy-workflow-recipes/tests/workflow_recipe_acceptance.rs (added, +45/−0)
- .tddy-workflow-recipes-red-test-output.txt (added, +243/−0)

## Validity Assessment

The changes substantially address the PRD green-scope: F1 free-prompting is registered with Prompting/prompting metadata and a minimal graph; F2/F3 are partially met via approval_policy helpers, recipe.uses_primary_session_document=false for free-prompting, and workflow_runner persisting recipe on bootstrap so TDD/bugfix parity tests pass; F4 bugfix presenter path is fixed by recording recipe in changeset. F5 is met in code (resolver, CLI, TUI recipe question). Remaining gaps: A4 documentation not in diff; DemoArgs goal list may need prompting for tddy-demo.
