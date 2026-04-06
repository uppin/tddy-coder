# Evaluation Report

## Summary

Code review: GitHub PR tools for tddy-tools (mock-tested client + curl-based MCP REST), merge-pr/tdd-small prompt awareness, and optional changeset-workflow `github_pr_tools_metadata`. `cargo check` on tddy-tools and tddy-workflow-recipes passes. Main risks: runtime dependency on `curl` and PATH for MCP live calls; duplicated token/header patterns vs merge_pr/github.rs; two untracked artifact files should be gitignored or deleted before commit. Overall alignment with PRD is strong: auth gating, REST headers, MCP registration, schema extension, and acceptance tests are in place.

## Risk Level

medium

## Changed Files

- packages/tddy-tools/Cargo.toml (modified, +1/−1)
- packages/tddy-tools/src/lib.rs (modified, +1/−0)
- packages/tddy-tools/src/server.rs (modified, +119/−1)
- packages/tddy-tools/src/github_pr.rs (added, +493/−0)
- packages/tddy-tools/tests/github_pr_acceptance.rs (added, +177/−0)
- packages/tddy-tools/tests/persist_changeset_github_tools_acceptance.rs (added, +24/−0)
- packages/tddy-workflow-recipes/generated/tdd/changeset-workflow.schema.json (modified, +11/−1)
- packages/tddy-workflow-recipes/src/lib.rs (modified, +3/−1)
- packages/tddy-workflow-recipes/src/merge_pr/hooks.rs (modified, +47/−1)
- packages/tddy-workflow-recipes/src/merge_pr/mod.rs (modified, +1/−1)
- packages/tddy-workflow-recipes/src/tdd_small/mod.rs (modified, +1/−1)
- packages/tddy-workflow-recipes/src/tdd_small/red.rs (modified, +25/−3)
- packages/tddy-workflow-recipes/tests/github_tools_recipe_prompt_acceptance.rs (added, +47/−0)

## Validity Assessment

The diff substantively implements the PRD: token-gated GitHub PR create/update (mock layer for CI), documented MCP tool names, REST headers consistent with GitHub API, merge-pr prompts augmented when `GITHUB_TOKEN`/`GH_TOKEN` is set, tdd-small merged red prompt includes GitHub/tddy-tools PR guidance, and changeset-workflow schema accepts optional routing metadata without breaking existing fields.

## Issues (highlights)

- Duplicated curl/header logic vs merge_pr/github.rs
- MCP tool list not introspected end-to-end in tests
- tdd-small prompt gating vs merge-pr env check nuance
