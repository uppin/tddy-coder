# 2026-09-23 — The PR-stack data model becomes `tddy-pr-stack`

**Type:** Refactor

`#carve` 10/11, PR [#496](https://github.com/uppin/tddy-coder/pull/496). Builds on `#carve`
presenter-split ([#495](https://github.com/uppin/tddy-coder/pull/495)) and consumes four earlier
nodes: nested anchors and cluster moves in the restructure tooling, `changeset/stack.rs` as its own
module (#491), and `tddy-git` plus the GitHub REST client in `tddy-github` (#492). Dependent:
[#520](https://github.com/uppin/tddy-coder/pull/520), the stack tip.

`pr_stack` and `orchestrate_pr_stack` were the most-referenced external surfaces of
`tddy-workflow-recipes` — 79 of ~190 cross-crate references — and neither is a recipe. The data
model moves to a new crate, `packages/tddy-pr-stack`; the recipe, its hooks and the plan→stack
bridges stay, behind re-exports at every historical path. Behaviour-preserving: no signature
changes, no consumer edited.

The crate's layout, dependencies, seam tests and the recipe-side remainder are documented in
[`packages/tddy-pr-stack/docs/architecture.md`](../../../packages/tddy-pr-stack/docs/architecture.md);
product behaviour is unchanged in [pr-stacking.md](../../ft/coder/pr-stacking.md) and
[pr-stack-docs.md](../../ft/coder/pr-stack-docs.md), whose code locations now name
`tddy-pr-stack`.

## What moved

| From (`tddy-workflow-recipes/src/`) | To (`tddy-pr-stack/src/`) | How |
|---|---|---|
| `pr_stack/mod.rs` — the stack operations after `PrStackRecipe`'s impls, and their unit tests | `stack_ops/` | cut by line range; `reseed_stack_from_plan_if_unspawned` stayed |
| `pr_stack/docs.rs` | `docs.rs` | `git mv`, byte-identical |
| `orchestrate_pr_stack/{assess,git_ops,pr_insight}.rs` | same names | `git mv`; `super::github` → `tddy_github::pr_api` |
| `pr_number_from_status_url`, from `orchestrate_pr_stack/bridge.rs` | `pr_insight.rs` | moved with its callers; `bridge.rs` re-exports it |

Path rewrites inside moved code only: `crate::orchestrate_pr_stack::{git_ops,pr_insight}` →
`crate::{git_ops,pr_insight}`, and `tddy_core::worktree::{detect_default_remote_name,
worktree_path_for_branch, local_branch_name_for_remote, checked_out_branch_name}` → `tddy_git::…`
(the same functions; `tddy_core::worktree` glob re-exports `tddy_git`).

**`stack_ops` split at `/pr-wrap`.** Moved whole, `stack_ops.rs` was **1,524 production lines**. It
is split by concern into seven files, measured in production lines (before the first
`#[cfg(test)]`): `mod.rs` 147, `order.rs` 156, `nodes.rs` **386** (largest), `repoint.rs` 228,
`adopt.rs` 198, `seed.rs` 225, `pull_base.rs` 267 — every file under the 500-line budget. The rest
of the crate: `assess.rs` 379, `git_ops.rs` 449, `pr_insight.rs` 305, `docs.rs` 164, `lib.rs` 12.

**Mechanism.** No `tddy-tools restructure` step was used: `move_module_to_crate` refuses whenever a
facade is left behind, which every move here required. Whole files moved with `git mv` (rename
detected, blame kept); the `pr_stack/mod.rs` and `stack_ops` splits were cut by line range; every
old path is a hand-written re-export. Identity of the moved code was proven by a line-multiset diff
against the pre-move files.

## What stayed, and why

- **`PrStackRecipe`**, its `WorkflowRecipe` and `SessionArtifactManifest` impls, and the state
  constants — the recipe. Moving it would make the new crate depend on the workflow machinery.
- **`reseed_stack_from_plan_if_unspawned`** — the only stack operation that calls `plan_pr_stack`
  (`validate_stack_plan`, `planned_prs_into_stack_nodes`), and `plan_pr_stack/{mod,hooks}.rs` name
  `pr_stack` back. Mutual, so it cannot come; it is a plan→stack bridge, recipe-side by nature.
- **`orchestrate_pr_stack/actions.rs`** — planned to move, but not zero-dependency:
  `MergeTask` / `RepointTask` call `bridge::{execute_stack_merge, execute_stack_repoint}`, and
  `bridge.rs` also holds `seed_orchestrator_stack_from_plan`, which names `plan_pr_stack`. Moving it
  would mean splitting `bridge.rs` and moving `transient.rs` too.
- **`pr_stack/{hooks,bridge}.rs`**, the rest of `orchestrate_pr_stack/`, and `plan_pr_stack/`.

## Deviations from the plan

1. **`EXPLORATION_BASENAME` never crossed the seam and stayed in `writer.rs`.** Both references are
   inside `PrStackRecipe`'s `SessionArtifactManifest` impl, which stays; nothing in the moving set
   names it, so `writer.rs` is untouched.
2. **`actions.rs` stayed** (above). The moved set is `docs.rs` plus three `orchestrate_pr_stack`
   modules, not four.
3. **`pr_insight.rs` was not zero-dependency** — it called `bridge::pr_number_from_status_url`. That
   pure function moved with it.
4. **`tddy-pr-stack` also depends on `tddy-workflow`**, an internal crate recipes already used:
   `docs::node_doc_paths` calls `tddy_workflow::session_artifacts_root`. No external dependency was
   added.

## Visibility

- `assign_missing_display_order` `pub(crate)` → `pub`: `bridge.rs`'s
  `seed_orchestrator_stack_from_plan` calls it across the crate boundary. The `stack_ops::*` glob
  therefore also exposes it as `tddy_workflow_recipes::pr_stack::assign_missing_display_order`.
- At crate level `assess` (private) and `git_ops` (`pub(crate)`) are `pub mod` in `tddy-pr-stack` —
  unavoidable across a crate boundary. The recipes re-exports keep their historical visibilities
  (`use`, `pub(crate) use`).
- The `unimplemented!` stub `git_ops::build_integration_ref` is narrowed to `pub(crate)` so the
  widening does not publish it.
- Two helpers introduced by the `stack_ops` split are `pub(super)`: `next_display_order`
  (`order.rs`) and `realign_node_to_effective_base` (`repoint.rs`).

## Acceptance tests

`packages/tddy-workflow-recipes/tests/pr_stack_crate_shape.rs`, **5/5**: three acceptance criteria
(the crate depends on `tddy-core` / `tddy-git` / `tddy-github`; not on `tddy-workflow-recipes`; no
source at any depth names `plan_pr_stack`, `::writer` or `::parser`) and two guards that passed
before the move and must keep passing (`PrStackRecipe` and `reseed_stack_from_plan_if_unspawned`
stay recipe-side). `tddy-pr-stack` carries **63** lib unit tests; per-file counts are unchanged by
the move (stack operations 23 + recipe 21 = the original 44; `assess` 14, `git_ops` 7,
`pr_insight` 2, `docs` 12).

## Validation

- Scoped: `cargo clippy -p tddy-pr-stack -p tddy-workflow-recipes --all-targets -- -D warnings`
  clean; `./test -p tddy-pr-stack -p tddy-workflow-recipes` 543 passed, 0 failed (with
  `TMPDIR=/private/tmp` — `pr_stack_artifact_paths_acceptance::a_plan_left_at_the_legacy_session_root_is_still_advertised_to_the_agent`
  fails under `./dev` on macOS because the nix shell's `TMPDIR` is not canonical; pre-existing,
  untouched). Whole-workspace health: CI on #496.
- `/validate-changes`, `/validate-tests`, `/validate-prod-ready`: should-fix items applied —
  the vacuous `tddy-git` manifest assertion (satisfied by `tddy-github`) tightened, the unused
  `serde_json` and `pretty_assertions` dependencies dropped, the broken intra-doc link to the
  recipe-side `reseed_stack_from_plan_if_unspawned` removed, the stub narrowed.

## Code issues reconciled

| Record | Outcome | Final measurement |
|---|---|---|
| `tddy-workflow-recipes` · `squatting-pr-stack-data-model` | **partially fixed**, claim released | ~4,280 lines in recipes → **4,903 lines in `tddy-pr-stack`** (incl. tests); **3,210** left in `pr_stack/` + `orchestrate_pr_stack/` (recipe, hooks, bridges, facades). Remainder: **45** references to `tddy_workflow_recipes::{pr_stack,orchestrate_pr_stack}` in `tddy-coder`, `tddy-tools`, `tddy-session-files` and `tddy-session-lifecycle` still to re-point at `tddy_pr_stack`, which is what stops those crates compiling every recipe |
| `complexity-mod-repoint-planned-pr-node` | **moved**, renamed `tddy-pr-stack` · `complexity-repoint-repoint-planned-pr-node` | **79 lines**, nesting 3 — unchanged |
| `complexity-mod-pull-base-into-node-branch` | **moved**, renamed `tddy-pr-stack` · `complexity-pull-base-pull-base-into-node-branch` | 159 → **158 lines** (a `rustfmt` rewrap of a shortened path), nesting 4 |

No `docs/dev/todo/` entry was resolved. Four sat in the moved code's path and all four are
behaviour, which a behaviour-preserving move must not touch:
`2026-07-30-pr-stack-full-control-follow-ups`,
`2026-08-13-pr-stack-an-externally-located-worktree-is-refused-as-a-stack-base` and
`2026-08-13-pr-stack-seeding-a-stack-from-several-existing-sessions` (each must behave identically
after the move; unchanged), and
`2026-08-29-stack-progress-json-is-documented-as-a-host-guarantee-but-nothing-writ` (answered:
nothing writes it, and this change does not add it).
