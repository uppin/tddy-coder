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
  - `connection_service.rs` — `repoint_planned_pr` validates `target_base_branch` against the resolved
    default branch and the node's parents' branches, then passes it through.
- **Web** (`packages/tddy-web/src/`): a blocked row keeps its information and gains a real action.
  - `components/sessions/prstack/startBlockers.ts` — **new** pure module: `startBlockers` and
    `resolveRepointTarget`.
  - `components/sessions/prstack/PlannedPrRow.tsx` — always renders full information; Start-session is
    disabled rather than replaced; new base-branch line and warning strip; Repoint control names its
    target.
  - `components/sessions/prstack/PlannedPrList.tsx` — computes blockers and the repoint target instead of
    the `baseBranchMissing` / `baseBranch` pair; widens `canRepoint`.
  - `components/sessions/prstack/PrStackScreen.tsx` — new `defaultBranch` prop; sends
    `targetBaseBranch`; uses the default branch for `baseBranchLabel`.
  - `components/sessions/workflowViews.tsx` — `WorkflowViewContext.defaultBranch`.
  - `components/sessions/SessionMainPane.tsx` — resolves the default branch from the already-loaded
    `projects` by `session.projectId`.
- **Web test support** (`packages/tddy-web/cypress/support/`):
  - `testIds.ts` — `prStackStartWarning`, `prStackBaseBranch`; `prStackMissingBranch` removed.
  - `pages/prStackScreenPage.ts` — `startWarning`, `baseBranch`; `missingBranch` removed.
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
- [ ] **Implementation**: proto field, recipe target rule + plan-only repoint, daemon validation, web
      blockers/target/threading
- [~] **Testing**: 24 acceptance/unit tests written and confirmed failing for the right reasons; none
      passing yet by design (see [Implementation Evidence](#implementation-evidence))
- [ ] **Integration**: verified against a live stack with a merged-and-deleted predecessor branch
- [ ] **Technical Debt**: production readiness gaps addressed
- [ ] **Code Quality**: `cargo clippy -- -D warnings` clean, `bun run build` clean

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

**Daemon.** `repoint_planned_pr` rejects (`invalid_argument`) a non-empty `target_base_branch` that names
neither the resolved default branch (with or without an `origin/` prefix) nor any of the node's parents'
branches, then forwards `Some(target)` / `None`.

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
has no blockers (its spawn resumes), an unarrived resolution is *unknown* rather than missing, and a root
or all-merged node is startable.

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
| `connection_service.rs` | validate + forward `target_base_branch` |
| `startBlockers.ts` | **new** — `StartBlocker`, `startBlockers`, `resolveRepointTarget` |
| `PlannedPrList.tsx` | `blockers` + `repointTarget` replace `baseBranchMissing` + `baseBranch`; `canRepoint` also true when blockers exist |
| `PlannedPrRow.tsx` | unconditional full information; base-branch line; warning strip; disabled-not-replaced CTA; targeted Repoint label |
| `PrStackScreen.tsx` | `defaultBranch` prop; `targetBaseBranch` on the `repointPlannedPr` call; real `baseBranchLabel` |
| `workflowViews.tsx`, `SessionMainPane.tsx` | thread `defaultBranch` from the loaded projects |
| `testIds.ts`, `prStackScreenPage.ts` | `prStackStartWarning` + `prStackBaseBranch` in, `prStackMissingBranch` out |

## Implementation Milestones

- [ ] `RepointPlannedPrRequest.target_base_branch` added and TS regenerated
- [ ] `repoint_planned_pr_node` honours an explicit target and repoints a branchless node plan-only
- [ ] `repoint_planned_pr` validates the target and forwards it
- [ ] `startBlockers.ts` written with `startBlockers` + `resolveRepointTarget`
- [ ] `PlannedPrRow` renders full information, a base-branch line, a warning strip, and a disabled CTA
- [ ] Repoint control offered for any unresolvable base and labelled with its target
- [ ] `defaultBranch` threaded `SessionMainPane` → `resolveWorkflowView` → `PrStackScreen`
- [ ] Root node's Start-session dialog names its base branch
- [x] Test-support ids and page-object helpers moved to the new contract
      (`prStackBaseBranch`, `prStackStartWarning` in; `prStackMissingBranch` out)
- [x] `PrStackMissingBranchAcceptance.cy.tsx` migrated to the disabled-CTA + warning contract
- [x] `PrStackOrphanedNodeAcceptance.cy.tsx` migrated (one assertion; spec stays green)
- [x] `pr_stack_repoint_acceptance.rs` call sites pass `target_base_branch = None`
- [ ] `cargo clippy -- -D warnings` and `bun run build` clean

## Implementation Evidence

**Phase: red complete, no production code written yet.** Every test below fails, and the failure of each
was read and confirmed to be the absence of the feature rather than a mistake in the test.

### Tests written

| File | Count | Status |
|---|---|---|
| `packages/tddy-workflow-recipes/tests/pr_stack_repoint_dead_end_acceptance.rs` *(new)* | 5 | ✗ all fail |
| `packages/tddy-web/src/components/sessions/prstack/startBlockers.test.ts` *(new, `bun:test`)* | 11 | ✗ all fail |
| `packages/tddy-web/cypress/component/PrStackRepointDeadEndAcceptance.cy.tsx` *(new)* | 8 | ✗ 7 fail, 1 guard passes |
| `packages/tddy-web/cypress/component/PrStackMissingBranchAcceptance.cy.tsx` *(migrated)* | 6 | ✗ 3 fail, 3 already hold |
| `packages/tddy-web/cypress/component/PrStackOrphanedNodeAcceptance.cy.tsx` *(migrated)* | 5 | ✓ 5 pass |

### Confirmed failure reasons

- **Rust (5/5)** — `error[E0061]: this function takes 5 arguments but 6 arguments were supplied`
  against `pr_stack/mod.rs:457`. The only error class in the target, so the fixtures and the
  `FakeGithub` double are sound.
- **`bun:test` (11/11)** — `Cannot find module './startBlockers'`.
- **Cypress `PrStackRepointDeadEndAcceptance` (7/8)** — `pr-stack-repoint-n2` never found (5 cases: the
  control is still gated on the plan's recorded `merged` phase), `pr-stack-base-branch-n2` never found
  (1), and `pr-stack-start-session-n2` **never found** (1) — the button is replaced, not disabled, which
  is D16's defect stated as an assertion.
- **Cypress `PrStackMissingBranchAcceptance` (3/6)** — `pr-stack-start-warning-<nodeId>` never found.

### Tests that pass today, deliberately

Four assertions already hold and are kept as regression guards, not as coverage of new behaviour:
`PrStackRepointDeadEndAcceptance.cy.tsx:255` (no Repoint control on a healthy base) and the three
not-blocked cases in `PrStackMissingBranchAcceptance.cy.tsx` (`:124`, `:205`, `:217`).

### Fixture verification

A module-not-found red cannot show whether the 11 unit-test bodies are right, so the three riskiest
fixtures were probed against the **real** `resolveStackBase` / `branchlessNonMergedParent` before being
committed. One was wrong and was rebuilt: an all-merged chain resolves to `default-branch` (startable),
not `no-ancestor-branch`, so the no-ancestor case is now modelled as a branchless non-merged ancestor
*above* a merged parent — confirmed to yield `{kind:"no-ancestor-branch"}` with **no** blocking direct
parent, which is what makes it a distinct blocker rather than a duplicate of `parent-has-no-branch`.

### Environment notes

- This worktree had no `node_modules`; `bun install` was run at the repo root (no `bun.lock` change).
- `cypress` is not on `PATH` inside `./dev`. Working invocation:
  `./dev bash -c 'cd packages/tddy-web && CYPRESS_DISABLE_REACT_COMPILER=1 ELECTRON_EXTRA_LAUNCH_ARGS="--disable-gpu --no-sandbox" bun x cypress run --component --spec <spec>'`

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
| Daemon integration test for `repoint_planned_pr` end to end | Would need a real git repository with an `origin` remote and a GitHub double behind `RealGithubPrApi`; `resolve_default_integration_base_ref` runs `git fetch origin`, so the test either hits the network or needs a local bare-repo fixture | **Rejected** — the target validation is the only daemon-side logic, and it is a pure string check better covered by the recipe-level and web-level tests than by that much fixture |
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

## Technical Debt & Production Readiness

No `TODO` or `FIXME` annotations exist in either touched area
(`packages/tddy-web/src/components/sessions/prstack/`, `packages/tddy-workflow-recipes/src/pr_stack/`).
Open items as of the red phase:

- **`docs/dev/TODO.md` records "Repoint availability still derives from the stored `pr_status`, not the
  live poll"** — that entry is exactly what D17 fixes. It should be **removed** from `TODO.md` during the
  wrap, not carried forward.
- **`origin/<branch>` freshness still depends on the last fetch** (existing `TODO.md` entry, untouched
  here). It now has a second consequence: a branch pushed from another machine reads as absent, so the
  row offers "Repoint to `origin/master`" for a base that is actually alive. The warning is still
  conservative — it can only delay a spawn — but the *repoint* is destructive, since it drops the parent
  edge from the plan. A fetch-on-demand from the row would close this properly; until then the
  operator-driven, target-naming control is the mitigation.
- **Repointing an unstarted predecessor detaches a real dependency** (accepted, D17). Not recoverable
  from the UI — re-adding the parent edge needs a new planned-PR edit path, which is out of scope here.
- **The daemon's target validation is not covered by a test.** It is a pure string check
  (`target` ∈ {resolved default branch, any parent's branch}) and the testing plan deliberately rejected
  a daemon integration test for it (see [Testing options considered](#testing-options-considered)). If it
  grows past a string check it needs its own coverage.
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
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests — verify 100% pass
- [ ] Validate changes
- [ ] USER REVIEW — development complete
- [ ] Linting and type checking
- [ ] Wrap documentation
- [ ] USER REVIEW — work complete, decide next steps
