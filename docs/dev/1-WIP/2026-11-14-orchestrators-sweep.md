# Changeset: refactor — the `#orchestrators` sweep pays down the stack's code issues and TODOs

**Date**: 2026-11-14
**Status**: 🚧 In Progress — implemented on the rebased sweep base; B2, B6 (remainder) and B8 are stopped items (engine refusals recorded in their records, below)
**Type**: Refactor
**Complexity**: 5/5 — eight packages (four Rust crates with restructures, three web packages with hand splits, one more crate for a scoped fix), seven oversized files and one complexity record, an inherited `server.rs` split of about 2,000 lines, and a behaviour change to the PR-stack RPC contract with its tests
**Stack**: `#orchestrators` 12/12, the sweep node. Base: `feature/orchestrators/project-settings-pane` (#888)

## Initial Discovery

Full exploration, the measurement commands and every seam read:
[2026-11-14-orchestrators-sweep-initial-discovery.md](./2026-11-14-orchestrators-sweep-initial-discovery.md).

## Related Feature Documentation

None for the restructures, which preserve behaviour and have no PRD. The one behaviour change (B9) is
the developer's decision, recorded below. It edits `docs/ft/coder/pr-stacking.md` through this
changeset's Final Checklist.

## Summary

This node closes every code issue that `#orchestrators` (#878 → #888) added or reopened. It also closes
the oversized files that `[W]` declined to split because they were shared with other nodes of the stack.
It closes the TODOs that a scoped change can close, and it removes the first-write-create path on the
`session_id`-addressed PR-stack RPCs, as the developer decided on 2026-11-14. Every public path stays
reachable: the Rust moves keep their callers on `reexport: "outside"` or `"glob"`, and the TypeScript
splits keep each component's export.

## Background

`/pr-wrap` step 3.5 does not split a file that a later node also edits. Over this stack that left one
new 999-line file (`stack_git_tools.rs`), a drawer screen pushed past budget, and seven files that were
already over budget and grew or were touched. With every node green, nothing downstream shares these
files any more, so the splits are now safe.

## Dependencies

- **Inherited:** every other `#orchestrators` node: #878 (n1), #879 (n2), #880 (n3), #881 (n4), #882 (n5),
  #883 (n6), #884 (n7), #885 (n8), #886 (n9), #887 (n10) and #888 (n11). This node implements no
  surface of theirs. It only moves, extracts and splits what they left, and changes the one guard the
  developer named (B9).
- **Engine:** `tddy-tools restructure` at this base. Its known `move_item` defects:
  `docs/dev/todo/2026-11-09-restructure-move-item-defects-found-splitting-coding-subagent-session.md`.
  `[G]` runs one plan file per seam group, as the `#agent-tools` sweep did, to stay clear of E1.

## Prerequisites — the deferred-work cross-check (Step 2b)

| Record | Verdict |
|---|---|
| the eight code-issue records in B1–B8 | **fix here** |
| `packages/tddy-daemon/docs/code-issues/complexity-mount-host-build-session-host.md` | out of scope: improved by the stack (74 → 73) |
| `packages/tddy-pr-stack/docs/code-issues/complexity-pull-base-pull-base-into-node-branch.md` | out of scope: improved (158 → 157) |
| `packages/tddy-pr-stack/docs/code-issues/complexity-repoint-repoint-planned-pr-node.md` | out of scope: improved (79 → 78) |
| `packages/tddy-workflow-recipes/docs/code-issues/complexity-bridge-execute-stack-repoint.md` | out of scope: improved (121 → 120) |
| `docs/dev/todo/2026-11-13-spawner-scope-check-reads-changeset-with-ok.md` | **✅ RESOLVED HERE** (T1) |
| `docs/dev/todo/2026-11-14-conversation-rotation-residual-drift.md` | **narrowed here** (T2: item 4). The rest goes to the TODO gate |
| the other TODOs the stack added | the TODO gate, below |

`grep -l 'Claimed by' packages/*/docs/code-issues/*` over the touched packages finds no record claimed
by an open PR.

## Scope

- [ ] **Baseline**: CI on #888's head (below) read and recorded by failing-test name, before the first plan
- [x] **Plan**: anchors emitted with `restructure anchors` (from the warm index), one plan file per seam group, and `check --deep` clean for each. (B1/B4 fell back to the `extract_module` cut when `move_item` was refused on an unrelated `#[path]` module; B2/B6/B8 refusals recorded in their records.)
- [x] **Apply**: each plan applied and committed on its own
- [x] **Hand edits**: the TS splits (B10–B12), B9, T1 and T2, each in its own commit
- [ ] **Baseline after**: the scoped suites back to the recorded failing set by name. B9's rewritten tests are the only intended difference
- [ ] **Code quality**: `cargo fmt --check`; `cargo clippy -p <touched> --all-targets -- -D warnings`
- [ ] **Re-measure**: `/analyze-code-issues` adds none and closes every backlog row. Each record is deleted with its final numbers in the change-history entry
- [ ] **Documentation**: the Final Checklist executed

## Backlog — each item, and the intent that closes it

The plan is [`2026-11-14-orchestrators-sweep-plans/plan.jsonl`](./2026-11-14-orchestrators-sweep-plans/plan.jsonl)
(28 operations, ids `r1-a` … `r8-c`). **It is intents only.** Its anchors name items by path, without
fingerprints, ranges or a header. `[G]` replaces each one with the output of
`restructure anchors --items …` (or `--at L:C-L:C` for an `extract_method` range), writes the header
with `restructure snapshot`, splits the file per seam group, and proves each with `check --deep`. That
check is **deferred to `[G]`** by the developer's no-local-builds rule for this planning run.

### B1 — `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-stack-git-tools.md`: `stack_git_tools.rs` 999 → ≈ 430

- `r1-a`: `move_item` (with `name`) of the git side (`GitWork` … `merge_tree_conflicts`) into
  `stack_git_tools::git_work`.
- `r1-b`: then the five node operations and `merge_tree_conflicts` into `git_work::node_ops` (≈ 195).
  This leaves `git_work` at ≈ 375.
- If `move_item` is refused again on the crate's unrelated `#[path]` module, fall back to the
  `extract_module` + `extract_module_to_file` cut that #887 proved with `check --deep`.

Closed when no file in `stack_git_tools/` is over 500. Delete the record.

### B2 — `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md`: `pr_stack/ports.rs` 868 → ≈ 450

The file is one trait impl.
- `r2-x1` … `r2-x4`: `extract_method` the bodies of `query_branch`, `resolve_stack_base`,
  `get_pr_status` and `pull_base_into_branch` into inherent `serve_*` methods. Each range starts after
  the `forward_if_addressed … return` guard, because a range must hold no early `return`.
- `r2-m1`, `r2-m2`: `move_impl_members` those methods into `pr_stack::stack_reads` and
  `pr_stack::pull_base_leg`.
- The trait impl keeps one-line delegations.

Closed when `ports.rs` and both new files are each at most 500. Delete the record.

### B3 — `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md`: 566 → ≈ 446

`r3`: `move_impl_members` of `add_project_to_host_at_project_coordinate` (the record's Seam A) into
`project::add_to_host`. The record was not updated when #888 added two lines (564 → 566). Its final
row records that. Delete the record.

### B4 — `packages/tddy-tools/docs/code-issues/oversized-file-session-hook.md`: 547 → ≈ 422

`r4`: `move_item` (with `name`) of `REPORT_TIMEOUT`, `deliver`, `HookTransport`, `hook_transport`,
`exchange`, `call_over_socket` and `post_proto` into `session_hook::transport`, with
`reexport: "outside"`. Delete the record.

### B5 — `packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md`: 893 → ≈ 375

The moves, each with `reexport: "outside"`:
- `r5-a`, `r5-b`: the shell surface goes to `shell_tool`. This is the seam the record names.
- `r5-c`: Grep and Glob go to `search_tools`.
- `r5-d`, `r5-e`: the LSP bridge goes to `lsp_tools`.

Delete the record.

### B6 — `packages/tddy-tools/docs/code-issues/oversized-file-server.md`: 2,289 → ≈ 480

The stack *improved* this file by 791 lines. It is in scope because `[W]` deferred its split to the sweep
(stack-shared). The moves:
- `r6-a` … `r6-e`: the subagent surface goes to `server::subagent_tools`, which takes three children
  (`turns`, `status`, `schemas`), so that none of them is over 500.
- `r6-f`: the approval members of `impl PermissionServer` go to `server::approval`.
- `r6-g`: the dynamic tool list goes to `server::dynamic_tools`.
- `r6-h`: the tool input types go to `server::tool_inputs`.

`relay_approve` is a `#[cfg]` pair. If the engine refuses it, as it refused cfg pairs in the
`#agent-tools` sweep (B2 there), it is merged into one item by hand first, in its own commit, with the
developer's consent at 6e (**Q2**). The `#[tool_router] impl` stays: a macro owns it. Delete the record.

### B7 — `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-callbacks.md`: 687 → ≈ 400

- `r7-a`: the session-list items go to `telegram_session_control::session_list_entries`.
- `r7-b`: the recipe, intent, project, agent and branch callback parsers go to
  `telegram_session_control::callback_data`.

Both use `reexport: "glob"`, because every sibling reaches them through `use super::*`. Delete the record.

### B8 — `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md`

`start_session_on_request` is 83 lines and `start_here_body` is 273. Both must reach 60 or less.
- `r8-a`, `r8-b`: in `start_here_body`, `extract_method` each arm's body without its trailing
  `return`: the `NoAgent` arm, and the placement and refusal block. Ordered bottom-up. Further extracts
  are added at `[G]` once the arms have been read.
- `r8-c`: in `start_session_on_request`, `extract_method` of the route `match`.

The E4 seams (early-return guards) stay as call sites. ⚠ This is the backlog item most likely to
stop 🔴: a `return` inside an arm body blocks the cut.

### B9 — the developer's decision (2026-11-14): no first-write create over the `session_id`-addressed RPCs

The scope of this change:
- A `StackTarget::Session` whose changeset names no `stack_id` is refused, whatever its recipe, with
  `FailedPrecondition` and a message that names `pr_stack_create`.
- `require_pr_stack_orchestrator` (`pr_stack/guards.rs`) is deleted.
- `require_orchestrator_unless_stacked` (`pr_stack/project_stacks.rs`) becomes `require_a_stack`. It is
  called at `ports.rs` `add_planned_pr`, `link_stack_node` and `reorder_planned_pr`, and in
  `session_dir_for_target`.

This is TDD and comes after the restructures, so that their baseline comparison stays clean:
- **Red.** Rewrite the four recipe-refusal tests as "a session naming no stack is refused, naming
  `pr_stack_create`". Add one with a `pr-stack`-recipe session that names no stack (refused: this is
  the removed path). Move `record_the_plan` in `planned_pr_mutation_rpc_acceptance.rs` and the link
  fixture onto a store record plus `stack_id`.
- **Green.** Change the guard.

`update_stack_atomic`'s first-write create in `tddy-changeset` is **not** touched. Its in-process callers
(`tddy-pr-stack`, `tddy-workflow-recipes`) keep it (**Q1**).

### B10 — `packages/tddy-session-web/docs/code-issues/oversized-file-sessions-drawer-screen.md`: 625 → ≤ 500 (hand split)

The record says 581. #888 took the file to 625 and did not update it.
- Move the detail-pane route cases into a sibling module, as the record says: `SessionsDrawerDetailPane`
  (the composer, the project stack view, the project settings pane and the session views).
- Leave the drawer composition in `SessionsDrawerScreen`.
- Prove it with the package's component specs, green before and after.

Delete the record.

### B11 — `packages/tddy-prstack-web/docs/code-issues/oversized-file-pr-stack-screen.md`: 731 → ≤ 500 (hand split)

As the record says: the Start-session flow moves into `usePrStackStartSession` and
`PrStackStartSessionDialog`. That takes `startSessionNode`, `startSessionInitialValues`,
`handleChildSessionCreated`, the blockers and the base-branch derivation. If that is not enough, the
stack-address loading that #886 added moves into `usePrStackPlan`. The baseline is the `PrStack*.cy.tsx`
specs. Delete the record.

### B12 — `packages/tddy-session-create-web/docs/code-issues/oversized-file-create-session-pane.md`: 591 → ≤ 500 (hand split)

- `useStartingHost`, as the record says: `startingHostId`, `startingHostLabel` and the split-codebase
  choice.
- A second cut, because 91 lines have to go: the orchestrator and effort field cluster becomes a
  `CreateSessionOrchestratorFields` component.
- The baseline is `CreateSessionPane.cy.tsx` and `CreateSessionStartProgress.cy.tsx`.

Delete the record.

### T1 — `docs/dev/todo/2026-11-13-spawner-scope-check-reads-changeset-with-ok.md` ✅ RESOLVED HERE

`refuse_a_spawner_outside_its_own_sessions` (`tddy-daemon-rpc/src/exec_tool/project_home/caller.rs`)
matches on `read_changeset` the way `answers.rs::require_orchestrator` does:
- `Ok` → compare.
- `ChangesetMissing` → today's refusal.
- Any other error → a refusal that names the unreadable changeset.

The test uses a corrupt `changeset.yaml`. This is a scoped defect fix, done test-first. Delete the TODO.

### T2 — `docs/dev/todo/2026-11-14-conversation-rotation-residual-drift.md`: item 4 only (narrowed, not deleted)

Reword the module doc of `tddy-agent-launch/src/project_home_idle.rs` and its `stop_session` comment.
Since #883, `.session.yaml` *does* change on `/clear`, and that change is what makes "the next ensure
resumes the same conversation" true. Strike item 4 in the TODO. Items 1, 2, 3 and 5 go to the gate.

### M1 — `.agents/skills/muse-subagent/scripts/muse-agent.sh`

The `health --watch` race: a turn that is being launched has its prompt file but no exit file yet, so
it is skipped instead of being reported `ended` with `RUN_EXIT=?`. Committed in this node's commit 1.

## Callers

| | |
|---|---|
| External importers | `tddy_tool_engine`'s crate-root tool functions are `pub(crate)` or private except `execute_tool*`, `ToolOutcome` and the `pub mod`s. `tddy_tools::session_hook::hook_transport` / `HookTransport` are `pub` (`reexport: "outside"` keeps them). The `telegram_session_control` items are `pub` and re-exported through `mod.rs` (`glob`). The rest is crate-internal |
| Facade | `reexport: "outside"` (B4, B5, B6), `"glob"` (B7), `"none"` (B1, which is crate-private) |
| Consequence | No consumer edits are expected outside the touched crates. `[G]` confirms this per plan in the `check --deep` output |

## Visibility

From the applies' `visibility:` lines: B1 widened `set_phase`, `pull_number`, `realign`,
`set_parents`, `resolve_conflicts` (and the other git-side items) to `pub(crate)`; B4 widened
`deliver` and `exchange` to `pub(crate)`; B5 widened `ShellTaskBody::env` to `pub(crate)`; B6
widened `subagent_new_session_tool` and `subagent_prompt_tool` to `pub(crate)`. No public path
changed: every move kept its callers on `reexport: "outside"` / `"glob"` or an `extract_module`
facade.

## Outcome at the sweep (2026-11-14, rebased base)

Closed: B1 (999 → 432/429/195), B3 (566 → 444), B4 (547 → 430 production), B5 (893 → 408
production), B7 (687 → 405 production), B9 (guard + `CreateProjectStack` + New-stack panel action),
B10 (625 → 468), B11 (731 → 395), B12 (591 → 369), T1, T2.
Stopped (engine refusals, records open with the refusal class recorded): B2 (`ports.rs`, 868
production), B6 remainder (`server.rs` 2,289 → 2,024 production; the first subagent group is in
`server::subagent_tools`), B8 (`start_session_on_request` 83, `start_here_body` 273).

## Baseline

The authority is **CI on #888's head `2c87c1006959e4620c429f30b1851f2267e97b31`**, not a local run.

| Gate | Before (CI, #888 @ `2c87c1006`) | After |
|---|---|---|
| Rust: `tddy-daemon-rpc`, `tddy-tools`, `tddy-tool-engine`, `tddy-telegram-control`, `tddy-agent-launch` | ⚠ **not yet recorded**: no CI run exists on that head (`scripts/ci-status.sh 888` reports no checks) | |
| Web: `tddy-session-web`, `tddy-prstack-web`, `tddy-session-create-web` component specs | ⚠ same | |

`[G]` reads the CI result first and records the failing set by name. If #888's head has no run by then,
it asks for one before applying anything. It does not substitute a local full run.

## Plan

`docs/dev/1-WIP/2026-11-14-orchestrators-sweep-plans/plan.jsonl`: 28 intent lines, with no header,
fingerprints or ranges. `restructure check` is deferred to `[G]` (see Backlog).

Order for `[G]`:
1. **B1–B8.** Restructures, one plan file and one commit per seam group. Definitions go before their users. B8 goes last because it may stop.
2. **B9.** Red, then green.
3. **T1.** Red, then green.
4. **T2.** Comments.
5. **B10–B12.** Hand TS splits.
6. **Re-measure.**

## Decisions & Trade-offs

- **Complexity 5/5 is a boundary warning.** If `[G]` runs out of room, the clean split is two nodes:
  the Rust restructures plus B9, T1 and T2 in one, and the three TS hand splits in the other. The 6e
  review decides.
- **Q1 answered (developer, 2026-11-14): add a create RPC.** B9 also adds `CreateProjectStack`
  (`pr_stack.proto`: project id, optional title, optional owning `daemon_instance_id`; answers the new
  `StackAddress`) backed by the same `ProjectStackStore::create_titled` path as `pr_stack_create`, forwarded
  to the owning host like the other address-based RPCs, and a "New stack" action in the web stack panel
  (`tddy-prstack-web`) that creates a stack and then adds planned PRs to it by address. With it, B9's
  refusal ("this session names no stack — create one with pr_stack_create or New stack") has a web
  answer. Failing tests first (RPC acceptance + the panel's Cypress spec), as for the rest of B9.
- **Q2 (orchestrator decision, developer: finish without stopping, 2026-11-14): merge the `relay_approve` cfg pair** in `server.rs` by hand if the engine refuses it, as one commit with no behaviour change.
- **`server.rs` (B6) is in scope although the stack shrank it.** The sweep rule takes every stack-shared
  file `[W]` deferred to it, and this is the largest item. The developer may narrow it at 6e. A code
  issue is never deferred by the planner.
- **`session_metadata.rs` is not a code issue.** It has 409 production lines, and the 1,087-line total
  is mostly its test module. The wrap reports named it, but nothing is due.
- **No function-level re-measure at planning.** `/analyze-code-issues` was not run. Any function the
  stack pushed over a budget without a record surfaces at `[W]`'s re-measure, and this node then has to
  close it too.

## TODO gate candidates

These are not in the plan. No restructure or scoped change closes them. They are shown to the user at
the gate once this node is green.

| # | TODO | Why the sweep cannot close it | Recommendation |
|---|---|---|---|
| 1 | `docs/dev/todo/2026-11-13-acp-turn-input-is-not-delivered.md` | It is blocked on an external fact: no agent YAML declares `acp_prompt`. The developer ruled on 2026-11-12: "do not implement it in `#orchestrators`" | **defer** |
| 2 | `docs/dev/todo/2026-11-14-jail-list-exec-tools-misses-stack-node-reads.md` | It needs a design choice: how a jail learns a session's node binding (a shared classifier in `tddy-tool-engine`, or the binding recorded on the session at spawn). That is feature work in `tddy-session-lifecycle` | **continue**, as one node together with #3 (one concern: a node binding visible outside `tddy-daemon-rpc`) |
| 3 | `docs/dev/todo/2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md` § *a stack node's read tools on another host* (added by #884) | It is feature work: the `ListSessions` widening, or the owner checking its own stack record. The developer accepted the gap on 2026-11-14 | **continue** together with #2, or **defer** both. Since the owning host holds the stack record, the local check is small |
| 4 | `docs/dev/todo/2026-11-14-retire-stack-child-spawn-handler.md` | It deletes files across four crates (`tddy-agent-launch`, `tddy-toolcall`, `tddy-cli-sessions`, `tddy-session-lifecycle`), and deleting files needs consent. It also re-cuts two wiring suites so that the operator-dialog spawn keeps its coverage. That is test design, not a restructure | **continue**, as its own node after the sweep. The code is unreachable now, and it is cheapest while the stack's context is loaded |
| 5 | `docs/dev/todo/2026-11-14-conversation-rotation-residual-drift.md` items 1, 2, 3, 5 | (1) needs confirmation of what Claude sends for an in-CLI `/resume`. (2) is a fallback, which needs the developer's consent. (3) is a refusal or loud log, which is a behaviour choice. (5) is a new acceptance test of the chain. Item 4 is done here (T2) | **continue** for (3) and (5) as one node. **Defer** (1) and (2) until the developer rules on (2) |

## Final Checklist

Tasks executed by `/wrap-context-docs` before this changeset is deleted.

- [ ] `packages/tddy-daemon-rpc/docs/changesets/2026-11-14-orchestrators-sweep.md`: the release note. It covers B1, B2, B3, B9 and T1, with before and after numbers
- [ ] `packages/tddy-tools/docs/changesets/…`, `packages/tddy-tool-engine/docs/changesets/…`, `packages/tddy-telegram-control/docs/changesets/…`, `packages/tddy-agent-launch/docs/changesets/…`, and the three web packages: one release note each
- [ ] `docs/dev/changesets/2026-11-14-orchestrators-sweep.md`: the cross-package entry. It carries **every deleted record's final measurement**
- [ ] `docs/ft/coder/pr-stacking.md:421`: "only a session naming no stack still needs the `pr-stack` recipe, because its first write creates the stack". Replace with: a session naming no stack is refused, and a stack is made with `pr_stack_create`
- [ ] `docs/ft/coder/pr-stacking.md:97`: keep `update_stack_atomic`'s first-write rule, but say that no RPC reaches it any more (subject to Q1)
- [ ] `packages/tddy-daemon-rpc/docs/architecture.md`: the recipe-guard paragraph, and the `exec_tool/stack_git_tools` and `pr_stack/` module maps
- [ ] `packages/tddy-tool-engine` and `packages/tddy-tools` docs: the module maps, if they list `lib.rs` / `server.rs` contents (grep at wrap)
- [ ] `docs/dev/todo/2026-11-13-spawner-scope-check-reads-changeset-with-ok.md`: deleted by T1's commit
- [ ] `docs/dev/todo/2026-11-14-conversation-rotation-residual-drift.md`: item 4 struck (T2)
