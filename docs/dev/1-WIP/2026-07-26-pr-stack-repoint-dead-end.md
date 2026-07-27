# Changeset: PR-Stack — repoint a dead-end planned PR, and never hide a row's information

**Date**: 2026-07-26
**Status**: 🚧 In Progress
**Type**: Bug Fix

## Affected Areas

- **Proto** (`packages/tddy-service/proto/connection.proto`): `RepointPlannedPrRequest` gains
  `target_base_branch = 4`.
  - `connection.proto` — one new field; regenerated into `packages/tddy-web/src/gen/connection_pb.ts`.
- **Workflow recipes** (`packages/tddy-workflow-recipes/src/`): the single-node repoint learns an explicit
  target and stops requiring a branch.
  - `pr_stack/mod.rs` — `repoint_planned_pr_node` takes `target_base_branch: Option<&str>`; a node with
    `branch = None` becomes a plan-only repoint instead of an error.
- **Daemon** (`packages/tddy-daemon/src/`): validate and forward the target.
  - `connection_service.rs` — new pure `pub fn validate_repoint_target`; `repoint_planned_pr` calls it
    against the resolved default branch and the node's parents' branches, then passes the result through.
- **Web** (`packages/tddy-web/src/`): a blocked row keeps its information and gains a real action.
  - `components/sessions/prstack/startBlockers.ts` — **new** pure module: `startBlockers` and
    `resolveRepointTarget`.
  - `components/sessions/prstack/PlannedPrRow.tsx` — always renders full information; Start-session is
    disabled rather than replaced; new base-branch line and warning strip; Repoint control names its
    target.
  - `components/sessions/prstack/PlannedPrList.tsx` — computes blockers and the repoint target instead of
    the `baseBranchMissing` / `baseBranch` pair; widens `canRepoint`.
  - `components/sessions/prstack/PrStackScreen.tsx` — new `defaultBranch` prop; sends
    `targetBaseBranch`; uses the default branch for `baseBranchLabel`; records a per-node repoint failure
    (`handleRepoint` currently `await`s with no `catch`).
  - `components/sessions/workflowViews.tsx` — `WorkflowViewContext.defaultBranch`.
  - `components/sessions/SessionMainPane.tsx` — resolves the default branch from the already-loaded
    `projects` by `session.projectId`.
- **Web test support** (`packages/tddy-web/cypress/support/`):
  - `testIds.ts` — `prStackStartWarning`, `prStackBaseBranch`, `prStackRepointError`;
    `prStackMissingBranch` removed.
  - `pages/prStackScreenPage.ts` — `startWarning`, `baseBranch`, `repointError`; `missingBranch` removed.
- **Documentation** (`docs/`): `docs/ft/coder/pr-stack-live-status.md` amended (D16–D20 + new section).

## Related Feature Documentation

- [PR-Stack live status & repoint](../../ft/coder/pr-stack-live-status.md) — amended in place; see
  [Repointing a dead-end planned PR](../../ft/coder/pr-stack-live-status.md#repointing-a-dead-end-planned-pr-added-2026-07-26)
  and decisions **D16–D20**.
- [PR stacking](../../ft/coder/pr-stacking.md)
- [Session drawer § PR-Stack Chat Screen](../../ft/web/session-drawer.md#pr-stack-chat-screen)

## Summary

A planned PR whose predecessor's PR merged and whose branch was then deleted on `origin` is a permanent
dead end: the row reads "Missing branch: `<deleted branch>`", the Start-session button is gone, and the
Repoint control is not offered because it is gated on the plan's own `pr_status.phase == "merged"` — a
field the orchestrator agent writes and which is stale in exactly this case. This changeset offers
**"Repoint to `<default branch>`"** on any unresolvable base, persists the new base in the plan, and stops
the blocked state from suppressing the row's information: every planned PR always renders in full, with a
warning naming the blocking issues and a **disabled** Start-session button.

## Background

Two independent defects meet in the reported case.

**The gate that hides the recovery.** `PlannedPrList` computes
`canRepoint = node.parents.some(p => nodeById.get(p)?.prStatus?.phase === "merged")`
(`PlannedPrList.tsx:49-51`). `prStatus` comes from `stack_plan_json`, written by the orchestrator agent's
assess pass. When the operator merges a PR on GitHub and deletes its branch — the normal GitHub flow, with
"automatically delete head branches" on — the plan still says `open`, so no Repoint control appears. Behind
it, `repoint_planned_pr_node` would have refused anyway: it errors with
`node '<id>' has no branch to repoint` (`pr_stack/mod.rs:508-510`) for a node that was never started,
which is precisely the node stuck behind the deleted base.

**The indicator that replaced the row.** D10 chose to render the blocked "Missing branch" chip *in place
of* the Start-session button (`PlannedPrRow.tsx:179-190`). The reasoning was sound — a disabled button with
no explanation is a dead end — but the row is the only place a planned PR's title, description, planned
branch, base and PR live, and once blocked the operator lost the CTA with no replacement action. The row
also never rendered its base branch at all, so "which branch is missing" was only visible inside the amber
chip's text.

A third, smaller gap falls out of the same fix: `PrStackScreen` passes `""` as `defaultBranch` to
`deriveStackBaseBranch` (`PrStackScreen.tsx:152`), so a root node's Start-session dialog shows
"New branch from base:" with no name. The default branch has to reach the row for the Repoint label
anyway.

## Scope

**High-level deliverables tracking progress throughout development:**

- [x] **Documentation**: `pr-stack-live-status.md` amended (D16–D20, new section, revised tables)
- [x] **Implementation**: proto field, recipe target rule + plan-only repoint, daemon validation, web
      blockers/target/threading
- [x] **Testing**: 41 of 41 passing (see [Implementation Evidence](#implementation-evidence))
- [ ] **Integration**: verified against a live stack with a merged-and-deleted predecessor branch
- [x] **Technical Debt**: recorded below; the resolved `docs/dev/TODO.md` entry is removed and three new
      ones are filed
- [x] **Code Quality**: `cargo clippy -p tddy-workflow-recipes -p tddy-daemon --all-targets -- -D warnings`
      clean, `cargo fmt --all --check` clean, `bun run build` clean

## Technical Changes

### State A (Current)

**Repoint availability (web).** `PlannedPrList.tsx:49-51` — `canRepoint` is true only when some direct
parent's `prStatus.phase === "merged"`. `PlannedPrRow.tsx:160-169` renders a bare `Repoint` button, no
target named.

**Repoint effect (Rust).** `pr_stack::repoint_planned_pr_node(session_dir, repo_root, node_id,
default_branch, gh)`:
1. Collects `merged_parents` via `StackNode::is_skipped()` (`changeset.rs:85-90`, `phase == "merged"`).
2. `update_stack_atomic` retains `!merged_parents.contains(p)`.
3. Re-reads, takes `effective_base_refs(node_id, default_branch)[0]`, strips `origin/`.
4. **Errors** when `node.branch` is `None` (`mod.rs:508-510`).
5. Rebase + `force_push_with_lease` when `local_branch_exists`; a rebase failure writes
   `pr_status.phase = "error"` and returns `Err`.
6. `gh.get_open_pr(branch)` → `gh.patch_pr_base(number, effective_base)`.

**Daemon.** `connection_service.rs:7383-7443` — resolves `default_branch` via
`tddy_core::resolve_default_integration_base_ref(&repo_root)` (which runs `git fetch origin`), then
`spawn_blocking` into the recipe. `RepointPlannedPrRequest` carries `{session_token, session_id, node_id}`.

**Startability (web).** `PlannedPrList.tsx:70-90` produces two values consumed by the row:
`baseBranchMissing: boolean` (a direct parent is non-merged and branchless, **or**
`resolveStackBase` is `no-ancestor-branch`, **or** the base's `remote.exists === false`) and
`baseBranch: string` (one name, blocking parent first). `PlannedPrRow.tsx:170-200` renders exactly one of
three things: status chip, `pr-stack-missing-branch-<nodeId>` chip, or the Start-session button.

**Default branch (web).** Not available on the PR-Stack screen. `PrStackScreen.tsx:152` passes `""`.
`SessionMainPane` already receives `projects: ReadonlyArray<ProjectEntry>` (`SessionMainPane.tsx:60`) from
`SessionsDrawerScreen`, which loads them via `listProjects` (`SessionsDrawerScreen.tsx:138`).
`ProjectEntry.mainBranchRef` is `p.main_branch_ref.unwrap_or_default()` — empty for a legacy project
(`connection_service.rs:4006`).

### State B (Target)

**Proto.** `RepointPlannedPrRequest.target_base_branch = 4` — the branch the node should be based onto
after the repoint, as named by the control the operator clicked. Empty preserves today's behaviour.

**Repoint effect (Rust).** `repoint_planned_pr_node(session_dir, repo_root, node_id, default_branch,
target_base_branch: Option<&str>, gh)`:
- `Some(target)` → retain exactly the parents whose `branch == Some(target)`; drop the rest.
- `None` → retain `!is_skipped()` parents (today's rule, byte-for-byte).
- `node.branch == None` → plan-only: persist the parent change and return the node; no rebase, no
  force-push, no `patch_pr_base`.
- Everything else unchanged.

**Daemon.** A new pure helper, in the same shape as the existing `effective_spawn_branch`:

```rust
pub fn validate_repoint_target(
    target_base_branch: &str,
    default_branch: &str,
    parent_branches: &[&str],
) -> Result<Option<String>, String>;
```

Empty or whitespace-only → `Ok(None)` (the drop-merged-parents rule). The default branch matches with
`origin/` stripped from both sides, since `resolve_default_integration_base_ref` returns a
remote-tracking ref while a node's `branch` and a GitHub PR base are plain names. Anything else is an
error, which `repoint_planned_pr` turns into `invalid_argument`. Validation is not optional politeness:
"no parent owns this branch" *is* the detach instruction, so an unvalidated target silently rewrites the
plan.

**Startability (web).** A new pure module `startBlockers.ts`:

```ts
export type StartBlocker =
  | { kind: "base-branch-not-on-origin"; branch: string; message: string }
  | { kind: "parent-has-no-branch"; parentTitle: string; message: string }
  | { kind: "no-ancestor-branch"; message: string };

export function startBlockers(
  node: StackNode,
  nodes: StackNode[],
  branchResolutionByBranch: Record<string, BranchResolution>,
): StartBlocker[];

export function resolveRepointTarget(
  node: StackNode,
  nodes: StackNode[],
  branchResolutionByBranch: Record<string, BranchResolution>,
  defaultBranch: string,
): string;
```

`startBlockers` returns every reason a node cannot be started, each with an operator-readable `message`;
an empty array means startable. It preserves every existing suppression rule: a node that owns a branch
has no blockers (its spawn resumes), an unarrived resolution is *unknown* rather than missing, a root or
all-merged node is startable, and a dangling parent id is a malformed plan rather than an unmet
dependency. Two further rules, both pinned by tests:

- `no-ancestor-branch` is reported **only when no direct parent is the blocker**. A branchless non-merged
  direct parent already makes the base `no-ancestor-branch`, so emitting both states one fact twice; the
  blocker therefore appears only when the block is *above* a merged parent, which is the case
  `parent-has-no-branch` cannot express.
- A `parents` cycle must terminate — this runs on every render, so a malformed `stackPlanJson` must not be
  able to hang the screen.

`resolveRepointTarget` returns the first direct parent branch that can serve as a base right now
(non-merged, owns a `branch`, and whose `remote.exists` is not `false`), else `defaultBranch`. This is the
web-side statement of the daemon's retain rule: a target no parent owns means every parent is dropped and
the base collapses to the default branch.

**Row rendering (web).** `PlannedPrRow` always renders title, description, owned branch or planned branch,
**base branch** (`pr-stack-base-branch-<nodeId>`), worktree, PR link/state and internal-status badge. The
CTA slot holds either the spawned status chip or the Start-session button; the button is `disabled` when
blockers exist, with the joined messages as its `title`. A warning strip
(`pr-stack-start-warning-<nodeId>`) lists the blocker messages. `pr-stack-missing-branch-<nodeId>` is
removed. The Repoint control reads `Repoint to <target>`, or `Repoint to default branch` when the target
resolves to an empty default.

**Default branch (web).** `SessionMainPane` resolves `projects.find(p => p.projectId ===
selectedSession.projectId)?.mainBranchRef ?? ""` into `WorkflowViewContext.defaultBranch`;
`resolveWorkflowView` passes it to `PrStackScreen`, which uses it for the Repoint target and for
`baseBranchLabel`.

### Delta

| Area | Change |
|---|---|
| `connection.proto` | `+ string target_base_branch = 4;` on `RepointPlannedPrRequest` |
| `pr_stack/mod.rs` | `repoint_planned_pr_node` `+ target_base_branch: Option<&str>`; retain-by-target rule; plan-only branch for `branch == None` |
| `connection_service.rs` | new pure `validate_repoint_target`; `repoint_planned_pr` validates + forwards |
| `startBlockers.ts` | **new** — `StartBlocker`, `startBlockers`, `resolveRepointTarget` |
| `PlannedPrList.tsx` | `blockers` + `repointTarget` replace `baseBranchMissing` + `baseBranch`; `canRepoint` also true when blockers exist |
| `PlannedPrRow.tsx` | unconditional full information; base-branch line; warning strip; disabled-not-replaced CTA; targeted Repoint label |
| `PrStackScreen.tsx` | `defaultBranch` prop; `targetBaseBranch` on the `repointPlannedPr` call; real `baseBranchLabel`; per-node repoint error state |
| `workflowViews.tsx`, `SessionMainPane.tsx` | thread `defaultBranch` from the loaded projects |
| `testIds.ts`, `prStackScreenPage.ts` | `prStackStartWarning` + `prStackBaseBranch` + `prStackRepointError` in, `prStackMissingBranch` out |

## Implementation Milestones

- [x] `RepointPlannedPrRequest.target_base_branch` added and TS regenerated
- [x] `repoint_planned_pr_node` honours an explicit target and repoints a branchless node plan-only
- [x] `validate_repoint_target` written; `repoint_planned_pr` validates the target and forwards it
- [x] A refused repoint surfaces the daemon's reason on the row and leaves it blocked
- [x] `startBlockers.ts` written with `startBlockers` + `resolveRepointTarget`
- [x] `PlannedPrRow` renders full information, a base-branch line, a warning strip, and a disabled CTA
- [x] Repoint control offered for any unresolvable base and labelled with its target
- [x] `defaultBranch` threaded `SessionMainPane` → `resolveWorkflowView` → `PrStackScreen`
- [x] Root node's Start-session dialog names its base branch
- [x] Test-support ids and page-object helpers moved to the new contract
      (`prStackBaseBranch`, `prStackStartWarning` in; `prStackMissingBranch` out)
- [x] `PrStackMissingBranchAcceptance.cy.tsx` migrated to the disabled-CTA + warning contract
- [x] `PrStackOrphanedNodeAcceptance.cy.tsx` migrated (one assertion; spec stays green)
- [x] `pr_stack_repoint_acceptance.rs` call sites pass `target_base_branch = None`
- [x] `cargo clippy -- -D warnings` and `bun run build` clean

## Implementation Evidence

**Phase: green complete and validated. 47 of 47 tests pass.** Implementation was delegated to two
`tdd-implementer` agents on disjoint file sets (proto + Rust, and `packages/tddy-web/src/`), then put
through a validation pass that found and fixed four real defects — see
[Validation Results](#validation-results).

### Production code

| File | Change |
|---|---|
| `packages/tddy-service/proto/connection.proto` | `+ string target_base_branch = 4;` on `RepointPlannedPrRequest` |
| `packages/tddy-web/src/gen/connection_pb.ts` | regenerated via the `generate` script (`buf generate ../tddy-service/proto`), not hand-edited |
| `packages/tddy-workflow-recipes/src/pr_stack/mod.rs` | `repoint_planned_pr_node` gains `target_base_branch: Option<&str>`; the retain decision is made **inside** the `update_stack_atomic` closure; the git/PR block moved inside `if let Some(branch) = node.branch` |
| `packages/tddy-daemon/src/connection_service.rs` | new pure `pub fn validate_repoint_target`; `repoint_planned_pr` substitutes the resolved default branch for an empty wire target, then validates and forwards |
| `packages/tddy-web/src/components/sessions/prstack/startBlockers.ts` | **new** — `BranchRemoteState`, `StartBlocker`, `startBlockers`, `resolveRepointTarget` |
| `PlannedPrRow.tsx` | full information unconditionally + base-branch line, warning strip, disabled CTA with the blockers as its tooltip, repoint error, in-flight-disabled Repoint control |
| `PlannedPrList.tsx` | `startBlockers` / `resolveRepointTarget`; `canRepoint` widened; `defaultBranch`, `repointErrorByNodeId`, `repointingNodeIds` props |
| `PrStackScreen.tsx` | `defaultBranch` prop; sends `targetBaseBranch`; catches failures per node; re-entrancy guard on repoint |
| `workflowViews.tsx`, `SessionMainPane.tsx` | `defaultBranch` threaded from the drawer's already-loaded `projects` |
| `package.json`, `packages/tddy-web/package.json` | `test:unit` wired into the `test` scripts (see Validation Results) |

### Test results

| Suite | Result |
|---|---|
| `pr_stack_repoint_dead_end_acceptance` | **7/7** |
| `pr_stack_repoint_acceptance` (existing, no-target mode) | **2/2** |
| `repoint_target_validation_acceptance` | **8/8** |
| `startBlockers.test.ts` | **14/14** |
| `PrStackRepointDeadEndAcceptance.cy.tsx` | **14/14** |
| `PrStackMissingBranchAcceptance.cy.tsx` | **6/6** |
| `PrStackOrphanedNodeAcceptance.cy.tsx` | **5/5** |

Wider: web unit set **477/477** · six PR-stack Cypress specs **39/39** · `cargo fmt --all --check` clean ·
`clippy -p tddy-workflow-recipes -p tddy-daemon --all-targets -D warnings` clean · `bun run build` clean.

`cargo test --workspace --no-fail-fast` reports 10 failing targets, all environmental and pre-existing:
`tddy-daemon` sandbox/cgroups (this host forbids unprivileged user namespaces), `tddy-integration-tests`
ACP backends (external binaries), `tddy-sandbox-darwin` (macOS-only), `tddy-sandbox-recipes --lib`.
`tddy-daemon --test terminal_control_acceptance` also appeared in that list but passes 10/10 in isolation
— that run overlapped concurrent edits and is not trustworthy for the web-adjacent targets.

## Validation Results

Four defects were found after the implementation reported green. Two were mine by design, not the
implementers'.

### [CRITICAL, fixed] An empty default branch made the whole recovery a silent no-op

On a project with no stored `main_branch_ref`, `resolveRepointTarget` returns `""`, which the web sent as
`target_base_branch`. The daemon read empty as *"no target named"* and selected the drop-merged-parents
rule — which, in the exact dead-end case this feature exists for (predecessor still recorded `open`),
drops nothing. The RPC returned success against an unchanged plan: no error, no change, row still blocked.

D20 had claimed "the daemon resolves the real ref at click time". It did not. The test for that path
asserted only the button's **label** and never clicked it.

Fixed by substituting the daemon's resolved `default_branch` for an empty wire target
(`connection_service.rs`), making the recipe's no-target mode in-process only. D20 and the proto comment
now say so.

### [WARNING, fixed] Repoint had no re-entrancy guard

`handleRepoint` could be entered twice and the control was never disabled, so a double-click ran two
rebase + `force_push_with_lease` + `patch_pr_base` sequences against one branch. Pre-existing, but D17
took the control from rare to present on every stranded row. Now guarded by `repointingNodeIds` plus a
`disabled` control.

### [WARNING, fixed] The retain decision was computed against a stale snapshot

`retained_parents` was built from the changeset read *before* `update_stack_atomic`, which re-reads the
file before applying its closure — and the orchestrator agent writes that same file. Converting a
drop-list into a keep-list across that boundary inverted the behaviour for any parent added between the
two reads. The filter now runs inside the closure, against the stack about to be written.

### [WARNING, resolved as intended behaviour] Multi-parent collapse

The retain rule keeps only parents owning the target, so a node with `[merged A, healthy B, healthy C]`
comes out of a repoint with B alone — master dropped only A. Raised as a possible regression; confirmed
by the developer as the **intended** semantic: repointing is a decision to stack on one predecessor, so
the node becomes single-parent. Now stated in D18 and pinned by
`repointing_collapses_a_multi_parent_node_onto_the_single_target_parent`.

### Test-quality pass

13 audit items applied. Most valuable: a mutation check showed `resolveRepointTarget`'s
"unarrived resolution is unknown, not absent" rule was **unpinned** (flipping `=== false` to `!== true`
passed every test) — now covered. Also added: no-GitHub-call assertion for the plan-only path, the
`origin/` prefix equivalence in both directions, the disabled button's tooltip, multi-blocker rendering,
D17's "any cause", and the spawned-node warning suppression. Two tests were split for asserting two
behaviours; several loose matchers tightened to `have.text`.

**One requested test could not be written.** "A blocked row keeps its PR link" is unsatisfiable: a
blocked node owns no branch by construction, branch is the join key for the PR leg, so a blocked row can
never have a PR link. The PRD's full-information claim was corrected rather than the code changed.

### Coverage gate

`startBlockers.test.ts` — and every unit test under `src/components/` — was reachable only through
`test:unit`, which **nothing invoked**. There is no CI workflow in this repo; the gate is `bun run test`.
Both the root and package `test` scripts now run `test:unit` (477 tests, 2.5s).

## Testing Plan

### Test level

Three levels, each chosen for what only it can pin:

- **Rust acceptance (`tddy-workflow-recipes/tests/`)** for the repoint *effect*. The retain-by-target rule
  and the plan-only path are decisions about what is written to `Changeset.stack` and what git/GitHub calls
  are made. A `tempfile::tempdir` plus the existing `FakeGithub` (see
  `pr_stack_repoint_acceptance.rs:23-76`) makes both observable with no repository and no network:
  `local_branch_exists` is false in a bare tempdir, so the rebase is deterministically skipped, and
  `FakeGithub::patched_bases` records every PR re-target — including its **absence**, which is the whole
  point of the plan-only case.
- **Cypress component (`tddy-web/cypress/component/`)** for the operator contract. "Full information is
  never hidden", "the button is disabled, not replaced", "the label names the target" and "the node becomes
  startable after repointing" are all statements about rendered DOM across a real RPC round trip. The
  in-memory backend (`mountWithRpc` + `aSessionsDrawerBackend`) drives `queryBranch`, `listProjects` and
  `repointPlannedPr` together, which is the only level where the whole path is exercised.
- **`bun:test` unit (colocated)** for `startBlockers.ts`. The blocker matrix has more cases than it is
  worth mounting a screen for (owned branch, unarrived resolution, root, multi-parent precedence), and it
  is a pure function — the same reasoning that put `deriveStackBaseBranch.test.ts` and
  `isNodeOrphaned.test.ts` next to their modules.

### Testing options considered

| Option | Trade-off | Verdict |
|---|---|---|
| Daemon integration test for `repoint_planned_pr` end to end | Would need a real git repository with an `origin` remote and a GitHub double behind `RealGithubPrApi`; `resolve_default_integration_base_ref` runs `git fetch origin`, so the test either hits the network or needs a local bare-repo fixture | **Rejected** — that much fixture buys nothing the recipe-level and web-level tests do not already cover |
| Leave the target validation untested because the handler is hard to test | One less file | **Rejected on review** — this was the first conclusion and it was wrong. It conflated "the handler is hard to test" with "the rule is hard to test". The repo already has the pattern for exactly this: `effective_spawn_branch` is a pure `pub fn` in `connection_service.rs` with its own `packages/tddy-daemon/tests/effective_spawn_branch_acceptance.rs`. `validate_repoint_target` gets the same treatment, and the rule it encodes is load-bearing — "no parent owns this branch" *is* the detach instruction, so an unvalidated target is a silent plan rewrite |
| Assert on `resolveRepointTarget` only through the rendered button label | One less test file | **Rejected** — the target rule has four distinct outcomes (surviving sibling, all-dead → default, merged parent skipped, root); mounting a screen per outcome is slow and the failure message points at a DOM string rather than the rule |
| Keep `pr-stack-missing-branch-<nodeId>` alongside the new warning | No churn in the existing spec | **Rejected** — two test ids for one concept, and the existing spec's `startSessionBtn(...).should("not.exist")` assertions encode the behaviour being reversed. They must change to state the new contract, not be worked around |
| Have the daemon re-derive dead parents from `remote_branch_ref_sha` instead of taking a target | No proto change | **Rejected** — see D18: every probe failure collapses to `None`, so the daemon would drop a live dependency whenever git was unavailable, and it would break `pr_stack_repoint_acceptance.rs`, whose `repo_root` is a bare tempdir where *every* branch reads as absent |

### Coverage requirements

- Every `StartBlocker` kind produced and asserted by message.
- Every `resolveRepointTarget` outcome: surviving sibling branch, all-parents-unusable → default,
  merged-parent skipped, root node.
- Both `repoint_planned_pr_node` target modes (`Some` / `None`) and both branch states
  (owns a branch / plan-only).
- Every existing suppression rule re-asserted after the rewrite: owned branch, unarrived resolution, root.
- Every `validate_repoint_target` outcome: empty, whitespace-only, the default branch with and without its
  `origin/` prefix, a parent's branch, and both refusal paths.
- Malformed-plan defensiveness: a dangling parent id, and a `parents` cycle (must terminate).
- The refusal path end to end in the web: the reason is shown, and the row stays blocked because nothing
  was persisted.

## Acceptance Tests

### `packages/tddy-web/cypress/component/PrStackRepointDeadEndAcceptance.cy.tsx` *(new)*

| Test | Validates |
|---|---|
| `offers "Repoint to <default branch>" when the base branch was deleted from origin` | The reported case: the plan still says the predecessor is open, its branch is gone from `origin`, and the row now carries a named recovery action |
| `keeps the planned PR's title, description, planned branch, base branch and PR link on a row that cannot be started` | D16 — blocked never means hidden |
| `disables Start session and warns that the base branch is not on origin` | The CTA is disabled with a reason instead of replaced |
| `sends the named target branch when Repoint is clicked` | D18 — the request carries the `targetBaseBranch` the label promised |
| `makes the node startable once the repoint has dropped its dead parent` | The recovery is durable within the session: the response's `stack_plan_json` re-renders the row unblocked with no warning |
| `names the surviving parent's branch as the Repoint target when only one of two parents is dead` | The target rule is "nearest usable ancestor", not "always the default branch" |
| `reads "Repoint to default branch" when the project records no default branch` | D20 — a legacy project degrades the label only, never the action |
| `offers no Repoint control on a node whose base branch is on origin` | The control does not become ambient noise on a healthy stack |
| `surfaces the daemon's reason when a repoint is refused` | The new `invalid_argument` path is reachable; a refusal the operator cannot see is a fresh dead end |
| `leaves the row blocked when the repoint was refused` | No optimistic clearing — nothing was persisted, so the row must not read as recovered |

### `packages/tddy-web/cypress/component/PrStackMissingBranchAcceptance.cy.tsx` *(migrated)*

The six existing cases keep their scenarios and switch to the new contract — `startSessionBtn(...)`
asserted `disabled` / `enabled` rather than `not.exist` / `exist`, and `startWarning` in place of
`missingBranch`.

### `packages/tddy-workflow-recipes/tests/pr_stack_repoint_dead_end_acceptance.rs` *(new)*

| Test | Validates |
|---|---|
| `repoints a branchless planned node by dropping every parent that does not own the target` | D18 + D19 together: the plan-only path for the node this recovery exists for |
| `repointing a branchless node re-targets no pull request` | D19 — no `patch_pr_base` for a node with no branch (asserted on `FakeGithub::patched_bases` being empty) |
| `retains the parent that owns the target base branch` | The target rule is a retain rule, not "drop everything" |
| `drops a parent whose pull request is not recorded as merged when it does not own the target` | The reported case at the data layer: a stale `pr_status.phase == "open"` no longer protects a dead parent |
| `drops merged parents only when no target base branch is given` | The `None` mode is unchanged for existing callers |

### `packages/tddy-web/src/components/sessions/prstack/startBlockers.test.ts` *(new, `bun:test`)*

| Test | Validates |
|---|---|
| `reports no blockers for a root node` | The default branch exists by construction |
| `reports no blockers for a node that already owns a branch` | Its spawn resumes the branch — creates nothing, fetches nothing |
| `reports no blockers while the base branch resolution has not arrived` | Unknown is not missing; polls are swallowed |
| `names the base branch that is absent from origin` | `base-branch-not-on-origin` and its message |
| `names the parent that has not created its branch yet` | `parent-has-no-branch` and its message |
| `reports that no predecessor owns a branch yet` | `no-ancestor-branch` and its message |
| `reports the unmet parent ahead of a base branch that is merely unresolved` | Blocker precedence — the unmet dependency is the actionable one |
| `resolves the repoint target to the default branch when no parent can serve as a base` | The reported case's target |
| `resolves the repoint target to the surviving parent's branch when one parent is dead` | Nearest usable ancestor wins |
| `resolves the repoint target past a merged parent to the next usable one` | The original merged-parent repoint keeps its target |
| `resolves the repoint target to the default branch for a root node` | No parents at all |
| `ignores a parent id that matches no node in the stack` | A malformed plan is not an unmet dependency — the daemon's gate takes the same stance |
| `terminates on a parent cycle rather than recursing forever` | This runs on every render; a bad `stackPlanJson` must not hang the screen |

### `packages/tddy-daemon/tests/repoint_target_validation_acceptance.rs` *(new)*

| Test | Validates |
|---|---|
| `an_empty_target_selects_the_drop_merged_parents_rule` | `None` is the original behaviour, not a rejection |
| `a_whitespace_only_target_is_no_target_at_all` | A blank field is not a branch literally named `"   "` |
| `the_resolved_default_branch_is_accepted` | The reported case's target |
| `the_default_branch_is_accepted_without_its_origin_prefix` | The resolver returns `origin/master`; a PR base and a node's `branch` are plain names |
| `a_parents_own_branch_is_accepted` | The surviving-sibling target |
| `a_target_that_is_neither_the_default_branch_nor_a_parents_branch_is_rejected` | An unvalidated target silently detaches the node |
| `a_parents_branch_is_rejected_once_that_parent_is_no_longer_a_parent` | A stale label after an earlier repoint |

## Technical Debt & Production Readiness

No `TODO` or `FIXME` annotations exist in either touched area
(`packages/tddy-web/src/components/sessions/prstack/`, `packages/tddy-workflow-recipes/src/pr_stack/`).
Open items as of the red phase:

- ~~`docs/dev/TODO.md` records "Repoint availability still derives from the stored `pr_status`"~~ —
  resolved by D17 and **removed** from `TODO.md` in this changeset.
- **`origin/<branch>` freshness still depends on the last fetch** (existing `TODO.md` entry, untouched
  here). It now has a second consequence: a branch pushed from another machine reads as absent, so the
  row offers "Repoint to `origin/master`" for a base that is actually alive. The warning is still
  conservative — it can only delay a spawn — but the *repoint* is destructive, since it drops the parent
  edge from the plan. A fetch-on-demand from the row would close this properly; until then the
  operator-driven, target-naming control is the mitigation.
- **Repointing an unstarted predecessor detaches a real dependency** (accepted, D17). Not recoverable
  from the UI — re-adding the parent edge needs a new planned-PR edit path, which is out of scope here.
- **The `None` target mode stays reachable** for callers that do not name a target. Nothing in the web
  sends it after this change; it exists so the existing repoint semantics remain available and its
  acceptance tests keep meaning. Worth revisiting once the agent path is confirmed to be the only user.

## Decisions & Trade-offs

The full table lives in
[pr-stack-live-status.md § Design decisions](../../ft/coder/pr-stack-live-status.md#design-decisions)
(D16–D20). The load-bearing ones and what they cost:

- **D16 reverses D10.** The blocked row keeps everything and disables its button. D10's objection —
  "a disabled control with no explanation is a dead end" — is answered by the warning naming each issue
  *and* by Repoint sitting beside it. What D10 got wrong was suppressing the row's own information to make
  room for the explanation.
- **D17 stops gating recovery on the plan's `pr_status`.** That field is agent-written and stale in the
  exact case being fixed. Cost: Repoint is now also offered when a predecessor simply has not started
  yet, where clicking it detaches a real dependency. Accepted deliberately — the control names its target,
  so it is an explicit operator choice, and the alternative (offer it only for provably
  merged-and-deleted bases) leaves the indistinguishable causes with no recovery at all.
- **D18 puts the target in the request.** The alternative — the daemon re-deriving dead parents from git —
  cannot distinguish "absent from `origin`" from "could not tell", because `remote_branch_ref_sha`
  collapses every failure to `None`. It would drop live dependencies on a probe failure and would break
  the existing repoint acceptance tests, whose `repo_root` is a bare tempdir.
- **D19 makes a branchless repoint plan-only** rather than an error. Cost: none — there is nothing to
  rebase and no PR to re-target.
- **D20 reads the default branch from the project list the drawer already holds.** Cost: a legacy project
  with no stored `main_branch_ref` shows "Repoint to default branch" instead of a name. The action is
  identical; the daemon resolves the real ref at click time. Rejected alternatives: a new RPC (a whole
  round trip for a label) and a live probe per render (`resolve_default_integration_base_ref` runs
  `git fetch origin`).

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [x] Run all tests — every test for this changeset passes; the only workspace failures are the
      documented environmental ones (sandbox/cgroups, ACP binaries, macOS-only)
- [x] Validate changes
- [ ] USER REVIEW — development complete
- [x] Linting and type checking
- [ ] Wrap documentation
- [ ] USER REVIEW — work complete, decide next steps
