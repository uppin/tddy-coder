---
description: Squash-land a PR - to origin/master (common), or fold it into its base PR (rare, 2 PRs → 1); detects the ambiguity and asks. Offers to repoint any open successors and deletes the branch once nothing references it
---
## Squash PR

Squash a PR's commits and land it. The **target** of a squash-merge is always the PR's **base**, so
for a stacked PR the target is ambiguous — this command detects that and **asks** before doing
anything destructive.

Load the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`) for the stack model, base tracking,
and the golden rules referenced below. The product-level model — the DAG, the node fields, the
`pr_*` tools, merge and repoint — is
[`docs/ft/coder/pr-stacking.md`](../../docs/ft/coder/pr-stacking.md).

> **A branch pinned by a worktree cannot be rebased**, so free it first (`git worktree list`).
> tddy-coder sessions live in worktrees, so this is the normal case, not the exception. And once a
> PR leaves the stack, **re-register what remains** — `gh stack link --base master <prs…>`.

## Step 1: Resolve the PR, its base, and its successors — MANDATORY

- Identify the PR (current branch by default, or a branch/number passed as an argument).
- **Resolve its base, and note which of the two stack models applies** — it changes how successors
  are handled:

  The base is `gh pr view <N> --json baseRefName`, plus the ancestry probe from
  `.agents/commands/pr.md`. Successors are the open PRs whose `baseRefName` is **this** branch.

  Order of resolution: an explicit argument → this PR's `docs/dev/1-WIP/` changeset →
  `gh pr view <branch> --json baseRefName`.
- Fetch the base PR's status (if the base is a branch with an open PR):
  ```bash
  repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)   # uppin/tddy-coder
  gh pr view <base-branch> --json number,state,url,mergeStateStatus 2>/dev/null
  ```
- **Detect successors** — open PRs whose base is **this** branch. They decide both the repoint offer
  and whether the branch is safe to delete (Step 4); deleting a branch a successor still bases on
  **closes** that successor (golden rule 2):
  ```bash
  gh pr list --state open --base <this-branch> --json number,headRefName,url \
    --jq '.[] | "#\(.number) \(.headRefName) \(.url)"'
  ```
  A successor whose PR is not open yet owns no `baseRefName` and will not appear here, though its
  branch is still based on yours. If the plan names one, check for it explicitly.

## Step 2: Check CI, and understand the merge gate before you use it

**Always check CI before landing** — a squash-merge (Case A or B) must not run against a red or
still-pending PR:

```bash
scripts/ci-status.sh <N>              # per-check state plus pass/fail test counts
scripts/ci-status.sh <N> --failures   # failing test names, files, assertion messages, log tails
scripts/ci-status.sh <N> --watch      # block until the run finishes, then report
```

- Checks **failing** → stop and report; do not squash. `/fix-pr` is the command that makes a PR
  mergeable.
- Checks **pending** → wait for them to settle (`--watch`), or report and ask before proceeding.

**Also read the repo's merge settings once, before anything destructive.** One of them can close a
successor behind your back:

```bash
gh api repos/{owner}/{repo} --jq '{delete_branch_on_merge, allow_squash_merge, allow_auto_merge}'
```

If `delete_branch_on_merge` is **true**, the merge itself deletes this branch — which is exactly
the golden-rule-2 hazard Step 4 exists to prevent, moved earlier than you can intervene. When it is
true and this PR has open successors, **repoint the successors first** (Step 4a, before the merge)
rather than after, and say in the report that the order was forced by the repo setting.

### The merge gate: `#automerge`

`master` carries a ruleset requiring four checks (`Rust lint`, `Rust build`, `Rust tests`,
`Web tests`), so a PR is not mergeable until they pass. The repo's own mechanism for landing one is
a **comment trigger**, handled by `.github/workflows/automerge.yml`
([`docs/dev/guides/ci.md` § Automerge](../../docs/dev/guides/ci.md#automerge)):

| Comment | Effect | Reaction it leaves |
|---|---|---|
| `#automerge` | Merge — **squashed** — as soon as the four required checks pass. If they are already green it merges immediately; otherwise it arms GitHub's native auto-merge. | 🚀 |
| `#automerge-cancel` | Disarm it again | 👀 |
| `#forcemerge` | Merge **now**, past red or still-running checks | 🎉 |

What the workflow's own header documents about its preconditions, stated honestly because a
mis-set repo makes the trigger fail rather than merge:

- **`Allow auto-merge` must be on for the repo**, and the `master` ruleset must actually require
  those four checks. Native auto-merge is a *queue for a blocked PR*: with nothing required, a PR is
  mergeable the instant it opens and GitHub rejects the request with *"Pull request is in clean
  status"*. The workflow works around that by asking `gh pr checks --required` first and merging
  directly when everything is already green.
- **`#forcemerge` additionally needs the GitHub Actions app in that ruleset's `bypass_actors`.**
  Without it, `gh pr merge --admin` fails rather than merging quietly.
- Only commenters with `write`, `maintain` or `admin` are obeyed; the workflow asks the API for the
  real permission level. An unauthorised comment gets **no reaction at all** — silence is the
  failure mode, so check for the 🚀 rather than assuming the comment worked. A request that fails
  for another reason gets 😕.
- `issue_comment` workflows always run from the **default branch**, so a change to `automerge.yml`
  on a branch has no effect until it is on `master`.
- Before a force merge the workflow comments on the PR naming who asked, linking their comment, and
  quoting the full check state at that moment. That trace is permanent.

**Never post `#forcemerge` unless the user explicitly asked for it in this invocation.** It merges
past the gate; it is not a way to get around a slow build. `#automerge` is the default path and is
what "squash-land" means here — it is the same squash the direct call below performs.

A **direct** `gh pr merge <N> --squash` is equivalent and synchronous, and is the right call when
you hold write access and the required checks are already green. Prefer it when you need to observe
the merge complete before touching the next branch (that is what `/merge-pr-stack` does); prefer
`#automerge` when the checks are still running and you want the merge to happen when they settle.
Either way the method is **squash** — never merge-commit, never rebase-merge.

## Step 3: Choose behavior by base

### Case A — base is `origin/master` (common)

Squash-land straight to master:

```bash
gh pr merge <N> --squash            # no --delete-branch here — successors + deletion are handled in Step 4
# or, when the checks have not settled yet:
gh pr comment <N> --body '#automerge'
```

**Golden rule:** do **not** `--delete-branch` in this call. Successors (Step 1) may base on this
branch; deleting it while one is open closes that PR (unreopenable). Hand off to **Step 4**, which
offers to repoint successors and then deletes the branch once nothing references it. The new base for
any successor here is `origin/master`.

(If Step 2 found `delete_branch_on_merge: true`, the repo deletes the branch for you regardless of
this flag — which is why that check is a preflight and not an afterthought.)

### Case B — base is another (open) PR branch (rare) — ASK, do not assume

A plain squash-merge here would merge **this PR into its base PR** (the base branch absorbs this PR's
squashed commit; this PR closes as merged; the base PR's diff grows to cover both). That collapses
**2 PRs → 1** — which may or may not be intended. **Ask the user which they meant:**

> This PR's base is the open PR **#\<base-N\> (\<base-branch\>)**, not master. Did you intend to:
> **(A) Land this PR to master** — `/repoint` it onto `origin/master` first, then squash-merge; both
>   PRs stay separate. *(repoint + squash)*
> **(B) Fold this PR into its base PR** — squash this PR's commits into `\<base-branch\>`, closing this
>   PR and making the base PR contain both. *(2 PRs → 1)*

**Wait for an explicit answer.** Then:

**If (A) repoint + squash:**
1. Run `/repoint` to move this PR's base to `origin/master` and restack — it does three things
   (re-target the PR base, rebase
   the branch onto the new effective base, force-push with lease) *and* persists the new parent set
   in the plan.
2. `gh pr merge <N> --squash` (now lands to master). Then go to **Step 4** for successor repoint +
   branch deletion (new base for successors = `origin/master`).

**If (B) fold into base PR:**
1. Confirm the base PR is the intended combine target and is still open.
2. Squash-merge this PR into its base branch as-is:
   ```bash
   gh pr merge <N> --squash        # base is <base-branch>, so this folds into the base PR
   ```
   (Equivalently: `git checkout <base-branch> && git merge --squash <this-branch> && git commit && git push`.)
3. **Successor handling** — defer to **Step 4**; the new base for any successor here is
   `<base-branch>` (the branch this PR folded into), not master.
4. Update the base PR's title/body to reflect the combined scope, and merge this PR's per-PR
   documents into the base PR's (the PRD and changeset pair in `docs/dev/1-WIP/`). Respect the
   forward-only linking rule — the base is the predecessor. The four required headings survive the
   fold: the combined `## Responsibility` and `## Boundaries` must describe the union, and anything
   the folded PR listed under `## Dependencies` that the base now implements has to come **out** of
   that list, or the base PR is documenting itself as depending on itself.
   - **Check the fold still satisfies the boundary contract.** The combined PR must remain
     independently reviewable and mergeable, and it must not be a layer split reassembled — if the
     two halves were "add the surface" and "implement it", folding them is a *fix*, and worth saying
     so in the report. See
     [`docs/ft/coder/pr-stacking.md` § PR boundary contract](../../docs/ft/coder/pr-stacking.md#pr-boundary-contract-every-node-is-self-contained).
5. **Fix up the stack, not just the prose.** The fold means one fewer PR, so re-register what
   remains — `gh stack link --base master <prs…>` with the folded PR left out — **after** the merge
   closes it. Retitle the remaining PRs so `K/N` reflects the new count, and note the fold in the
   base PR's body.

## Step 4: Successors — repoint (opt-in) and branch deletion

After the squash-merge lands, resolve the fate of this PR's branch and any **successors** (Step 1).
The new base for successors is `origin/master` (Case A) or `<base-branch>` (Case B).

Nothing retargets or rebases them for you. GitHub will re-point an open PR's *base ref* when its
base branch is **deleted**, but it never rebases the branch — so the successor's history still
carries the commits this PR just squashed, and its diff shows them until someone rebases. When the
branch is **not** deleted, not even the base ref moves. That gap is precisely what
`/repoint` exists for; there is no server-side restack to wait for.

### 4a. Successor repoint (opt-in)

If open PRs base on this branch, deleting it would **close** them (golden rule 2). List them and let
the user **opt in** to repointing each onto the new base:

> This branch has open successors: **#\<n\> \<branch\>**, … Repoint them onto **\<new-base\>** so they
> stay open and their diffs stay clean? *(all / pick which / none — leaving them keeps this branch)*

- **Opt in** → repoint each chosen successor onto the new base:
  - `/repoint` for that PR. It recomputes the effective
    base by climbing `parents` and skipping merged ancestors, `patch_pr_base`es the open PR, rebases
    the branch with `git rebase --onto` under a `git merge-base` guard, and force-pushes with
    `--force-with-lease=<branch>:<expected-sha>` — a concurrent child push aborts the repoint rather
    than clobbering the child's work. `git rerere` is on, so a conflict resolved once replays.
  - **Ad-hoc chain** → `/repoint <new-base>` for each successor.

  Do this **after** the squash-merge landed, so the merged commit drops out of each successor's
  diff. A rebase conflict is surfaced, not swallowed: resolution
  syncs the branch, returns the conflicted paths and marks the node `has-conflicts`; resolve them in
  that node's worktree and re-run the tool to confirm a clean tree.
- **Decline (or no successors chosen)** → leave those successors as-is. The branch still has open
  references, so it MUST NOT be deleted (skip 4b for it).

### 4b. Branch deletion (default when safe; opt-out)

**Delete the squashed branch once no open PR still bases on it** — this is the default. Re-check after
any repoints:

```bash
gh pr list --state open --base <this-branch> --json number --jq 'length'   # must be 0 to delete
```

- Result `0` → delete the branch: `git push origin --delete <this-branch>` (drop the local branch too
  if it lingers: `git branch -D <this-branch>`). If a worktree still holds it, remove that worktree
  first — `git worktree list`, then `git worktree remove <path>` — and **ask before removing a
  worktree that belongs to a live session**; it is somebody's checkout.
- **Opt-out:** if the user asked to keep it (e.g. `keep-branch` / `--no-delete` / "don't delete" in
  the invocation), skip deletion and say the branch was kept by request.
- **Still referenced** (user declined a repoint) → **keep the branch** and report that it survives
  because open successors depend on it.
- **Deleting the remote branch is not the same as removing the PR from the stack.** Re-register the
  remaining PRs explicitly; nothing infers it from a deleted branch.

Never pass `--delete-branch` to `gh pr merge` — branch deletion happens here, only after 4a is
resolved.

## Step 5: Report

State the CI status that was verified (and the scope of `scripts/ci-status.sh` you ran), which merge
path was used — a direct `gh pr merge --squash`, or `#automerge`, or `#forcemerge` **only if the user
explicitly asked** — and that the PR reached a mergeable state; which case ran (A or B); the squash
target (master or the base PR); which stack model applied (planned / ad-hoc); which successors were
offered and which were repointed (`/repoint`); whether
the branch was deleted, kept by request, or kept because it is still referenced; whether
`delete_branch_on_merge` forced the ordering; and the resulting open-PR set
(`gh pr list --state open`), and the re-registration (`gh stack link`) so the stack matches
what landed. Never use `--no-verify`.

## Related

**Commands**: `/repoint`, `/merge-pr-stack`, `/fix-pr`, `/merge`, `/pr`, `/pr-wrap`,
`/wrap-context-docs`
**Skill**: `pr-stack` (`.agents/skills/pr-stack/SKILL.md`)
**Specs**: [`docs/ft/coder/pr-stacking.md`](../../docs/ft/coder/pr-stacking.md),
[`docs/ft/coder/pr-stack-docs.md`](../../docs/ft/coder/pr-stack-docs.md),
[`docs/ft/coder/pr-stack-live-status.md`](../../docs/ft/coder/pr-stack-live-status.md)
**Guide**: [`docs/dev/guides/ci.md`](../../docs/dev/guides/ci.md)
