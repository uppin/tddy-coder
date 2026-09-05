---
description: Rebase a stack branch onto the latest of its own base - this worktree's branch by default, or a bottom-up cascade over a named PR range, each layer in a worktree that owns it
---
## PR Stack Rebase — one branch at a time, in a worktree that owns it

Bring a stack branch up to date with the latest tip of **its own base**. Two modes:

| Mode | Invocation | Scope |
|---|---|---|
| **Single** (default) | `/pr-stack-rebase` · `/pr-stack-rebase <base-branch>` | the branch checked out in **this** worktree, and nothing else |
| **Cascade** | `/pr-stack-rebase from #2 to #4` · `#2..#4` · `2..4` · a list of PR numbers or branches | each named layer, **bottom-up**, each rebased in a worktree that owns it |

Single mode is the per-worktree way to get current: it is what each `/green` worktree runs for itself
while a stack is being implemented concurrently. **It remains the default and the mode every
hard-gate caller uses.**

Cascade mode exists for the case single mode cannot serve: several layers have gone stale at once —
typically because a predecessor was greened and force-pushed — and nobody is sitting in each of those
worktrees to run the command themselves.

**Scope rule.** In either mode, exactly one branch is rewritten at a time, and only ever from a
worktree that owns it. `master` is never fast-forwarded. Branches outside the requested set are never
touched.

**Hard-gate callers.** `/green`, `/validate-changes`, and `/pr-wrap` **always** invoke this command in
**single mode** on a stack branch **before any implementation or code diff**. A clean-looking
`gh pr list` or a green `pr_stack_status` row is not permission to skip it: leaked ancestor commits can
still sit in `HEAD` relative to a stale merge-base, and treating that as this PR's work (or deleting
the "extra" files to tidy the diff) is how predecessor code gets destroyed. `/green` also needs the
latest ancestor implementation before it starts, or it will re-implement a symbol a parent node
already owns. If the branch already contains the latest base tip and the commit range is this PR only,
this command is a **verify-and-return** — it does not rewrite history and does not force-push. **A
hard-gate caller must never widen itself into cascade mode**: rewriting branches its user did not name
is not part of "get my branch current".

> ### There is no stack-wide rewrite command here — and cascade mode is not one
> GitHub's `gh stack` extension offers `sync` / `rebase`, which rewrite **every** branch in a stack
> and force-push them **atomically from one worktree**. **Neither the extension nor that behaviour
> exists here**, and nothing in this command reproduces it. Do not go looking for an equivalent: git
> refuses to update a branch checked out in another worktree, so in the concurrent-implementation
> model an all-branches rewrite either fails outright or, where it succeeds, rewrites a branch
> somebody is mid-`/green` on.
>
> Cascade mode differs on all three counts: it rebases **only the layers you named**, it runs each
> layer **inside a worktree that owns that branch**, and it pushes each branch **individually,
> stopping at the first failure** rather than atomically.

> ### The orchestrator-side path (planned stacks only)
> In a **planned** stack — a `pr-stack` orchestrator session owning the DAG in its `Changeset.stack` —
> the sync-and-detect-conflicts job belongs to the orchestrator's own tooling, not to this command:
>
> | Orchestrator tool | What it does |
> |---|---|
> | `pr_stack_status` | every node with its live GitHub state and its computed internal status (`needs-repoint`, `has-conflicts`, `ready-to-merge`, …) |
> | `pr_resolve_conflicts` | syncs a node's branch with its base, detects conflicts (`git ls-files -u`), marks the node `has-conflicts`, and returns the conflicted paths for the agent to resolve in that node's worktree |
> | `pr_repoint` | changes a node's base after an ancestor merged — that is `/repoint`, not this command |
>
> Those tools are available **only to the orchestrator agent** during its `orchestrate` goal. A child
> session working one node does **not** have them: it has its attached `PRD.md` / `changeset.md` and
> ordinary git. **This command is the branch-side one** — it is what the child worktree runs, and what
> anyone in an ad-hoc chain runs. See
> [`docs/ft/coder/pr-stacking.md` § PR-management tools](../../docs/ft/coder/pr-stacking.md#pr-management-tools).
>
> The web PR-Stack panel also offers a non-rewriting alternative per row — *pull the base in*
> (`PullBaseIntoBranch`, merge by default, rebase on request), described in
> [`docs/ft/coder/pr-stack-live-status.md` § Pulling the base into a branch](../../docs/ft/coder/pr-stack-live-status.md#pulling-the-base-into-a-branch).
> That is `/merge` semantics with a UI attached; it is not a substitute for the leak check below.

Contrast with the neighbours:

| Command | Scope | What it changes |
|---|---|---|
| `/pr-stack-rebase` (single) | this worktree's branch | replays this branch's commits on the **latest tip of its existing base** |
| `/pr-stack-rebase` (cascade) | the named layers, bottom-up | the same, per layer, each in a worktree that owns the branch |
| `/merge` | this branch | **merges** the base in instead of rebasing (keeps history, adds a merge commit) |
| `/repoint` | this branch | **changes** the base (usually to `origin/master` after the predecessor landed) |
| `/merge-pr-stack` | the whole stack | lands the stack bottom-up, repointing dependents as each layer merges |

## Step 0: Parse the argument — which mode?

- **No argument** → single mode on the current branch.
- **One token that names a branch** (`/pr-stack-rebase feature/auth/token-store`) → single mode with an
  explicit base *candidate*. Read Step 2 before using it: in this repo an argument does **not**
  outrank the PR's real base.
- **A range or list** — `from #2 to #4`, `#2..#4`, `2..4`, `feature/auth/token-store..feature/auth/middleware`,
  or two or more PR numbers / branch names → **cascade mode**.

PR numbers may be written as `#2` meaning *the second PR of this stack* (its position), or as a real
PR number like `#433`. **Disambiguate explicitly and say which reading you used** — a small integer is
almost always a stack position, but never guess silently. Resolve positions bottom-up, where position
1 is the branch based on `master`:

- **Planned stack** — the node order in the orchestrator's `artifacts/stack-plan.yaml` (or what
  `pr_stack_status` reports), which is the DAG's topological order.
- **Ad-hoc chain** — the ancestry walk in Step 1.

If the argument names layers that are not contiguous, say so and rebase the contiguous span that
covers them — a gap would leave a middle layer on a stale parent while its successor moved.

**A DAG, not only a chain.** A node in a planned stack has a `parents` **list** and may have several
parents. "Bottom-up" therefore means *topological* order, not "the next number". A multi-parent
(diamond) node is rebased only after **every** parent in the requested set has been processed; if one
of its parents is outside the set, say so and treat that parent's tip as fixed.

## Step 1: Preflight

```bash
git rev-parse --show-toplevel                 # the worktree we are pinning
git branch --show-current                     # the branch checked out here
git status --porcelain                        # must have no tracked-file changes
git worktree list                             # which branches are pinned where
gh pr list --state open --json number,headRefName,baseRefName,isDraft,reviewDecision
```

**Is this branch in a stack at all?** Three signals, any one of which is enough:

1. **Planned stack** — this session's `changeset.yaml` carries `orchestrator_session_id`, i.e. it is a
   child of a `pr-stack` orchestrator. Its attached `changeset.md` has a `## Dependencies` section
   naming the parent nodes.
2. **Ad-hoc chain** — the branch's open PR has a `baseRefName` that is not `master`/`main`. Detect it
   exactly as `/pr` does: `gh pr list --state open --json number,headRefName,baseRefName`, then
   `git merge-base --is-ancestor origin/<headRefName> HEAD` for each other open PR's head; the ones
   that pass are stack ancestors, and the **stack parent** is the closest (smallest
   `git rev-list --count origin/<headRefName>..HEAD`).
3. The branch's `changeset.yaml` records a `worktree_integration_base_ref` that is not the project
   default branch.

**Single mode.** If none of the three holds, stop and tell the user — `/merge` is the command for an
ordinary branch, and it does not force-push.

**Cascade mode.** The current branch does **not** need to be in the stack — cascade mode may be run
from an orchestrator worktree, or any worktree sitting on an unrelated branch. What it does need:

- **A clean tree here**, because cascade may borrow this worktree (Step 3b). Commit first — never
  `--no-verify`, never amend, do not stash silently.
- **`ORIGINAL_BRANCH` recorded** before anything else, and restored at the end **including on
  failure**:
  ```bash
  ORIGINAL_BRANCH=$(git branch --show-current)   # must not be empty; refuse a detached HEAD
  ```
  A detached start cannot be restored to a named branch. Stop and ask the user to check one out.

## Step 2: Resolve each layer's base — MANDATORY (never assume master)

For every branch to be rebased, resolve what to rebase **onto**, in this order:

1. **The PR's own base** — `gh pr view <branch> --json baseRefName --jq '.baseRefName'`. This is the
   live fact: `/repoint` and the `pr_repoint` tool both write it, so it is what GitHub will actually
   diff the branch against. It comes **first** because nothing else in this repo is more authoritative
   — there is no registered stack object sitting above GitHub's own answer.
2. **The stack plan / changeset.**
   - *Planned stack*: the node's `parents` in the orchestrator's `artifacts/stack-plan.yaml`, resolved
     the way the product resolves it — climb `parents`, skip **merged** ancestors, take the nearest
     non-merged ancestor's `branch` as `origin/<branch>`; when every ancestor is merged the base
     collapses to the stack bottom (`origin/master`). This is `Stack::effective_base_refs` /
     `base_ref_for_spawn`, documented in
     [`docs/ft/coder/pr-stacking.md` § Stack data model](../../docs/ft/coder/pr-stacking.md#stack-data-model).
     A non-merged ancestor that owns no branch contributes nothing — there is no ref to rebase onto,
     and that is a **blocked** node, not a base. Say so and stop.
   - *Ad-hoc chain*: the stack parent found by the ancestry walk in Step 1, or the branch's
     `worktree_integration_base_ref`.
3. **The explicit argument**, if the user gave one.
4. **Ask the user.** Never silently fall back to `master`.

**If the argument disagrees with rule 1, stop.** Rebasing onto a branch that is not the PR's base
produces a local history GitHub will not show: the PR keeps diffing against its old base, so the
"clean diff" you just made is invisible and the leak check below is measuring the wrong range.
Changing a base is **`/repoint`**, which edits the PR base *and* restacks. Say which of the two the
user meant and route accordingly.

State which rule resolved each base before doing anything.

## Step 3: Choose where each layer is rebased — the cascade's core rule

**A branch is only ever rebased from a worktree that owns it.** Per layer, in order:

### 3a. Delegate — a worktree already pins this branch

```bash
W=$(git worktree list --porcelain | awk -v b="refs/heads/$BRANCH" '
  /^worktree /{p=$2} $0=="branch "b{print p}')
```

Session worktrees live under `<repo>/.worktrees/<name>`, so this finds child-session checkouts as well
as hand-made ones. If `$W` is non-empty, run that layer's whole rebase **inside that worktree** with
`git -C "$W" …`. This is the case that matters most, and it comes with a hard gate — **that worktree
may belong to a session mid-`/green`**:

```bash
git -C "$W" status --porcelain | grep -v '^?? '                  # must be empty
git -C "$W" rev-list --count "origin/$BRANCH..$BRANCH"           # must be 0 — nothing unpushed
```

If either check fails, **STOP the whole cascade** and report which worktree and why. Rebasing under
somebody's uncommitted or unpushed work destroys it, and no amount of "it was probably fine" makes
that recoverable. Offer: they commit and push, or they run single-mode `/pr-stack-rebase` there
themselves.

Untracked files are deliberately **not** a blocker here (hence the `grep -v '^?? '`): git refuses
loudly rather than clobbering an untracked file, and blocking on them would make this permanently dead
in any real agent worktree — the same reasoning the product's own dirtiness probe uses
(`git status --porcelain --untracked-files=no`, see
[`docs/ft/coder/pr-stack-live-status.md` § Base sync](../../docs/ft/coder/pr-stack-live-status.md#base-sync)).

### 3b. Borrow — no worktree pins this branch

Check the branch out **here**, rebase, push, and move on. This is safe precisely because nothing else
holds it:

```bash
git fetch origin "$BRANCH"
git switch "$BRANCH"
```

Restore `$ORIGINAL_BRANCH` when the cascade finishes — see Step 6. Borrowing switches the working
tree, so `target/`, `packages/tddy-web/node_modules` and generated protobuf output may no longer match
the checkout; a layer's build/test step may need a rebuild, and a layer whose build cannot be verified
must say so rather than claim green.

### Never

- Never update a pinned branch from **this** worktree — git refuses, and forcing past it is how
  concurrent work is lost.
- Never `git worktree add` a new worktree per layer here. Each one needs its own `target/` (multiple
  GB once anything is built) plus a `node_modules` if the web package is touched. Borrow-and-restore
  is the cheap path; delegate is the correct one.
- Never rebase a layer whose PR is **merged or closed**. Skip it, say so, and continue from the next
  open layer. A merged node is `is_skipped()` in the stack model for exactly this reason.

## Step 4: Rebase one layer

Run these for the layer's branch, in the worktree Step 3 chose (`git -C "$W"` when delegating).

1. **Fetch only what is needed** — scoped refs, never a bare `git fetch origin` and never `--all`:
   ```bash
   git fetch origin <base> <branch>
   ```
   The scope is not tidiness. A blanket fetch moves every remote-tracking ref, and a later
   `--force-with-lease` then compares against a remote state you never inspected — the lease stops
   protecting you. The product's own pull path fetches base-ref-scoped for this reason.
2. **Already current? Verify leak-free, then return without rewriting.**
   ```bash
   git merge-base --is-ancestor origin/<base> HEAD && echo CURRENT || echo STALE
   git log --oneline origin/<base>..HEAD                   # must be ONLY this PR's own commits
   ```
   - **CURRENT and the log is this PR only** → report "already current, leak-free, no rewrite", skip
     the backup, the rebase, and the force-push. In single mode, continue from Step 5. In cascade
     mode, **record this layer's tip as unchanged and move to the next layer** — a layer that did not
     move does not make its successor stale. Do **not** treat this as a skip of the command: the leak
     check *is* the work.
   - **CURRENT but the log contains predecessor commits** → ancestor work leaked into this range.
     This is the rewritten-base / wrong-merge-base case. Do **not** analyze those files as this PR's
     work and do **not** delete them to "clean" the diff. Use the `--onto` form below with the
     predecessor's previous tip as `<upstream>`.
   - **STALE** → continue; the branch does not contain the latest base tip.
3. **Backup branch** — cheap insurance before any history rewrite:
   ```bash
   git branch backup/<branch>-$(date +%Y%m%d-%H%M%S)
   ```
4. **Record this layer's pre-rebase tip — REQUIRED IN CASCADE MODE.**
   ```bash
   OLD_TIP_<k>=$(git rev-parse "origin/<branch>")
   ```
   **This is the mechanic that makes a cascade correct, and the reason running single mode N times by
   hand goes wrong.** Once layer K has been rebased, layer K+1's recorded merge-base is a commit that
   no longer exists on K. A plain `git rebase origin/<base>` at layer K+1 would then replay **K's old
   commits** as if they were K+1's own, silently duplicating the predecessor's work into the
   successor's diff. So each layer's `<upstream>` is the tip its base had **before** this run rewrote
   it.
5. **Rebase this branch only:**
   ```bash
   # first layer, base not rewritten by this run:
   git rebase origin/<base>

   # any layer whose base WAS rewritten by this run (every cascade layer after the first that moved),
   # or a base that was force-pushed earlier by somebody else:
   git rebase --onto origin/<base> "$OLD_TIP_<k-1>" <branch>
   ```
   When the base was rewritten by somebody else before this run and no `OLD_TIP` was recorded, recover
   it from `git reflog show origin/<base>` or the backup branch. This is the same
   `git rebase --onto <new_base> <old_base> <branch>` shape the product's repoint bridge uses, with the
   same `git merge-base` guard against a stale `<old_base>` — see
   [`docs/ft/coder/pr-stacking.md` § Merge and repoint](../../docs/ft/coder/pr-stacking.md#merge-and-repoint).

   Never `git rebase --update-refs` — it rewrites sibling stack branches all at once, which is the
   atomic, all-branches behaviour this command deliberately does not have.
6. **Resolve conflicts preferring this branch's own surface.** This PR owns its symbols; the base owns
   its own. On a shared catalog or index file — a `mod.rs` list, a match arm table, a proto field
   block, a router registration — resolve by **union**: keep both contributions. **Never implement a
   parent-owned symbol while resolving.** The whole point of `## Dependencies` in the per-PR
   `changeset.md` is that a surface listed there is somebody else's to create; re-creating it here is
   the duplicate-development failure the per-PR documents exist to prevent
   ([`docs/ft/coder/pr-stack-docs.md` § Why boundaries belong in a per-PR document](../../docs/ft/coder/pr-stack-docs.md#why-boundaries-belong-in-a-per-pr-document)).
   **Never delete parent-owned files to resolve a conflict.**

   **In cascade mode a conflict stops the run.** Resolve this layer, or abort it
   (`git rebase --abort`) and report — never continue to the next layer, which would rebase it onto a
   half-resolved parent. Say which layers were completed, which one stopped, and which were not
   attempted.
7. **Build and test.** Fix until the branch is in the same state it was before the rebase (green stays
   green; a still-red `/green` in progress stays red for the *same* reasons, not new ones):
   ```bash
   cargo build                       # compiles?
   cargo fmt                         # a rebase can leave a badly-merged block
   cargo clippy -- -D warnings
   ./test -p <package>               # the packages this layer touches
   ```
   `./test` also writes `.verify-result.txt`; read that file for evidence rather than trusting an exit
   code you cannot see. **Scope the verification to the packages this layer touches and say that you
   scoped it** — a full-workspace run carries pre-existing noise that will be misread as rebase
   damage. In cascade mode, report each layer's state separately; a layer whose build could not be
   verified (borrowed worktree, stale `target/`) must be reported as **unverified**, never as green.
8. **Reviewed-PR gate.** A rebase rewrites SHAs and requires a force-push, which can dismiss approvals
   under branch protection. If the layer's PR is **not a draft and has an approval**
   (`gh pr view <branch> --json isDraft,reviewDecision`), say so and **ask before force-pushing that
   layer**. In cascade mode ask about all affected layers up front, in Step 3, rather than
   interrupting halfway through.
9. **Push this branch only:**
   ```bash
   git push --force-with-lease origin <branch>
   ```
   Never plain `--force`, never `--no-verify`, never push a branch other than the layer being
   processed, and never push several branches atomically.

## Step 5: Report successors as stale

Rebasing a branch leaves every successor **outside the processed set** on a stale parent. In a planned
stack `pr_stack_status` will derive `needs-repoint` / `has-conflicts` for them on its next run, and the
web panel's per-row base-sync badge will show them *behind*. That is expected, not a defect: each such
successor's own worktree clears it with single-mode `/pr-stack-rebase`. Name them in the output so the
user knows who runs it next, and in what order.

In cascade mode this applies only to layers **above the top of the range** — the layers inside the
range were just brought current by the run itself. Skip the step entirely for any layer that Step 4.2
reported already-current, since nothing moved.

Then re-read the topology (read-only, safe from any worktree):

```bash
gh pr list --state open --json number,headRefName,baseRefName,title
```

In a planned stack, the orchestrator agent can instead run `pr_stack_status`, which reports the same
topology plus each node's internal status.

## Step 6: Restore the borrowed worktree — MANDATORY in cascade mode

If any layer was borrowed (Step 3b), this worktree is sitting on a stack branch and **pinning it**,
which blocks whoever should be implementing it:

```bash
git switch "$ORIGINAL_BRANCH"
git branch --show-current      # must equal ORIGINAL_BRANCH
```

Do this **even when the cascade failed part-way** — an aborted run must not leave the worktree parked
on somebody else's branch. Confirm the restore in the report. Never delete a stack branch: a successor
bases on it, and deleting it closes that PR (unreopenable via the API).

## After the rebase

Report:

- **which mode ran**, and for cascade, the resolved layer list in topological order and how `#N` was
  read (stack position vs PR number);
- the resolved base for each layer and **which rule from Step 2 resolved it**;
- per layer: **already current vs rebased**, where it ran (**delegated** to which worktree path, or
  **borrowed** here), old → new HEAD SHA (or unchanged), and the commits in `origin/<base>..HEAD`
  (that PR's own delta only);
- whether each layer's leak check passed (commit range is that PR only);
- conflicts resolved and how — naming any file resolved by union;
- build/test state per layer, **with the package scope you verified**, explicitly flagging any layer
  reported **unverified**;
- **which successor branches now need their own `/pr-stack-rebase`, and in what order** (none when
  everything in range was already current);
- whether a force-push may have dismissed an approval, and whether an armed `#automerge` now needs
  re-arming — the new head re-runs the required checks from scratch
  ([`docs/dev/guides/ci.md` § Automerge](../../docs/dev/guides/ci.md#automerge)). Never reach for
  `#forcemerge` to get past checks a rebase invalidated;
- **for cascade: that `$ORIGINAL_BRANCH` was restored**, and any layer skipped (merged/closed) or not
  attempted (stopped at a conflict).

## Rules

- **One branch rewritten at a time, always from a worktree that owns it.** Delegate with `git -C` to
  the worktree that pins it; borrow this worktree only when nothing pins it.
- **Never rewrite a branch pinned by another worktree from here**, and **never** proceed when that
  worktree has uncommitted or unpushed tracked work — stop the cascade and report.
- **Cascade runs bottom-up in topological order and stops at the first conflict or failure.** Never
  continue onto a half-resolved or stale parent. A diamond node waits for all its parents.
- **In cascade mode, each layer after a rewritten base uses `git rebase --onto <base> <recorded
  pre-rebase tip>`.** A plain rebase there duplicates the predecessor's commits into the successor.
- **The PR's `baseRefName` outranks an argument.** An argument that disagrees is a `/repoint`, not a
  rebase.
- **Restore `$ORIGINAL_BRANCH`** when cascade mode borrowed this worktree — including after a failure.
- **Hard-gate callers use single mode only.** `/green`, `/validate-changes` and `/pr-wrap` never widen
  into a cascade; a healthy-looking status is not a skip, and already-current is verify-and-return,
  not a rewrite.
- Never rebase a merged or closed PR's branch; skip it and say so.
- Never delete a stack branch — a successor bases on it, and deleting it closes that PR.
- Never `--force` (use `--force-with-lease`), never `--no-verify`, never amend, never
  `git rebase --update-refs`, never a bare `git fetch origin`/`--all` before a leased push.
- Never implement a parent-owned symbol while resolving conflicts, and never delete parent-owned files
  to "clean" a leaked diff — rebase (or `--onto`) instead.

## Related

**Commands**: `/merge`, `/repoint`, `/add-to-pr-stack`, `/split-pr-to-stack`, `/split-branch`,
`/squash-pr`, `/green`, `/validate-changes`, `/pr-wrap`, `/merge-pr-stack`, `/fix-pr`
**Skill**: `pr-stack` (`.agents/skills/pr-stack/SKILL.md`)
**Product docs**: [PR stacking](../../docs/ft/coder/pr-stacking.md) ·
[PR-Stack live status & repoint](../../docs/ft/coder/pr-stack-live-status.md) ·
[PR-stack documents](../../docs/ft/coder/pr-stack-docs.md) ·
[Continuous integration](../../docs/dev/guides/ci.md)
