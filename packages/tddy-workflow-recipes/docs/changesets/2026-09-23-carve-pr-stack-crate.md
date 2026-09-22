# 2026-09-23 — The PR-stack data model moves to `tddy-pr-stack`

**Type:** Refactor · `#carve` 10/11, PR [#496](https://github.com/uppin/tddy-coder/pull/496)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-pr-stack-crate.md`](../../../../docs/dev/changesets/2026-09-23-carve-pr-stack-crate.md)

The stack operations, `pr_stack/docs.rs` and `orchestrate_pr_stack/{assess,git_ops,pr_insight}.rs`
move to [`tddy-pr-stack`](../../../tddy-pr-stack/docs/architecture.md); this crate gains that
dependency and re-exports every item from its historical path — `pr_stack` glob re-exports
`tddy_pr_stack::stack_ops` and `docs`; `orchestrate_pr_stack` re-exports `assess` (private),
`git_ops` (`pub(crate)`) and `pr_insight` (`pub`); `bridge.rs` re-exports
`pr_number_from_status_url`. No consumer crate is edited. `assign_missing_display_order` is
additionally reachable as `pr_stack::assign_missing_display_order`, being `pub` in its new home.

Stays here: `PrStackRecipe` and its `WorkflowRecipe` / `SessionArtifactManifest` impls (so
`writer::EXPLORATION_BASENAME` never crosses the seam), `reseed_stack_from_plan_if_unspawned` (names
`plan_pr_stack`, which names `pr_stack` back), `pr_stack/{hooks,bridge}.rs`,
`orchestrate_pr_stack/actions.rs` (its tasks call `bridge::execute_stack_{merge,repoint}`, and
`bridge.rs` names `plan_pr_stack`), the rest of `orchestrate_pr_stack/`, and `plan_pr_stack/`.

`tests/pr_stack_crate_shape.rs` pins the seam, 5/5: three acceptance criteria on the new crate's
dependencies and sources, and two guards that `PrStackRecipe` and
`reseed_stack_from_plan_if_unspawned` stay here.

`docs/code-issues/squatting-pr-stack-data-model` is **partially fixed** and its claim released:
**4,903 lines** now in `tddy-pr-stack` (incl. tests), **3,210** left in `pr_stack/` +
`orchestrate_pr_stack/`; the remainder is **45** references in `tddy-coder`, `tddy-tools`,
`tddy-session-files` and `tddy-session-lifecycle` still naming the re-exported paths. The two
`complexity-mod-*` records for `repoint_planned_pr_node` (79 lines) and
`pull_base_into_node_branch` (158) moved with their code to `tddy-pr-stack`.
