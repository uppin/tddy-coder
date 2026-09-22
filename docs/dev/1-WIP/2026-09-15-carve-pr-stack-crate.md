# Changeset: carve-pr-stack-crate

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Refactor
**Stack**: `#carve` 9/9 — the stack tip
**PR**: [#496](https://github.com/uppin/tddy-coder/pull/496)

PRD: [`2026-09-15-carve-pr-stack-crate-prd.md`](./2026-09-15-carve-pr-stack-crate-prd.md)

## Initial Discovery

[`2026-09-15-carve-pr-stack-crate-initial-discovery.md`](./2026-09-15-carve-pr-stack-crate-initial-discovery.md)

## Affected Packages

- **`tddy-pr-stack`** (new): the PR-stack data model and its git/GitHub operations, ~4,230 lines.
- **`tddy-workflow-recipes`**: [README.md](../../../packages/tddy-workflow-recipes/README.md) —
  keeps `PrStackRecipe`, the hooks and bridges, and facades at every old path.

## Responsibility

- Create `tddy-pr-stack` and move the stack operations, `pr_stack/docs.rs`, and the four
  zero-dependency `orchestrate_pr_stack` modules into it.
- Leave `PrStackRecipe`, its two impls, all hooks and bridges, `reseed_stack_from_plan_if_unspawned`
  and `plan_pr_stack/` in `tddy-workflow-recipes`, behind facades.
- Resolve the `EXPLORATION_BASENAME` const so the moving set names nothing recipe-side.

## Boundaries

- Does **not** move `plan_pr_stack/` — mutually referenced with `pr_stack` and recipe-side by nature.
- Does **not** move `writer.rs` or `parser/`; only the one const is resolved.
- Does **not** change stack operations, GitHub sync, or base resolution behaviour.
- Does **not** edit any of the 79 external reference sites.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `1/9` restructure-moves | nested anchors; facade-aware cycle refusal | `pr_stack/` and `orchestrate_pr_stack/` are **both directories**; the move leaves facades | touch `tddy-code-restructuring` |
| `3/9` restructure-clusters | **multi-module cluster moves** | `pr_stack ↔ orchestrate_pr_stack` is **mutual** — `pr_stack/mod.rs` names `orchestrate_pr_stack::{git_ops,github,pr_insight}`; `orchestrate_pr_stack/bridge.rs` names `crate::pr_stack::assign_missing_display_order` | rely on leaf-first ordering |
| `4/9` core-foundations | `changeset/stack.rs` as its own module | `Stack` ×15, `read_changeset` ×7, `update_stack_atomic` ×3 | edit `changeset/` |
| `5/9` git-plumbing | `tddy-git`; GitHub REST already in `tddy-github` | four `tddy-git` helpers (`detect_default_remote_name`, `worktree_path_for_branch`, `local_branch_name_for_remote`, `checked_out_branch_name`) and the PR REST client | move the GitHub client itself — 5/9 already did |

## Draft PR contract

Published first:

**Published** (commit 2): `tests/pr_stack_crate_shape.rs` — five assertions, three of which pin
things that must **not** change. The stack operations keep their signatures through the move, so
there is no new surface to declare; what lands first is the seam.

`the_recipe_stays_with_the_recipes` and `the_plan_to_stack_bridge_stays_behind` **pass now and must
stay passing**. They are the guards on the two measured cuts: moving `PrStackRecipe` would make the
new crate depend on the workflow machinery it exists to be independent of, and moving
`reseed_stack_from_plan_if_unspawned` would drag `plan_pr_stack`, which is mutually referenced with
`pr_stack` and cannot come.

This PR goes on to implement all of it. **It must not merge in that state.**

## Green wave

**Wave:** 4 of 4 — the only node in its wave
**Greenable independently:** **no** — it consumes four predecessors' behaviour, more than any other
node: nested anchors (1/9), cluster moves (3/9), `changeset/stack.rs` (4/9), `tddy-git` and the moved
GitHub client (5/9)
**Concurrent with:** nothing
**Blocks:** nothing — this is the stack tip

Real dependency edges, as refined by `#carve` 6/9's discovery:

    n1 → n3, n4, n5, n6, n9      n3 → n7, n9      n4 → n6, n8, n9      n5 → n9

## Prerequisites

| Entry | Verdict | What this node does with it |
|---|---|---|
| [2026-07-30-pr-stack-full-control-follow-ups.md](../todo/2026-07-30-pr-stack-full-control-follow-ups.md) | ⚠ **DURING** | Open follow-ups on the PR-stack surface this node relocates. Every one must behave identically after the move. Not claimed — none is fixed here. |
| [2026-08-13-pr-stack-an-externally-located-worktree-is-refused-as-a-stack-base.md](../todo/2026-08-13-pr-stack-an-externally-located-worktree-is-refused-as-a-stack-base.md) | ⚠ **DURING** | The refusal lives in code this node moves (and in `tddy-git`, from 5/9). It must survive intact. Not claimed. |
| [2026-08-13-pr-stack-seeding-a-stack-from-several-existing-sessions.md](../todo/2026-08-13-pr-stack-seeding-a-stack-from-several-existing-sessions.md) | ⚠ **DURING** | Concerns `seed_stack_with_base_session` / `check_stack_seed_base`, both moving. Not claimed. |
| [2026-08-29-stack-progress-json-is-documented-as-a-host-guarantee-but-nothing-writ.md](../todo/2026-08-29-stack-progress-json-is-documented-as-a-host-guarantee-but-nothing-writ.md) | ℹ **Answered** | Confirms nothing writes it; this node does not add it. Not claimed. |

**This node claims nothing.** Four entries sit in its path and all four are behaviour, which a
behaviour-preserving move must not touch. The right time to close them is after the crate exists.

## State A → State B

### State A

- `pr_stack` + `orchestrate_pr_stack` are **79 of ~190** cross-crate references to
  `tddy-workflow-recipes`. Twelve crates pull in every recipe to reach them.
- `pr_stack/mod.rs` — `PrStackRecipe` and its two impls at **131–418**; stack operations at
  **418–1958**, with one reference each to `workflow::{task,recipe,ids,hooks,graph}` and `backend`,
  all inside the recipe impl.
- Zero-`crate::`-dependency modules: `orchestrate_pr_stack/{assess,git_ops,pr_insight,actions}.rs`
  (2,286 lines) and `pr_stack/docs.rs` (447).
- Two edges cross the seam: `reseed_stack_from_plan_if_unspawned` → `plan_pr_stack` (2 call sites),
  and `crate::writer::EXPLORATION_BASENAME` (2 references to one `&str` const).
- `plan_pr_stack/{mod,hooks}.rs` both name `crate::pr_stack` — mutual with it.

### State B

- `tddy-pr-stack` holds ~4,230 lines and names nothing recipe-side.
- `tddy-workflow-recipes` keeps the recipe and the bridges, and every old path resolves.

## Implementation phases

| Phase | Kind | Work |
|---|---|---|
| **A** | mechanical | `extract_module --to_file` on `pr_stack/mod.rs`: lift the stack operations (418–1958) **minus `reseed_stack_from_plan_if_unspawned`** into a flat module, leaving `PrStackRecipe` and its impls behind |
| **B** | manual | `tddy-pr-stack`'s skeleton — `Cargo.toml`, `lib.rs`, workspace `members`. Resolve `EXPLORATION_BASENAME`: move the const and re-export from `writer.rs`, or duplicate with a comment naming the other definition |
| **C** | mechanical | `move_module_to_crate` as a **cluster**: the flat stack-ops module, `pr_stack/docs.rs`, and the four zero-dependency `orchestrate_pr_stack` modules, `reexport: "glob"`. Requires `#carve` 3/9 |
| **D** | manual | `Cargo.toml` dependency edges; facade tidy-up; `README.md` × 2 |

Phase A must run before B and C, and this is the node where that order is load-bearing: the stack
operations and the recipe impl are in **one file**, so there is no module to move until the seam at
line 418 has been cut.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [x] Publish the draft-PR contract (`tddy-pr-stack` surface + failing tests)
- [x] Failing acceptance tests — **USER REVIEW** (approved 2026-09-15, gates delegated)
  - `tests/pr_stack_crate_shape.rs` — 3 failing (the crate does not exist, and nothing has moved);
    **2 passing guards** on the cuts that must hold.
- [x] Failing unit/integration tests — the same suite; the seam, not behaviour, is what this node is about
- [x] Implement production code making tests pass (`/green`) — see [Green notes](#green-notes)
- [x] `/validate-changes` — no blockers; should-fix items applied in `a79d09e5` (see Validation Results)
- [ ] `/pr-wrap` — correct the title, ready for review. Steps 0–6 and 7.5 done 2026-09-22 (validation fixes, `stack_ops/` split under the 500-line budget, code issues reconciled). **Wrap + ready blocked on stack order**: #494 and #495 are still drafts and must ready/wrap first. Backlog-delta sweep belongs to the stack tip, which is now #520, not this node
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Green notes

**Mechanism.** No `tddy-tools restructure` step was used: whole files moved with `git mv` (rename
detected, blame kept), the `pr_stack/mod.rs` split was cut by line range, and every old path is a
hand-written re-export. Phase order A → B → C → D held.

**What moved** (`tddy-pr-stack`, 4,244 lines including tests):

| From (`tddy-workflow-recipes/src/`) | To (`tddy-pr-stack/src/`) | How |
|---|---|---|
| `pr_stack/mod.rs` 454–1967 (production) and 2504–3198 (their unit tests) | `stack_ops.rs` | split by line range; `reseed_stack_from_plan_if_unspawned` (419–452) stayed |
| `pr_stack/docs.rs` | `docs.rs` | `git mv`, byte-identical |
| `orchestrate_pr_stack/{assess,git_ops,pr_insight}.rs` | same names | `git mv`, `super::github`/`crate::orchestrate_pr_stack::github` → `tddy_github::pr_api` |
| `pr_number_from_status_url`, from `orchestrate_pr_stack/bridge.rs` | `pr_insight.rs` | cut and pasted; `pr_insight` and `stack_ops` both call it, and `bridge.rs` cannot move |

**Facades.** `pr_stack/mod.rs`: `pub use tddy_pr_stack::docs;` and `pub use tddy_pr_stack::stack_ops::*;`.
`orchestrate_pr_stack/mod.rs`: `use tddy_pr_stack::assess;`, `pub(crate) use tddy_pr_stack::git_ops;`,
`pub use tddy_pr_stack::pr_insight;` — each at its old visibility. `bridge.rs`:
`pub use tddy_pr_stack::pr_insight::pr_number_from_status_url;`. No external reference site edited.

**Visibility widening — one.** `assign_missing_display_order` was `pub(crate)`; `bridge.rs`'s
`seed_orchestrator_stack_from_plan` calls it across the new crate boundary, so it is `pub`, and the
`stack_ops::*` glob now exposes it as `tddy_workflow_recipes::pr_stack::assign_missing_display_order`.

**Path rewrites inside moved code.** `crate::orchestrate_pr_stack::{git_ops,pr_insight}` →
`crate::{git_ops,pr_insight}`; `crate::orchestrate_pr_stack::github` → `tddy_github::pr_api`;
`tddy_core::worktree::{detect_default_remote_name, worktree_path_for_branch,
local_branch_name_for_remote, checked_out_branch_name}` → `tddy_git::…` (the same functions —
`tddy_core::worktree` glob re-exports `tddy_git`). One doc comment on `AddPlannedPrInput::child_recipe`
linked `crate::plan_pr_stack::{PlannedPr, planned_prs_into_stack_nodes}`; those links cannot resolve
from the new crate, so it names them in prose instead.

**Deviations from this changeset's premises.**

1. **`EXPLORATION_BASENAME` did not cross the seam and was not moved.** Both references (old lines
   355 and 376) are in `PrStackRecipe`'s `SessionArtifactManifest` impl, which stays. Nothing in the
   moving set names it, so `writer.rs` is untouched.
2. **`orchestrate_pr_stack/actions.rs` stayed.** It is not zero-dependency: `MergeTask`/`RepointTask`
   call `super::bridge::{execute_stack_merge, execute_stack_repoint}`, and `bridge.rs` also holds
   `seed_orchestrator_stack_from_plan`, which names `plan_pr_stack`. Moving `actions.rs` would mean
   splitting `bridge.rs` and moving `transient.rs` too — beyond this node's scope.
3. **`pr_insight.rs` was not zero-dependency either** — it called `super::bridge::pr_number_from_status_url`.
   That pure function moved with it (above).
4. **`tddy-pr-stack` also depends on `tddy-workflow`** (internal, already a recipes dependency):
   `docs::node_doc_paths` calls `tddy_workflow::session_artifacts_root`. No external dependency was
   added; `log`, `async-trait`, `serde`, `serde_json` and the dev-dependencies `tempfile`, `rstest`,
   `pretty_assertions` are copied from `tddy-workflow-recipes`.

**Pre-existing, environmental.** `pr_stack_artifact_paths_acceptance::a_plan_left_at_the_legacy_session_root_is_still_advertised_to_the_agent`
fails under `./dev` on macOS: the nix shell's `TMPDIR` is `/tmp/nix-shell.*`, and
`tddy_core::workflow::build_context_header` prints the canonical `/private/tmp/…` path. It passes
5/5 with a canonical `TMPDIR`. Neither the test nor the code it exercises is touched here.

## Verification

```bash
./test -p tddy-workflow-recipes -p tddy-pr-stack
cargo clippy -p tddy-workflow-recipes -p tddy-pr-stack -- -D warnings
cargo build -p tddy-coder -p tddy-tools -p tddy-daemon   # three of the twelve consumers, for AC3
cargo fmt --all --check
tddy-tools restructure verify --against HEAD
```

As the stack tip, this node also runs the **backlog-delta sweep** at `/pr-wrap`: every entry the
`#carve` stack wrote or edited is judged for whether one more node could close it while the context
is still loaded.

## Validation Results

**Run:** 2026-09-22 (`/pr-wrap` validation passes), diff `origin/feature/carve/presenter-split...HEAD`
(3 commits, leak-free).

**Scoped gates:** `cargo clippy -p tddy-pr-stack -p tddy-workflow-recipes --all-targets -- -D warnings`
clean; `./test -p tddy-pr-stack -p tddy-workflow-recipes` (with `TMPDIR=/private/tmp`) 543 passed,
0 failed; test counts per moved file unchanged (stack_ops 23 + recipe 21 = old 44; assess 14, git_ops
7, pr_insight 2, docs 12). `restructure verify` (AC5) not re-run here. Whole-workspace health: CI.

### From /validate-changes

- Stack boundary: ✅ nothing from `## Dependencies` implemented; no parent-owned files deleted;
  `## Boundaries` respected (no `plan_pr_stack`/`writer`/`parser` moved, no consumer edited).
- ⚠ Visibility: the changeset says "one widening", but at crate level `git_ops` (`pub(crate)` →
  `pub mod`) and `assess` (private → `pub mod`) are widened too — unavoidable across a crate
  boundary, but it newly publishes the panicking stub `git_ops::build_integration_ref`
  (`unimplemented!`, unused anywhere).
- ⚠ Stale `code-issues` records: `squatting-pr-stack-data-model.md` (closed by this PR — delete at
  wrap, measurement into the change-history entry), `complexity-mod-pull-base-into-node-branch.md`
  and `complexity-mod-repoint-planned-pr-node.md` (location now `tddy-pr-stack/src/stack_ops.rs`;
  move/rename under `packages/tddy-pr-stack/docs/code-issues/`).
- ⚠ PRD drift: FR1 still lists `actions.rs`, AC1 omits `tddy-workflow`, Snag 2 still says the const
  moves — deviations are recorded here but not in the PRD.
- nit: line counts (~4,230 / 4,244) vs actual 4,785 in `tddy-pr-stack/src`; node numbering 9/9 vs
  10/10 (code-issue) vs 10/11 (commit a2bceddb).

### From /validate-tests

- ⚠ `tests/pr_stack_crate_shape.rs:48` — `text.contains("tddy-git")` is satisfied by `tddy-github`;
  the `tddy-git` assertion is vacuous.
- nit: `:134` asserts the bare name `reseed_stack_from_plan_if_unspawned` (a `pub use` or a call
  would satisfy it) — assert `pub fn reseed_stack_from_plan_if_unspawned`.
- nit: `:83` scans `src/` non-recursively; `:65` matches comments in the manifest.

### From /validate-prod-ready

- ⚠ `packages/tddy-pr-stack/Cargo.toml` — `serde_json` and dev-dependency `pretty_assertions` are
  unused by any file in the crate (copied from recipes).
- nit (pre-existing, now in a lib's public API): `git_ops.rs:38,72,99,441` `#[allow(dead_code)]` on
  `pub fn`s is inert; `git_ops.rs:438` untracked `TODO` + `unimplemented!` stub.

### From /analyze-clean-code

- No new logic beyond facades and one verbatim-moved pure function; no findings on introduced code.
  Pre-existing complexity in moved code is already tracked in the two `complexity-mod-*` records.

### Docs

- ⚠ `cargo doc -p tddy-pr-stack --no-deps`: **introduced** broken link `stack_ops.rs:274`
  (`[reseed_stack_from_plan_if_unspawned]` stayed recipe-side); pre-existing private-item link
  `stack_ops.rs:279` (`[next_free_node_id]`).
- nit: recipes `README.md:69` "twelve crates were compiling every recipe" — they still do until they
  switch to `tddy_pr_stack`; `docs/ft/coder/pr-stack-docs.md:44,78` cite `pr_stack/mod.rs:<line>`
  (already stale; the code is now in `tddy-pr-stack`).
