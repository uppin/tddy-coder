# 2026-07-26 — pr-stack-repoint-dead-end

**Type:** Fix · **Branch:** `fix-repoint-to-master`
**Packages:** `tddy-service`, `tddy-workflow-recipes`, `tddy-daemon`, `tddy-web`
**Index line:** [docs/dev/changesets.md](../changesets.md)
**Features:** [pr-stack-live-status.md § Repointing a dead-end planned PR](../../ft/coder/pr-stack-live-status.md#repointing-a-dead-end-planned-pr-added-2026-07-26) ·
[session-drawer.md § PR-Stack Chat Screen](../../ft/web/session-drawer.md#pr-stack-chat-screen)
**Per-package:** [tddy-service](../../../packages/tddy-service/docs/changesets.md) ·
[tddy-workflow-recipes](../../../packages/tddy-workflow-recipes/docs/changesets.md) ·
[tddy-daemon](../../../packages/tddy-daemon/docs/changesets.md) ·
[tddy-web](../../../packages/tddy-web/docs/changesets.md)

## What was broken

A planned PR whose predecessor's PR merged and whose branch was then deleted on `origin` — the normal
GitHub flow, with "automatically delete head branches" on — was **unrecoverable**. Three things had to be
wrong at once, and all three were:

1. **The row replaced itself with an error.** [`pr-stack-ux-recovery`](2026-07-26-pr-stack-ux-recovery.md)
   chose (D10) to render a blocked "Missing branch: `<base>`" chip **in place of** the Start-session
   button. The reasoning was sound — a disabled control with no explanation is a dead end — but the row is
   the only place a planned PR's title, description, planned branch and PR live, and the operator lost the
   CTA with no replacement action. The row never rendered its base branch at all, so *which* branch was
   missing was legible only inside the chip's own text.
2. **The recovery was gated on a field that is stale in exactly this case.** Repoint appeared only when
   some parent's `pr_status.phase == "merged"` — a field the orchestrator agent writes during an assess
   pass. Merge a PR on GitHub without running the agent and the plan still says `open`, so no control
   appeared.
3. **The recovery would have refused anyway.** `repoint_planned_pr_node` errored with
   `node '<id>' has no branch to repoint` for a node that was never started — precisely the node stranded
   behind the deleted base.

A fourth, smaller gap fell out of the same area: `PrStackScreen` passed `""` as the default branch to
`deriveStackBaseBranch`, so a **root** node's Start-session dialog read "New branch from base:" with no
name.

## What was decided

The full table (D16–D20) is in
[pr-stack-live-status.md § Design decisions](../../ft/coder/pr-stack-live-status.md#design-decisions).
The load-bearing ones:

- **A blocked row is a full row (D16, reverses D10).** Everything the node *has* renders regardless of
  startability, plus a new base-branch line; the Start-session button is **disabled** with the blocking
  reasons as its tooltip, beside a warning listing them. D10's objection is answered by the warning *and*
  by Repoint sitting next to it — what D10 got wrong was suppressing the row's own information to make
  room for the explanation. (A blocked node necessarily owns no branch, and branch is the join key for the
  worktree and PR legs, so in practice a blocked row shows title, description, planned branch and base.)
- **Repoint is offered for any unresolvable base (D17)**, not only a recorded merged parent. The causes —
  branch deleted after merging, deleted without merging, never pushed — are indistinguishable to the
  operator and all resolve the same way. Cost, accepted deliberately: the control also appears on a node
  whose predecessor simply has not started yet, where taking it detaches a live dependency.
- **The web computes the target and sends it (D18).** `RepointPlannedPrRequest.target_base_branch`; the
  daemon retains exactly the parents that own that branch. Having the daemon re-derive dead parents from
  git was **rejected**: `remote_branch_ref_sha` collapses every failure to `None`, so "absent from
  `origin`" and "could not tell" are the same value, and the daemon would drop a live dependency on any
  probe failure. The web is not inventing the fact either — it reads `BranchResolution.remote`, which the
  daemon resolved. A repoint therefore **collapses the node to a single parent**, by intent: repointing is
  a decision to stack on one predecessor.
- **A branchless node is a plan-only repoint (D19).** No rebase, no force-push, no PR re-target — there is
  nothing to rebase and no PR of its own.
- **The default-branch name comes from the project list the drawer already loads (D20)**, not a new RPC
  and not a live probe (`resolve_default_integration_base_ref` runs `git fetch origin`, and the label
  renders on every poll tick).

## What validation caught after the implementation reported green

Four defects, two of them errors in the design above rather than in the implementation.

- **An empty default branch made the whole recovery a silent no-op.** A project storing no
  `main_branch_ref` yields an empty target; the daemon read empty as "no target named" and selected the
  drop-merged-parents rule — which, in the very case this changeset exists for, drops nothing. Success
  response, unchanged plan, no error, row still blocked. D20 had claimed the daemon "resolves the real ref
  at click time"; it did not. The daemon now substitutes its own resolved default branch for an empty wire
  target, and the recipe's no-target mode is in-process only.
- **Repoint had no re-entrancy guard**, so a double-click ran two rebase + `force_push_with_lease` +
  `patch_pr_base` sequences against one branch. Pre-existing, but D17 took the control from rare to
  present on every stranded row.
- **The retain decision raced the atomic write.** It was computed from the changeset read *before*
  `update_stack_atomic`, which re-reads the file the orchestrator agent also writes; converting a
  drop-list into a keep-list across that boundary inverted the behaviour for a parent added in between.
  The filter now runs inside the closure.
- **`resolveRepointTarget`'s "unarrived resolution is unknown, not absent" rule was entirely unpinned** —
  mutating `=== false` to `!== true` passed all 41 tests then in the branch. The same class of bug as the
  first item, and only a mutation check surfaced it.

## Verification

47 tests for this changeset, all passing: `pr_stack_repoint_dead_end_acceptance` 7,
`pr_stack_repoint_acceptance` 2 (existing, no-target mode), `repoint_target_validation_acceptance` 8,
`startBlockers.test.ts` 14, `PrStackRepointDeadEndAcceptance.cy.tsx` 14,
`PrStackMissingBranchAcceptance.cy.tsx` 6 (migrated), `PrStackOrphanedNodeAcceptance.cy.tsx` 5 (migrated).
Wider: web unit set 477/477, six PR-stack Cypress specs 39/39, `cargo fmt --all --check` clean,
`clippy -D warnings` clean on both changed crates, `bun run build` clean.

`startBlockers.test.ts` — and every unit test under `packages/tddy-web/src/components/` — was reachable
only through `test:unit`, which **nothing invoked**; there is no CI workflow in this repo, so the gate is
`bun run test`. Both the root and package `test` scripts now run it (477 tests, 2.5 s).

**Not verified against a live stack.** Unlike `pr-stack-ux-recovery`, this changeset was not exercised
against a real orchestrator session with a merged-and-deleted predecessor branch; the developer elected to
verify after merge. Given that the empty-default-branch defect above was a path no test exercised end to
end, that is the first thing to check.

## Deliberately out of scope

- **Naming the parents a repoint drops.** The control says "Repoint to `<target>`" and collapses the node
  onto that one parent; it never says which edges go, and there is no undo. Filed in `docs/dev/TODO.md`.
- **`origin/<branch>` freshness.** A branch pushed from another machine reads as absent until this host
  fetches, so the row can offer a repoint away from a base that is actually alive — and unlike the earlier
  warning, taking it is destructive. Filed.
- **`packages/tddy-rust-typescript-tests/gen/connection_pb.ts`** is a second checked-in generation of the
  same proto and predates `RepointPlannedPr` entirely; regenerating it is a large unrelated diff. Filed.
- **Three pre-existing silent-failure paths in `repoint_planned_pr_node`** (an empty `expected_sha`
  turning `--force-with-lease` into a guaranteed rejection, an invented `merge_base` on error, and a
  force-push failure that only warns while the RPC returns success). Untouched here — this changeset only
  moved them inside a branch guard. Filed.
