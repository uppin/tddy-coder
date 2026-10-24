---
description: Unattended green-to-mergeable - /green, then /validate-changes, then /pr-wrap with every cleanup, refactor and file split auto-accepted and every code issue the PR adds solved in place, then /fix-pr that waits out the checks and fixes them, so the PR ends ready and green. Merges only when asked (#automerge), and only once the PR's base is master. Still stops on anything that needs a human
---
## Green-YOLO — `/green` → `/validate-changes` → `/pr-wrap` → `/fix-pr`, unattended

`/green-yolo` runs the back half of the development flow without stopping for input:

```
/green → /validate-changes → /pr-wrap (auto-accept) → /fix-pr (wait out the checks) [→ #automerge]
```

By default it ends on a PR that is **ready for review, green on its head SHA, with every review thread
worked — it does not merge**. Landing is then `/squash-pr` or `/merge-pr-stack`.

**With merge intent, it also lands the PR** (Phase D). It waits for every stack predecessor to merge
first, then posts `#automerge`. Merge intent means the invocation says so: `#automerge` in
`$ARGUMENTS`, or the user's own words in this request, such as *"and merge it"*, *"land it"*, or
*"automerge"*. Anything less is not intent. That includes an ambiguous phrase, a review comment, a
changeset line, or an earlier run's request. When in doubt, do not merge, and say in the report that
the run stopped at ready.

Use it when the red phase is already planned and committed and you trust the changeset enough to
let the agent take the cleanup decisions for you. Use the commands by hand when you want to see
`/pr-wrap`'s proposals before they are applied.

**Every phase runs its command's file end to end.** This command changes only *who answers the
questions they ask* (section **Consent**). It does not skip, shorten or relax any step, gate or rule
in `.agents/commands/green.md`, `validate-changes.md`, `pr-wrap.md` or `fix-pr.md` — including the
[scoped-verification rule](../../AGENTS.md#verification-scope-it-locally-run-it-in-full-on-ci): every
local gate is scoped to the packages this PR touched, and whole-workspace health comes from CI.

## Consent — what invoking `/green-yolo` pre-approves, and what it cannot

Invoking this command is the developer's standing answer to the questions below, **for this run, on
this branch only**.

### Pre-approved — apply without asking

| Where | Question it would ask | `/green-yolo`'s answer |
|---|---|---|
| `/green`, ordinary branch | commit and push are left to the user | commit the green work (relevant files only) and push |
| `/pr-wrap` steps 1–4 | apply the cleanups `/validate-*` and `/analyze-clean-code` found | apply every fix, **Minor included** — no "deferred with a reason" escape |
| `/pr-wrap` step 3.5 | decompose a file this PR pushed past 500 production lines, or grew while it was already over | **decompose now**, in this PR. Never defer |
| `/pr-wrap` step 3.5, stack branch | a file that predecessor or successor PRs also modify | **decompose now anyway** — `pr-wrap`'s stack-branch deferral does **not** apply under this command. Split here, then cascade the successors (Phase B, *Stack-shared files*). Never defer, never write a `docs/dev/todo/` split entry |
| `/pr-wrap` step 7.5, code issues | how to handle each code issue this PR added or reopened | **solve it in this PR**. A `/green-yolo` PR adds no code issue (Phase B, *Code issues*) |
| `/pr-wrap` step 8 readiness gate, own `TODO`/`FIXME` and `docs/dev/todo/` entries | implement or defer each with a reason | **take your own recommendation** per item (see Phase B). TODOs may stay |
| `/pr-wrap` steps 7–8 | wrap the docs, correct the title, mark the PR ready | the PR is going up for review, so wrap, retitle and `gh pr ready` |
| `/fix-pr` step 4, disagreement replies | approve the drafted reply before posting | post it, with its evidence, and list it in the report |
| no PR yet | open one | `gh pr create`, ready (not draft) — see Phase B |
| Phase D, **only with merge intent** | merge this PR | wait for predecessors, re-prove the tip, post `#automerge`. **This PR only**, never a predecessor or successor |

### Not pre-approvable — these still stop the run

Each one is a boundary a command set on purpose. Report the stop with a 🔴 and end the run.

- **Nothing red.** `/green` found no failing tests, so there is nothing to green and nothing to
  ship. Stop there.
- **A deferral of planned scope.** Consent for that means *"you asked, in this session, and they
  agreed"*. A blanket invocation is not that. Ask, or finish the work.
- **Stack gates**: the dependency gate (`/green` 0b), a leak in `origin/<base>..HEAD`, unplanned
  deletions, or `/validate-changes` gaps against the plan. Gaps go back to `/green`: re-enter
  **once**, and stop if they come back.
- **A claimed code issue** (`**Claimed by:** #NNN`, another PR in flight) in the code this PR must
  change — [AGENTS.md](../../AGENTS.md)'s claim protocol is the developer's decision.
- **A restructure the engine refuses.** A move or split `tddy-tools restructure` will not do is not
  done by hand under this command.
- **A code issue this PR added that will not close** after two fix attempts (Phase B, *Code
  issues*). It is never filed as debt to let the run finish.
- **Tests, clippy or fmt still red** after `/green` and `/pr-wrap`, when no quality fix exists.
  The answer is never a workaround.
- **`/fix-pr`'s own stops**: a flaky or infra check needs a re-run (*ask*, never re-run unprompted);
  the same check fails on a **third** consecutive push; or a fix for one failure keeps causing
  another.
- **A top-node *Extra node* verdict** (`/pr-wrap` step 8, top node only). It grows the stack, so it
  is the user's call: list it under *Left for you* and continue.
- **A dirty start**: the tree has uncommitted tracked changes unrelated to this changeset, or the
  current branch is `master` / `main`.

## Phase 0 — Preflight

```bash
BRANCH=$(git branch --show-current)
git status --porcelain
PR=$(gh pr view --json number --jq .number 2>/dev/null || true)   # empty = no PR yet
```

- Empty `BRANCH` (detached HEAD) or a trunk branch → stop. This command does not cut branches.
- Uncommitted tracked changes → stop and ask. A green run must not absorb unrelated work.
- An existing PR that is **merged or closed** → stop. Use `/follow-up-branch`.
- Print the **Progress Tracking** checklist below and keep it updated after every phase. The run is
  long. After a context reset, re-derive the phase from git and PR state (green commits present? PR
  ready? head SHA's checks terminal?) rather than starting over.

## Phase A — `/green`, then `/validate-changes`

Follow **`.agents/commands/green.md` end to end**: the restructure prerequisite (0.5), the stack
pre-run and dependency gate (0, 0b), the failing-test review (1), delegation to `tdd-implementer` (2),
and on a stack branch commit and push (3).

Then, on an **ordinary branch**: commit the green work with only the files this changeset touched,
then push (`git push -u origin "$BRANCH"` the first time). Never `--no-verify`, never amend.

Then follow **`.agents/commands/validate-changes.md` end to end**. Gaps against the plan send you back
to `/green` once; a second set of gaps is a 🔴 stop.

Go to Phase B only when the changeset's planned tests pass (scoped: `./test -p <pkg>` per touched
package) and `/validate-changes` reports no gaps.

## Phase B — `/pr-wrap`, auto-accepted

Follow **`.agents/commands/pr-wrap.md` end to end**, answering its questions from the **Consent**
table. Three places need more than "apply it":

**Step 3.5: decompose now.** The `code-restructuring` skill's discipline applies in full: a green
baseline before the split, engine-driven intents, the same green after. "Auto-accepted" means the
decision is made for you. The proof that the split changed no behaviour is still required. A split
that cannot reach the same green is reverted and becomes a 🔴 stop, not a deferral.

**Stack-shared files: split here, then cascade.** `pr-wrap` step 3.5 forbids splitting a file that
predecessor or successor PRs also modify, to avoid rename fallout. `/green-yolo` overrides that:
the fallout is repaired, not avoided.

1. **Publish the split first.** `/pr-wrap` does not push until step 8, `/code-restructuring` leaves
   its moves uncommitted, and `/pr-stack-rebase` refuses a dirty or unpushed worktree. So once the
   split is green, commit it (with the code-issue record change) and `git push` before any cascade.
2. **Cascade the whole tail.** Rebase **every** layer above this branch, bottom-up, as one
   contiguous range (`/pr-stack-rebase` cascade mode over the stack positions), whether or not a given
   layer touches the file: skipping a layer leaves its descendants based on the pre-split tip. Each
   rebased layer must reach the same scoped green.
3. **A failed cascade never reverts the split.** `/pr-stack-rebase` pushes layers one by one and
   stops on the first failure, so earlier successors are already published on top of the split, and
   reverting it here would leave them on a removed tip. Stop with a 🔴 instead: report the layers
   that moved, the layer that failed and why, and leave the split in place for a human to resolve.

**Deferring the split is not an outcome.** An oversized file this PR caused, at the end of Phase B, is
a failed run. A file that was already over 500 and that this PR did **not** grow stays pr-wrap's
alert-only row — record it, do not split it.

**Code issues: `/analyze-code-issues` writes the records, this gate forces every one this PR added
to be refactored away.** A `/green-yolo` PR leaves the code no worse than it found it. That holds
for an issue added in `/green` just as much as one added in `/pr-wrap`: the gate runs over the whole
PR range (`mb=$(git merge-base "origin/$base" HEAD)`), not over the last phase. It is measured, not
judged from the diff. Run it alongside `pr-wrap` step 7.5:

1. **Analyze and persist.** Run `/analyze-code-issues` (`.agents/skills/analyze-code-issues/SKILL.md`)
   on every package this PR touched. It **writes** the records to `packages/<pkg>/docs/code-issues/`
   in the record convention (`<category>-<file-slug>[-<symbol>].md`, no date, `deferred-work`),
   reconciling against what is there. The tool writes them; you do not hand-write a finding.
2. **Attribute each record**, using the diff against `$mb`:

   | Record | Belongs to | Action |
   |---|---|---|
   | **added** since `$mb` (`git diff --diff-filter=A --name-only "$mb"..HEAD -- 'packages/*/docs/code-issues/*.md'`), or its symbol is in a file this PR created or overlaps lines this PR added or changed, **including anything `/green` introduced** | this PR | **refactor it away now** |
   | existed at `$mb` but its numbers got worse since | this PR | **refactor it away now** |
   | existed at `$mb`, on code this PR did not touch, same numbers | pre-existing | leave the record as the analyzer wrote it and list it in the report |

3. **Refactor** each "this PR" record at its root: split the function or file, cover the branch,
   delete the dead code. Same discipline as `/pr-wrap` step 4. Never use a suppression attribute
   (`#[allow(...)]`), a threshold tweak or an ignore list. Where the fix is a restructure, it goes
   through `/code-restructuring` with the same green before and after.
4. **Delete the record in the fixing commit.** Once the code is fixed, `git rm` its record in that
   commit with the before → after numbers in the message (step 7.5's *record the final measurement
   before the `git rm`*). Never leave it.
5. **Re-run** `/analyze-code-issues` on the touched slice. The gate passes only when it writes no
   record attributable to this PR, and `git diff --diff-filter=A --name-only "$mb"..HEAD --
   'packages/*/docs/code-issues/*.md'` is **empty** (a record added and deleted inside the PR leaves
   no trace, which is the point).
6. **Convergence:** if a record survives **two** refactor attempts, or a fix keeps creating another,
   it is a 🔴 stop. The record stays on the branch as the analyzer wrote it. Report what was tried
   and why it would not close. There is no "accepted debt" for a code issue under this command.

**Compliance checklist.** Phase B does not end until every line is ✅ in the report, each with its
evidence (command output, file:line, commit):

| # | Check | Evidence |
|---|---|---|
| 1 | No non-test source file this PR grew is at or over 500 production lines | step 3.5's loop output |
| 2 | `/analyze-code-issues` ran on every touched package and persisted its records | its reconciliation report |
| 3 | No code-issue record added since `$mb` survives, committed or not | `{ git diff --diff-filter=A --name-only "$mb" -- 'packages/*/docs/code-issues/*.md'; git ls-files -o --exclude-standard -- 'packages/*/docs/code-issues/*.md'; } \| sort -u` is empty |
| 4 | No pre-existing record got worse; none this PR fixed is left behind | the numbers in each touched record |
| 5 | No `docs/dev/todo/` entry written for a split, a code issue or a stack-shared file | `git diff --name-only "$mb"..HEAD -- docs/dev/todo/` |
| 6 | No suppression, threshold tweak or ignore-list entry added | `git diff "$mb"..HEAD` grep for them |
| 7 | Pre-existing findings on touched packages are present in `docs/code-issues/`, as the analyzer wrote them | the report's list |

Any ❌ or unverified line is a 🔴 stop. A PR body that lists a deferred split or an open code issue
under a `/green-yolo` heading means the run failed, not that it finished.

**Own TODOs: the recommendation is the decision.** For each `TODO`/`FIXME` this PR added and each
`docs/dev/todo/` entry its changeset claims, recommend a disposition and take it:

| Recommendation | Effect |
|---|---|
| Solve in this PR now | back to Phase A for that item, then re-enter Phase B. **At most once per run**. A second "solve now" becomes recorded debt |
| Follow-up PR | write the plan pointer into the `docs/dev/todo/` entry (`deferred-work`) and continue |
| Leave as recorded debt | continue, and mark the row ⚠️ accepted debt |

Every row and its decision goes into the PR body and the final report. So does every code issue
fixed in place, with before → after numbers. The developer did not see either before the PR went up,
so it has to be visible after.

**Steps 7–8: the PR goes up now.** Wrap the docs (`/wrap-context-docs`, Stack Mode on a stack
branch), correct the title (the full title rules in `pr-wrap` step 8), and push. Then:

- **PR exists, still a draft** → `gh pr ready "$PR"` (per-PR, never `gh stack submit --open`). This
  is also what starts CI — drafts are not built.
- **No PR** → open it ready. Do not use `/pr`, which hands the result to a human to finish:
  ```bash
  base=master   # or the stack parent's branch
  gh pr create --base "$base" --head "$BRANCH" --title "$title" --body-file tmp/pr-body.md
  PR=$(gh pr view --json number --jq .number)
  ```
  The body states what the branch delivers (from the changeset's `## Summary`, captured **before**
  the wrap deletes it), the verification that ran, the TODO table with its decisions, and the
  changeset's final `## TODO` and `## Scope` as the `## Progress` section (`/update-context-docs` §
  8), also captured before the wrap — and no post-wrap item such as CI or merge (`/wrap-context-docs`
  § *Checklists end at the wrap*). End it with the PR attribution line from the session's git
  instructions.

## Phase C — `/fix-pr`, waiting out the checks

Follow **`.agents/commands/fix-pr.md` end to end** on `$PR`. That includes the 👍 / 👎 / 🚀 reaction
protocol, disagreement replies carrying evidence (posted, then reported), and never resolving
threads.

`/fix-pr` already fixes failures as they appear rather than idling. What `/green-yolo` adds is
**the exit condition**: the loop does not end until CI has finished **on the current head SHA**.

**C1. Pin the SHA before you wait on it.** Right after a push the rollup may still show the previous
run, so "nothing pending" can be true before CI has even started:

```bash
SHA=$(git rev-parse HEAD)
[ "$SHA" = "$(git rev-parse "origin/$BRANCH")" ] || { echo "local HEAD not pushed" >&2; exit 1; }
gh pr view "$PR" --json headRefOid --jq .headRefOid     # must equal $SHA
scripts/ci-status.sh "$PR"                              # the checks it lists must be for $SHA
```

Then `scripts/ci-status.sh "$PR" --watch` once the fix queue is empty and only waiting remains, in the
background if the harness would time out a foreground call, **and keep this command's turn open on
it**. A running watch is not a reason to hand the turn back to the user. Silence never means
"probably fine".

**C2. Every push restarts C1.** A fix pushed in the loop gets a new `$SHA`. Re-pin it and wait
again. A result from the previous SHA says nothing about this one.

**C3. Final sweep once CI is terminal.** Review comments can land after the last fix round's triage.
Once the checks are terminal on `$SHA`, run `/fix-pr` Step 2 (collect every thread) **once more**.
Anything new goes back into the loop. Phase C ends when a sweep finds nothing new on a terminal,
green `$SHA`.

**Ready gate** — the run's default deliverable. Every line must hold; any failing line is a 🔴 stop:

```bash
gh pr view "$PR" --json state,isDraft,headRefOid,mergeable,statusCheckRollup \
  --jq '{state, isDraft, head: .headRefOid, mergeable,
         notGreen: [.statusCheckRollup[] | select((.conclusion // .state // "")
                    | test("SUCCESS|NEUTRAL|SKIPPED") | not) | .name]}'
```

| Must hold | Means |
|---|---|
| `state == OPEN`, `isDraft == false` | the PR is up for review |
| `head == $SHA == origin/$BRANCH` | the checks you read ran on the code that would merge |
| `notGreen == []` | every check completed and passed. A flake classified but not re-run is **not** green |
| `mergeable != CONFLICTING` | nothing to restack first |
| every 👍 thread carries its 🚀; no thread's latest human ask is unanswered | the review was worked through, not just the checks |
| the *Code issues* gate passed (Phase B) | no code-issue record added since `$mb` survives and none got worse |
| no 🔴 anywhere in this run | the earlier phases ended clean |

**No merge intent → stop here.** "Ready and green" is the deliverable.

## Phase D — `#automerge` (only with merge intent)

Skip this phase entirely unless the invocation expressed merge intent (see the top of this file).
The invocation authorizes landing **this PR only**. It never waives a gate, and it never authorizes
merging, retargeting or repairing another node. The merge gate is the comment trigger in
[ci.md § Automerge](../../docs/dev/guides/ci.md#automerge); `#forcemerge` is never used here.

**D1. Wait for predecessors to land.** On an ordinary PR based on `master`, go to D3. On a stack
member, resolve the topology with the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`):

```bash
gh stack view
gh pr view <predecessor> --json number,state,mergedAt --jq '"#\(.number) \(.state)"'
```

- Every predecessor `MERGED`, and this PR's base is now `master` → D2.
- Any predecessor still open → **wait.** Poll on a long interval (minutes, not seconds). Background
  the poll if the harness would time out, and keep the turn open on it, reporting progress briefly.
  Do not merge, fix, rebase or nudge the predecessor. That is its own PR's run. If the session has
  to end while waiting, report **`READY, NOT MERGED`**, naming each blocking PR and its state.
- A predecessor **closed without merging** → 🔴 stop. The stack's shape changed, and that is a
  human decision.

**Never arm `#automerge` while the base is a parent branch** — it would merge this PR into its
parent's PR (`plan-pr-stack` Step 10).

**D2. Re-prove the PR on its new base.** GitHub does not restack for you here; repoint it with
`/repoint` (or `/pr-stack-rebase` single mode once the base is `master`). Fetch, and confirm
`gh pr view --json baseRefName` is `master` and `git log origin/master..origin/$BRANCH` is this PR's
commits only. If the tip changed, Phase C runs again on the new `$SHA`.

**D3. Post `#automerge`.** Immediately before posting, re-check every ready-gate line on the current
`$SHA`, plus: base is `master`, and no `#automerge` comment is already posted for this tip.

```bash
gh pr comment "$PR" --body '#automerge'
```

**D4. Wait for `MERGED`, not "armed".** Poll `gh pr view "$PR" --json state,mergeCommit` until
`state == MERGED`, and record the merge commit. Armed auto-merge is not success. If a check flips
while it is armed, that is Phase C work on this tip, then D3 again. Afterwards, read the successors'
base state **read-only** and report any repoint they need. Do not operate on them.

## Progress Tracking

```
[ ] 0. Preflight — branch, clean tree, PR state
[ ] A. /green + /validate-changes — red verified, green, no gaps, committed + pushed
[ ] B. /pr-wrap (auto-accept) — cleanups applied, splits done, no code issues added, TODOs decided, docs wrapped, PR ready
[ ] C. /fix-pr — checks terminal on head SHA, threads worked, final sweep clean, ready gate passed
[ ] D. merge intent only: predecessors MERGED, base master, tip re-proved, #automerge posted, MERGED observed
```

## Report

```markdown
## 🚀 Green-YOLO: #<PR> <merged | ready, not merged (blocked on #N) | ready and green | stopped at phase X>

| Phase | Result |
|---|---|
| A /green + /validate-changes | ✅ N tests green · no gaps |
| B /pr-wrap | ✅ fixes applied (N) · 🔴/✅ file splits · ✅ compliance checklist 7/7 · ✅ code issues added: 0 (N fixed in place) · docs wrapped · title corrected |
| C /fix-pr | ✅ N rounds (pushes) · checks green on `<sha>` · N threads · ready gate ✅ |
| D #automerge | ✅ MERGED `<merge-sha>` / ⏳ waiting on #N / n/a (no merge intent) |

### Decisions taken on your behalf
- Every cleanup, refactor and split applied in Phase B, with file:line or commit
- Every code issue fixed in place, with before → after numbers; pre-existing issues listed separately
- Every TODO row and the disposition chosen
- **Every disagreement / out-of-scope reply posted by `/fix-pr`**, linked, with what it argued

### Left for you
- 🔴 the stop reason, if any, and exactly what decision unblocks it
- ⚠️ accepted debt rows · flaky checks awaiting a re-run decision · a top-node *Extra node* verdict
- Next (no merge intent): `/squash-pr` (base is master) or `/merge-pr-stack` (stack), or re-run with `#automerge`
```

The "Decisions taken on your behalf" section is not optional. Nobody saw these decisions before
they reached GitHub, so the report is where the developer reviews them.

## Rules

- **Run each phase's command in full.** `/green-yolo` answers their questions. It never removes
  their steps.
- **The invocation pre-approves cleanups, refactors, splits, wrapping, opening and readying the PR,
  posting `/fix-pr`'s replies, plus `#automerge` only when it expressed merge intent. Nothing
  else.** It cannot approve a planned-scope deferral, a re-run, a stack-gate bypass or a workaround.
- **No deferral of a split this PR caused or of a code issue it added, ever, including on stacks.**
  Stack-shared files are split and cascaded. Code issues are written to `packages/*/docs/code-issues/`
  in the record convention, never described only in the PR body.
- **Scoped local gates, CI for the rest.** Never run a full-workspace build, test or clippy locally.
- **Wait on the head SHA, not on a pending count.** Pin `$SHA` and re-pin after every push.
- **Sweep threads once more after CI is terminal.**
- **Merge only with expressed merge intent**: after every predecessor has merged and the base is
  `master`, through a separate `#automerge` comment, for this PR only. Never `#forcemerge`, never
  `gh pr merge`. Never re-run, cancel or force a check unprompted. Never `--no-verify`, never amend,
  never `gh stack submit --open`.
- **Re-enter `/green` at most once** (from `/validate-changes` gaps or a "solve now" TODO). A second
  loop means the plan is wrong. That is a human conversation.

## Related

**Commands**: `/green` (phase A), `/validate-changes` (phase A), `/pr-wrap` (phase B), `/fix-pr`
(phase C), `/squash-pr` and `/merge-pr-stack` (next: landing), `/repoint` (phase D),
`/pr-stack-rebase`, `/yolo-pr-stack` (the whole-stack version that reuses this consent table)
**Skills**: `tdd`, `code-restructuring`, `analyze-code-issues`, `deferred-work`, `pr-stack`
(all under `.agents/skills/`)
**Script**: `scripts/ci-status.sh`
