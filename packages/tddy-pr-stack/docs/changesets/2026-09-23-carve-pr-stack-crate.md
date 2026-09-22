# 2026-09-23 — The crate is created from `tddy-workflow-recipes`' PR-stack data model

**Type:** Refactor · `#carve` 10/11, PR [#496](https://github.com/uppin/tddy-coder/pull/496)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-pr-stack-crate.md`](../../../../docs/dev/changesets/2026-09-23-carve-pr-stack-crate.md)

Created from the non-recipe half of `tddy-workflow-recipes`' `pr_stack` and `orchestrate_pr_stack`:
the stack operations cut out of `pr_stack/mod.rs` (everything after `PrStackRecipe`'s impls, minus
`reseed_stack_from_plan_if_unspawned`), `pr_stack/docs.rs`, and `orchestrate_pr_stack/{assess,
git_ops,pr_insight}.rs`, plus `pr_number_from_status_url` from `orchestrate_pr_stack/bridge.rs`.
Depends on `tddy-core`, `tddy-git`, `tddy-github` and `tddy-workflow`; must never depend on
`tddy-workflow-recipes`, which re-exports it. See [architecture.md](../architecture.md).

Five `pub` modules. The stack operations, **1,524 production lines** as one moved file, are split by
concern under `stack_ops/` — `mod.rs` 147, `order.rs` 156, `nodes.rs` 386, `repoint.rs` 228,
`adopt.rs` 198, `seed.rs` 225, `pull_base.rs` 267 production lines — re-exported flat from
`stack_ops`. `assess.rs` 379, `git_ops.rs` 449, `pr_insight.rs` 305, `docs.rs` 164. Moved bodies are
unchanged apart from path rewrites (`tddy_core::worktree::` → `tddy_git::`,
`orchestrate_pr_stack::github` → `tddy_github::pr_api`); 63 lib unit tests came with the code.

Visibility: `assign_missing_display_order` is `pub` (called from the recipe-side bridge);
`assess` and `git_ops` are `pub mod`; the `unimplemented!` stub `git_ops::build_integration_ref` is
`pub(crate)`; the split added two `pub(super)` helpers, `next_display_order` and
`realign_node_to_effective_base`.

Code issues arriving with the moved code, both open and unclaimed in `docs/code-issues/`:
`complexity-repoint-repoint-planned-pr-node` (**79 lines**, nesting 3, unchanged) and
`complexity-pull-base-pull-base-into-node-branch` (**158 lines**, nesting 4; 159 before a `rustfmt`
rewrap).
