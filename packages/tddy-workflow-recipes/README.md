# tddy-workflow-recipes

Workflow recipes for `tddy-coder`: the goal graphs, their lifecycle hooks, and the parsers that turn
an agent's structured output into typed Rust.

## Module layout

Two areas are split by responsibility rather than held in one file, so a change to one phase is read
against that phase alone. **No public path changed at either seam**, by two different routes: the
parser parent re-exports what it split out, so every caller's path still resolves; the hooks parents
split out functions that were private to begin with, so there was no path to keep.

### `parser/` — one module per goal phase

`parser.rs` keeps only what every phase shares: the `ParseError` binding, the goals parsed by no
single phase (`validate`, `demo`, `refactor`, `update-docs`), and a glob facade so every existing
`tddy_workflow_recipes::parser::…` path goes on resolving.

| Module | Owns |
|---|---|
| `parser/planning.rs` | `PlanningOutput`, `DemoPlan`, `DemoStep`, `PortMap`, `DemoMode`, `parse_planning_response[_with_base]` |
| `parser/acceptance_tests.rs` | `AcceptanceTestsOutput`, `AcceptanceTestInfo`, `parse_acceptance_tests_response` |
| `parser/analyze.rs` | `AnalyzeOutput`, `parse_analyze_response` (bugfix pipeline) |
| `parser/green.rs` | `GreenOutput`, `DemoOutput`, `DemoResults`, `GreenTestResult`, `ImplementationInfo`, `parse_green_response` |
| `parser/red.rs` | `RedOutput`, `MarkerInfo`, `MarkerResult`, `RedTestInfo`, `SkeletonInfo`, `parse_red_response`, `validate_red_marker_source_paths` |
| `parser/evaluate.rs` | the `Evaluate*` report types and `parse_evaluate_response` |

Each phase owns its output struct, its private `Structured…` mirror and its `…De` deserialization
mirrors. Nothing crosses a seam but `ParseError`.

### `tdd/hooks/` and `tdd_small/hooks/` — split by lifecycle half

Each hooks parent keeps its struct, that struct's inherent impl, and `impl RunnerHooks` — the only
caller of the phase functions. The phase functions themselves live in the half they belong to, and
the parent reaches them by path (`before::before_red(…)`), which is why it declares
`mod before; mod after;` and re-exports neither: nothing outside the parent ever called them.

| Module | Owns |
|---|---|
| `tdd/hooks/before.rs` | the `before_*` phase functions of the full TDD workflow |
| `tdd/hooks/after.rs` | its `after_*` phase functions |
| `tdd_small/hooks/before.rs` | the `before_*` phase functions of the small TDD workflow |
| `tdd_small/hooks/after.rs` | its `after_*` phase functions |

## Other modules

`schema.rs`, `schema_manifest.rs`, `schema_pipeline.rs` (goal output schemas), `writer.rs`
(artifact I/O), `permissions.rs` and `approval_policy.rs` (per-goal tool allowlists),
`session_artifact_manifest.rs`, `recipe_resolve.rs`, and one module per
recipe family: `tdd`, `tdd_small`, `bugfix`, `grill_me`, `review`, `free_prompting`, `merge_pr`,
`plan_pr_stack`, `pr_stack`, `orchestrate_pr_stack`, `feature_start_slash`.

## The GitHub REST client lives in `tddy-github`

`github_rest_common.rs`, `github_pr.rs` and `orchestrate_pr_stack/github.rs` moved to
[`tddy-github`](../tddy-github/) — `tddy_github::github_rest_common`, `tddy_github::github_pr` and
`tddy_github::pr_api`. A REST client belongs in the crate named after the service, not in a
workflow-recipes crate.

The three old paths are kept here as one-line `pub use` facades, so
`tddy_workflow_recipes::github_pr::…` and `crate::orchestrate_pr_stack::github::…` keep resolving and
no caller was edited. Write new code against `tddy_github` directly.

## The PR-stack data model lives in `tddy-pr-stack`

The stack operations that were the second half of `pr_stack/mod.rs` (every writer of
`Changeset.stack`, and the node↔branch↔pull-request syncs), `pr_stack/docs.rs`, and
`orchestrate_pr_stack/{assess,git_ops,pr_insight}.rs` moved to [`tddy-pr-stack`](../tddy-pr-stack/).
None of it is a recipe. Consumers still reach it through the facades below, and so still compile
every recipe, until they are re-pointed at `tddy_pr_stack` — see
[`squatting-pr-stack-data-model`](docs/code-issues/squatting-pr-stack-data-model.md).

What stayed is recipe-side by nature:

- `PrStackRecipe`, its `WorkflowRecipe` and `SessionArtifactManifest` impls, `pr_stack/{hooks,bridge}.rs`.
- `reseed_stack_from_plan_if_unspawned` — it names `plan_pr_stack`, which is mutually referenced with
  `pr_stack` and cannot leave.
- `orchestrate_pr_stack/bridge.rs` (`seed_orchestrator_stack_from_plan` names `plan_pr_stack`) and so
  `orchestrate_pr_stack/actions.rs`, whose tasks call the bridge's merge and repoint.

Every old path still resolves: `pr_stack` glob re-exports `tddy_pr_stack::stack_ops` and re-exports
`docs`; `orchestrate_pr_stack` re-exports `assess`, `git_ops` and `pr_insight` at their old
visibilities; `orchestrate_pr_stack::bridge::pr_number_from_status_url` re-exports the function from
`tddy_pr_stack::pr_insight`. No caller was edited. Write new code against `tddy_pr_stack` directly.
