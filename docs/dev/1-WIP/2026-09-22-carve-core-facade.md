# Changeset: carve-core-facade

**Date**: 2026-09-22
**Status**: 🚧 In Progress — planned and red
**Type**: Refactor (crate extraction, no behaviour change)
**Stack**: `#carve` 12/12, on top of `rpc-handlers`

## Initial Discovery

[2026-09-22-carve-core-facade-initial-discovery.md](./2026-09-22-carve-core-facade-initial-discovery.md)
— the module graph and its one cycle, per-crate consumer counts, receiving-crate eligibility, and the
post-#493 sizes on master.

## Related documentation

- PRD: [2026-09-22-carve-core-facade-prd.md](./2026-09-22-carve-core-facade-prd.md)

## Affected Packages

- **`tddy-core`** — every module leaves. What stays is `lib.rs`, the facades, `ssh_exec` and the
  `cfg(test)` `test_support`.
- **`tddy-workflow`** — gains `PermissionHint` and `GoalHints` (Cut 1).
- **`tddy-session-store`** — nothing moves into it. The new `tddy-changeset` depends on it for
  `atomic_file` and `error`.
- **New crates:** `tddy-log`, `tddy-changeset`, `tddy-session-worktree`, `tddy-session-actions`,
  `tddy-toolcall`, `tddy-agent-backend`, `tddy-agent-skills`, `tddy-workflow-engine`,
  `tddy-presenter`.
- **Consumers:** none edited; they resolve through `tddy-core`'s facades.

## Summary

`tddy-core` is 20,924 production lines on master. It holds twelve unrelated responsibilities, 1,090
lines of never-compiled files, and one five-module cycle. The developer's target is a **wiring
point**: every group moves to a crate of its own, `tddy-core` keeps only `pub use` facades, and
**every receiving crate stays at or under 10,000 production lines**. It is delivered as one PR.

## Responsibility

- Delete the six dead `workflow/*.rs` files and the unused `futures` dependency.
- **Cut 1:** move `PermissionHint` and `GoalHints` to `tddy-workflow`.
- **Cut 2:** move `start_goal_for_session_continue` up to `tddy-workflow-engine`.
- Retarget `error.rs:3` and `toolcall/mod.rs:127` to `tddy_workflow::ClarificationQuestion`.
- Extract the nine new crates in the PRD's FR3 order, each ≤ 10k production lines.
- Leave a facade for every `tddy_core::<module>` path and every root-level re-export.
- Move tddy-core's 49 test files, and the code-issue records, with the code they cover.

## Boundaries

- Does **not** edit any consumer crate. Every consumer keeps naming `tddy_core::…`; repointing and
  deleting `tddy-core` is a later pass.
- Does **not** change behaviour: moved bodies are byte-identical apart from `use` paths and module
  wiring.
- Does **not** re-split what #495 partitioned in place (`presenter/presenter_impl/*`). The presenter
  moves as it is.
- Does **not** move anything into `tddy-session-store`. #493 established it, and putting the jobs
  there would close a cycle (discovery, conclusion 6).
- Does **not** touch `tddy-session-lifecycle` or `tddy-daemon-rpc`. Those belong to #520 and its
  successors.
- Does **not** fix the complexity of what moves.
- Does **not** re-export `plan_prd_path_for_session_dir`; the `workflow_decouple_acceptance` guard
  keeps holding.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `7/11` session-store (#493, **merged to master**) | `tddy-session-store` (`atomic_file`, `error`, `output`, most of `session_actions`) and `tddy-session-catalog` | `tddy-changeset` and `tddy-session-actions` build on the store, and `session_dir.rs` / `session_action_jobs` move now that the changeset leaves core | re-extract the store or catalog, or move anything into them |
| `9/11` presenter-split (#495) | `presenter/presenter_impl/*` partitioned in place | the presenter moves to `tddy-presenter` as partitioned | re-partition it, or change `presenter_impl`'s modules |
| `5/11` core-foundations (merged) | `tddy-workflow` holding the shared vocabulary | Cut 1 adds two types to it | move anything else into it |
| `4/11` test-homes (merged) | `move_test_binary_to_crate` | the 49 test files travel | re-implement it |
| `11` rpc-handlers (#520) | nothing this node consumes; it is the parent only because the stack is a line | — | touch `tddy-daemon-rpc` or `tddy-session-lifecycle` |

## Draft PR contract

This is a mechanical extraction, one of the pr-stack skill's two named exceptions, so there is no
stub surface to publish. The draft is the **failing shape tests** in
`packages/tddy-core/tests/core_facade_shape.rs`, plus the **path guard** that must stay green on both
sides of the move. **This PR must not merge in that state.**

## Green wave

**Wave:** after the stack is on master.
**Greenable independently:** **not yet.** The branch is behind master: #493's `tddy-session-store`
and `tddy-session-catalog` are not in this tree. A cascade `/pr-stack-rebase` #494..#520, then this
node, comes first. **It is left to the developer, who declined to have this plan run it
(2026-09-22).** After the cascade, **yes**: every test here reads this node's own tree.
**Concurrent with:** #520's `/green`. The two touch disjoint crates.
**Blocks:** nothing.

## Prerequisites

| Item | Verdict | What this change does about it |
|---|---|---|
| **The stack is behind master** (#493 merged after #494..#520 were cut) | ⛔ **BLOCKING** | Cascade `/pr-stack-rebase` #494..#520 and then this branch **before green**. Left to the developer. Listed in `## Scope` |
| [2026-09-19-backend-cannot-be-extracted-while-workflow-recipe-is-not-a-leaf.md](../todo/2026-09-19-backend-cannot-be-extracted-while-workflow-recipe-is-not-a-leaf.md) | ✅ **RESOLVED HERE** | Its premise is false: no compiled backend file names `WorkflowRecipe` (discovery, conclusion 5). Cut 1 extracts the backend. The wrap deletes the entry |
| `packages/tddy-core/docs/code-issues/cycle-dto-inside-behaviour-module.md` | ✅ **RESOLVED HERE** | "Claimed by: nobody", and it points at the entry above. Cuts 1 and 2 open the cycle. Delete it at wrap, with the final measurement recorded in the change-history entry |
| [2026-09-19-three-oversized-files-grew-by-an-import-split.md](../todo/2026-09-19-three-oversized-files-grew-by-an-import-split.md) | ⚠ **DURING** | Cut 1 retargets the imports in `backend/claude.rs` and `backend/stub.rs`. They must not grow again; fold the split `use` back into one where the new path allows it |
| `packages/tddy-core/docs/code-issues/heavy-dependency-sqlx-session-catalog.md`, `squatting-git-plumbing-worktree.md` | ℹ **ANSWERED — stale** | Claimed by #493 and #492, both merged. **Deleted on master already**; the cascade drops them here. Nothing to do |
| The 19 `packages/tddy-core/docs/code-issues/complexity-*.md` records | ⚠ **MOVE** | Each moves to `packages/<new-crate>/docs/code-issues/` with its file, with **Location** updated and a `**Moved:**` line. None is closed |
| [2026-07-01-tddy-core.md](../todo/2026-07-01-tddy-core.md) (toolcall listener → tddy-rpc) | — unrelated | The listener moves to `tddy-toolcall` unchanged. The entry's path goes stale, so update it at wrap |

`tddy-workflow` has no `docs/code-issues/`, so it has not been analysed. All nine new crates are
likewise unanalysed. Run `/analyze-code-issues` against each at wrap, so the moved complexity is
measured in its new home.

## Scope

- [ ] ⛔ Stack on master: cascade `/pr-stack-rebase` #494..#520, then this branch (the developer's to run)
- [x] Dead files deleted; `futures` dropped
- [x] Cut 1, Cut 2, the two retargets
- [x] `tddy-log`, `tddy-agent-skills`
- [x] `tddy-changeset`, `tddy-session-worktree`, `tddy-session-actions`
- [x] `tddy-toolcall`, `tddy-agent-backend`
- [ ] `tddy-workflow-engine`, `tddy-presenter`
- [ ] `tddy-core` facades only
- [ ] Tests and code-issue records moved

## Technical changes

### State A (master after #493)

`tddy-core` is 20,924 production lines. The largest modules are `backend` 4,684, `presenter` 3,962,
`workflow` 2,883 (of which 1,090 dead), `toolcall` 1,451, `changeset` 1,018, `stream` 884 and
`log_backend` 862. The SCC is `backend → toolcall → session_actions → changeset → workflow →
backend`. There are 41 `pub mod` paths and grouped root re-exports, and consumers name
`tddy_core::<module>::…` from several hundred sites.

### State B

```
tddy-workflow · tddy-log · tddy-agent-skills
        ↑
tddy-changeset ──► tddy-session-store, tddy-graph
        ↑
tddy-session-worktree (──► tddy-git) · tddy-session-actions
        ↑
tddy-toolcall ──► tddy-rpc, tddy-stdio
        ↑
tddy-agent-backend
        ↑
tddy-workflow-engine
        ↑
tddy-presenter
        ↑
tddy-core   (pub use facades only, ~140 lines)
```

### Delta

| Package | Change |
|---|---|
| `tddy-core` | − every module body; + facade modules; − `futures`, `agent-client-protocol`, `tokio-util`, `jsonschema` from its own deps; its tests move out |
| `tddy-workflow` | + `PermissionHint`, `GoalHints` |
| nine new crates | per PRD FR3 |
| root `Cargo.toml` | + nine workspace members |

## Implementation milestones

- [x] The path guard is green before anything moves
- [x] Dead files, `futures` — AC3
- [x] Cut 1 + retargets; tddy-core still builds as one crate — AC4 half
- [x] `tddy-log`, `tddy-agent-skills` extracted
- [x] Cut 2; `tddy-changeset` extracted — AC4 other half
- [x] `tddy-session-worktree`, `tddy-session-actions` extracted
- [x] `tddy-toolcall`, `tddy-agent-backend` extracted
- [ ] `tddy-workflow-engine`, `tddy-presenter` extracted
- [ ] `tddy-core` facades only — AC1; path guard still green
- [ ] Test files and code-issue records moved; AC2 and AC5–AC8 green

## Testing plan

**Test level.** A mechanical move. Behaviour stays pinned by tddy-core's 49 existing test files,
moved unedited. New tests pin the **shape** — what moved, where, and in which dependency direction —
and one **compile-level guard** pins that the public paths consumers use still resolve.

**Options considered.**

| Option | Trade-off | Verdict |
|---|---|---|
| Rely only on the moved suites | they pass just as well with everything still in core | insufficient |
| Text shape tests, as `#carve` 7–10 used | cheap, and pin exactly what the node claims | **used** |
| Compile-level tests naming the new crates | would break `cargo test -p tddy-core`'s whole compile until the crates exist, blocking everyone | **rejected** for the red phase |
| Compile-level guard over `tddy_core::…` paths | green before and after; catches a missing facade | **used** |

### Acceptance tests

`packages/tddy-core/tests/core_facade_shape.rs`:

- `tddy_core_is_only_a_wiring_point` — AC1
- `every_crate_receiving_code_stays_within_10k_production_lines` — AC2
- `the_never_compiled_workflow_files_are_gone` — AC3
- `the_unused_futures_dependency_is_dropped` — AC3
- `the_recipe_hints_are_defined_by_the_workflow_vocabulary_crate` — Cut 1
- `the_session_continue_goal_is_chosen_by_the_workflow_engine` — Cut 2
- `the_agent_backend_does_not_depend_on_the_workflow_engine` — AC4
- `the_changeset_crate_does_not_depend_on_the_workflow_engine` — AC4
- `no_receiving_crate_depends_on_tddy_core` — AC5
- `each_receiver_depends_only_on_crates_below_it` — AC5
- `tddy_core_no_longer_names_the_heavy_dependencies_itself` — AC6

`packages/tddy-core/tests/core_facade_paths.rs` (**green by design**, a guard):

- `the_public_paths_consumers_use_still_resolve` — AC7. It names a representative path from each
  group through `tddy_core::…`.

### Coverage

- Each group's behaviour is covered by the suites that move with it.
- AC7's full claim — no consumer edited — is checked by `/validate-changes` against the diff; a test
  cannot see a diff.
- AC8 is covered by the moved suites passing unedited.

## Technical debt & production readiness

## Decisions & trade-offs

- **One PR, every group.** The developer's decision (2026-09-22), taken after seeing the size
  (~19.7k lines, nine crates) and a two-node alternative split at the cycle cut.
- **A wiring point, not ~10k and not vanish.** The developer's decision. A facade keeps every
  consumer unedited; vanishing is a cheaper later pass.
- **`tddy-changeset`, not an extended `tddy-session-store`.** The store is #493's; changeset plus
  metadata is its own responsibility, and the jobs must sit above the changeset anyway.
- **No compile-level tests against the new crates in the red phase.** A red compile error in one
  test binary fails `cargo test -p tddy-core` for every other worker.
- **The stack's rebase onto master is not run by this plan.** It rewrites branches that four other
  worktrees own, and the developer chose to start it.

## Refactoring needed

## Validation results

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [x] Failing acceptance tests + path guard
- [ ] USER REVIEW — acceptance tests
- [ ] ⛔ Stack cascaded onto master (developer)
- [ ] TDD Green (`/green`)
- [ ] Move code-issue records
- [ ] `/validate-changes`
- [ ] `/pr-wrap`
- [ ] Wrap documentation (`/wrap-context-docs`) — deletes the ✅ entries above

## Verification

Scoped, per CLAUDE.md:

```bash
./test -p tddy-core -p tddy-workflow -p tddy-log -p tddy-changeset -p tddy-session-worktree \
       -p tddy-session-actions -p tddy-toolcall -p tddy-agent-backend -p tddy-agent-skills \
       -p tddy-workflow-engine -p tddy-presenter
cargo check --workspace --all-targets   # AC7: every consumer compiles unedited — CI's to confirm in full
```
