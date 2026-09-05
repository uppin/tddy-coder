---
description: Add a new PR on top of a named predecessor branch - cut the branch, open a draft PR against it, and re-register the stack
---
## Add to PR Stack — a new PR on a named parent

Start **new work** as a stacked PR **without waiting** for the rest of the stack to merge: create a
new node on an existing parent, commit the new work, and open a **draft** PR whose base is that
parent's branch.

`$ARGUMENTS` may carry:

- a **parent** — PR number (`#123` / `123`), PR URL, branch name, or a planned node id (`n2`)
- a **name** for the new branch
- a short description of the new work

This is the command that **opens the PR**. `/follow-up-branch` only creates and switches to a branch;
it never opens a PR and never silently defaults to the current branch or `master`. The **`pr-stack`
workflow recipe** (`tddy-coder --goal ...`, or the web New-session screen with recipe `pr-stack`)
plans a whole stack from a feature description. `/split-pr-to-stack` carves an existing PR into
slices.

Load the **`pr-stack` skill** (`.agents/skills/pr-stack/SKILL.md`) first, and read
the `pr-stack` skill — the stack is a product feature
here, not a `gh` extension, and this command has to say which of its two shapes it is operating on.

## What owns the stack

The order is the chain of PR base refs, plus the **registered stack** on GitHub (`gh stack`, see the
`pr-stack` skill). Adding a PR therefore has three parts: cut the branch off its predecessor, open
the PR against that branch, and **re-register** so the new PR joins the stack.

```bash
gh stack link --base master <pr> <pr> ... <new-pr>    # additive; never removes a PR
```

Opening the PR and forgetting the registration leaves it correctly based but outside the stack —
reviewers get the right diff and no context.

## Before anything: the node must be a whole PR

A new node must be **independently reviewable and independently mergeable** — the API/schema change,
the code implementing it, and its tests land in **one** PR. **Splitting by layer is forbidden**: a
node that ships only surface (an RPC returning `unimplemented`, a field nothing reads, a trait with
stub impls) is not a valid PR here, even as a deliberate first layer. When the slice is too large,
**split by capability, not by layer** — one source variant rather than all of them, one enum case,
one entry point, the happy path before the edge cases; each part still end-to-end.

Two narrow exceptions: a purely mechanical rename/move/extraction with no behaviour change, or a
regeneration of already-committed generated code exposing no new surface.

Full rule:
the `pr-stack` skill § *The PR boundary contract*.
If the work the user described is a layer split, say so and propose the capability cut before creating
anything.

## Default parent

Unless the user named a parent, the new PR sits on the **branch checked out here**, and only if that
branch is itself in a stack (Step 1 proves it). If the current branch is `master`, or is not in a
stack, **ask** — do not guess, and never silently default to `master`.

Unlike a linear stack-extension model, this repo's stack is a **DAG**: `parents` is a list, and two nodes
may share a parent. So a parent that already has a successor is **not** an error — it makes the new
node a **sibling**, which is legitimate and often what the user wants. Say plainly which it is:

| Named parent | What it means | What to do |
|---|---|---|
| A node/branch with no successors | the new node extends the stack | proceed |
| A node/branch that already has successors | the new node is a **sibling** of those | proceed, but **state it**, and confirm the two really are independent — siblings that touch the same surface will conflict at merge time |
| The new work genuinely depends on **several** open PRs | it sits **after all of them** | a stack is a line: place the new PR above the last one it depends on, so every dependency is an ancestor. Record what it takes from each in `## Dependencies` |
| A **merged** PR's branch | the parent is gone | base on `origin/master` instead (or the nearest non-merged ancestor) and say so — this is `/repoint`'s rule applied at creation time |
| A branch that does not exist on `origin` | nothing to base on | **stop**. The predecessor must push its branch first |

## Step 0: Preflight

```bash
gh auth status
git status --porcelain | grep -v '^?? '
```

If there are **tracked-file** changes, **stop** and ask the user to commit or stash them. Never
discard uncommitted work to switch branches. Untracked files may stay in the tree; do not add them
unless they belong to this new work.

Never `git add -A` / `git add .` when staging the new node — that scoops unrelated files into the new
PR, and the point of a stacked PR is a diff a reviewer can hold in their head.

## Step 1: Resolve the stack and the parent

```bash
gh pr list --state open --json number,headRefName,baseRefName,title,isDraft
git worktree list
```

- Read the chain off the open PRs' base refs, mirrored to
  each PR's `docs/dev/1-WIP/` changeset.
- **Ad-hoc chain?** Detect it the way `/pr` does: for each open PR's `headRefName` other than the
  current branch, `git merge-base --is-ancestor origin/<headRefName> HEAD`; the ones that pass are
  stack ancestors, and the closest is the stack parent (smallest
  `git rev-list --count origin/<headRefName>..HEAD`). Any base that is not `master`/`main` is a stack
  parent.
- **Neither, and `$ARGUMENTS` names nothing** → **ask** which parent to build on. Do not guess.

Apply the parent table above. Then confirm the new branch name: use the name from `$ARGUMENTS` if
given; otherwise propose one and confirm.

**Branch naming.** The convention is
(`validate_stack_plan`): `feature/<stack-slug>/<node>` — every node in one stack shares a single
`feature/<stack-slug>/` namespace (`feature/auth/token-store`, `feature/auth/middleware`), so the
stack's branches group together in every branch listing. Ad-hoc chains have no enforced convention;
use the same shape anyway.

If the proposed branch already exists, stop.

## Step 2: Branch off the predecessor

```bash
git fetch origin <parent-branch>
git switch -c <new-branch> origin/<parent-branch>   # off the fetched ref, not stale local state
```

Base off the **fetched** ref so the new branch starts from the parent's latest. If the parent branch
exists only locally (no remote), branch off the local ref and say so — but note the PR cannot be opened
until the parent is pushed.

Stay on `<new-branch>`. The point is to start the new work here.

## Step 3: Commit the new work, then open a draft PR

GitHub will not open a PR for a branch with no commits ahead of its base.

- If the new functionality is ready, stage **only those files**, commit, then open the PR.
- If there is nothing to commit yet, **do not** create an empty commit and **do not** open a PR.
  Report the new branch, the resolved base, and that the PR opens once there is a commit.
  `/follow-up-branch` is the better fit when the user only wanted a branch.

When there is a commit:

```bash
git push -u origin <new-branch>
gh pr create --draft --base <parent-branch> --head <new-branch> \
  --title "<title>" --body "<body>"
```

**`--draft`, always.** A new stacked PR is a draft until its own work is finished; the draft state is
also how a node publishes its interface early so dependents can branch off a real ref (see
`/draft-pr` and the `## Draft PR contract` heading in the node's `changeset.md`). Never mark a
**predecessor** ready-for-review or draft as a side effect of adding this node — their state is not
yours to change. `pr_status.phase` records a draft as `open`, so a draft node is a live node as far as
the stack is concerned.

**Re-register the stack** so the new PR joins it — `gh stack link --base master <prs…>`, in order,
bottom to top. `link` reuses the PRs that already exist and removes none, so re-running it with the
full list is the whole step. The new PR
reference. Adoption is refused if that head branch or pull number is already tracked, so it cannot
double-track.

Never `--no-verify` on the commit or the push.

### Title the new PR, and renumber the ones below it

Adding a node changes `N` for every other PR in the stack, and a stale `3/5` on a six-PR stack is read
as fact.

1. **Read the stack slug** — the `feature/<stack-slug>/` namespace the branches already share, or the
   slug used in the existing PR titles. It was chosen at planning and never changes. Do not invent a
   second one.
2. **Title this PR** as `<type>(<package>): <what it delivers> (#<slug> <new-K>/<new-N>)`, e.g.
   `feat(tddy-daemon): sandboxed workspace resume (#pr-stack 3/6)`. The subject states the delivery as
   shipped — never `red`, `stubs`, `WIP`, or `phase N`, which would describe a layer split this repo
   does not allow anyway.
3. **Renumber every open PR below it**, subject unchanged, only the trailing group moving:

```bash
gh pr edit <pr> --title "<type>(<pkg>): <unchanged subject> (#<slug> <K>/<newN>)"
```

Plain `gh pr edit --title` is correct here. **Skip merged PRs** — their titles are already commits on
`master`, and editing the PR page only suggests history was corrected when it was not.

## Step 4: Report

State:

- the **stack** the PR joined, and the `gh stack link` output confirming it;
- the new node id / branch, its **parents**, and whether it extends the stack or is a **sibling**;
- the new **draft** PR URL, or that no PR was opened and why;
- the stack topology after the change (`gh pr list --state open --json number,headRefName,baseRefName`,
  );
- every title you renumbered, and any merged PR you deliberately skipped;
- any node now showing stale against its base — `/pr-stack-rebase` is the per-branch fix, run from the
  worktree that owns each branch. Do not rebase branches the user did not ask about.

Remind: merge order is still bottom-up. This PR must not merge before its parents — that is
`/merge-pr-stack`'s job.

## Rules

- **Re-register the stack** after opening the PR (plain git +
  `gh`). Never mix the two in one run.
- **Never leave a new PR unregistered.** A correctly-based PR outside the stack gives reviewers the
  right diff and no context.
- **A node is a whole PR.** No stubs-only layer, no "greenable once PR N lands". Split by capability.
- **Never default the parent to `master` or the current branch silently.**
- **Siblings are allowed** — this is a DAG. Say when you are creating one.
- **New PRs are drafts.** Never flip a predecessor's draft/ready state.
- **Title the new PR and renumber the stack in the same run.** `K/N` that no longer matches is worse
  than absent. Never renumber a merged PR.
- **Branch name follows `feature/<stack-slug>/<node>`** — one namespace per stack, conventional
  elsewhere.
- Never `git add -A`/`.`, never discard uncommitted work, never `--no-verify`.
- Stay on the new branch when the command finishes.

## Related

**Commands**: `/follow-up-branch` (branch only, no PR, never defaults the base), `/draft-pr` (draft PR
from work already in the tree), `/split-pr-to-stack` (carve an existing PR; this command only **adds
on top**), `/split-branch`, `/pr`, `/pr-stack-rebase`, `/repoint`, `/merge`, `/merge-pr-stack`
**Skill**: `pr-stack` (`.agents/skills/pr-stack/SKILL.md`)
**Product docs**: the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`)
