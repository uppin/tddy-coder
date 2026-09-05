---
description: Start and switch to a new branch based on a referenced branch or PR (e.g. a stacked follow-up off an open PR) - branch only, never opens a PR
---
## Follow-up Branch

Create and switch to a **new branch based on a referenced branch or PR** — for continuing work on top
of something that hasn't merged yet, most often a **stacked successor** off an open PR. `$ARGUMENTS`
carries the reference (a branch name, a PR number like `#123`, a PR URL) and, optionally, a name for
the new branch.

This is the building block for extending a stack by hand. It **does not open a PR**. To add a node
**and** open a draft PR, use **`/add-to-pr-stack`**. For the full stacked-PR model (base tracking, the
DAG, per-PR documents, landing rules) use the **`pr-stack` skill**
(`.agents/skills/pr-stack/SKILL.md`) and
the `pr-stack` skill.

## Step 1: Resolve the reference to a base branch

Interpret `$ARGUMENTS`:

- **PR** (`#123`, `123`, or a `github.com/.../pull/123` URL) → resolve its head branch:
  ```bash
  gh pr view <ref> --json number,headRefName,baseRefName,url,state,isDraft,mergeStateStatus
  ```
  Base branch = `headRefName`. Note the PR's `state` — a follow-up normally targets an **open** PR. If
  it is already **merged**, tell the user: its head branch may have been deleted on `origin`, and the
  right base is then `origin/master` (or the nearest non-merged ancestor). A **draft** PR is a fine
  base — a draft is a live node as far as the stack is concerned, and publishing an interface early so
  dependents can branch off a real ref is exactly what the `## Draft PR contract` heading in a node's
  `changeset.md` is for.
- **Branch name** → use it directly.
- **Nothing given** → ask which branch or PR to base on. **Do not** silently base on the current
  branch or `master`.

## Step 2: Say whether the reference is part of a stack, before branching

A branch off an open PR's head is a stack member whether or not anybody says so. Detect it the way
`/pr` does: `gh pr list --state open --json number,headRefName,baseRefName` plus
  `git merge-base --is-ancestor origin/<headRefName> HEAD`, treating any base that is not
  `master`/`main` as a stack parent. This is the case this command serves directly.

## Step 3: Fetch the base and branch off it

```bash
git status --porcelain | grep -v '^?? '        # tracked changes must be empty
git fetch origin <base-branch>                 # ref-scoped, not a bare fetch
git switch -c <new-branch> origin/<base-branch>   # off the fetched ref, not stale local state
```

- **New branch name**: use the name from `$ARGUMENTS` if given; otherwise propose one and confirm. Use
  the stack's namespace — `feature/<stack-slug>/<node>` (`feature/auth/middleware`), so a stack's
  branches group together in a branch list and pair with the `(#<slug> K/N)` group in the titles.
- If the base branch only exists locally (no remote), branch off the local ref instead and say so — the
  PR cannot be opened until the base is pushed.
- If the working tree has tracked-file changes, stop and let the user commit them first — never
  discard work to switch branches, and never `--no-verify`. Untracked files may stay.

## Step 4: Report and point at the next step

State the resolved base (and the PR it came from, if any), the new branch, and that HEAD switched to
it. Remind the user of the base relationship for when they open the PR:

> When you open the PR for `<new-branch>`, set its **base to `<base-branch>`** (not `master`) so the
> stack relationship holds: `gh pr create --draft --base <base-branch>`, or `/add-to-pr-stack`, which
> does that plus the title numbering. `/pr` detects a stack base on its own but confirm what it picked.
> Once `<base-branch>` merges, `/repoint` moves this PR onto `origin/master`.

Also state, in one line, whether this branch is meant to become a PR **in the stack** — in which
case it must be registered with `gh stack link` once its PR exists — or a scratch branch outside it.
That is the one fact invisible from git afterwards.

Do not open a PR here — that is `/add-to-pr-stack`, `/draft-pr`, or `/pr`. This command only creates
and switches to the branch.

## Rules

- Resolve the reference explicitly (branch / PR / ask) — never default to `master` or the current
  branch silently.
- Base off the freshly fetched ref, ref-scoped, so the new branch starts from the reference's latest.
- **Say whether the branch is destined for the stack.** A branch off a stack member that never gets
  registered is correctly based but outside the stack.
- Never discard uncommitted work to switch branches.
- Never open a PR, and never change any existing PR's draft/ready state.
- Never use `--no-verify`.

## Related

**Commands**: `/add-to-pr-stack` (same add-on-top, but opens a **draft PR** and renumbers the stack),
`/draft-pr` (draft PR from work already in the tree), `/split-pr-to-stack` (carve an existing PR into
stacked slices; this command only **adds on top**), `/split-branch`, `/pr`, `/pr-stack-rebase`,
`/repoint`, `/merge`, `/squash-pr`
**Skill**: `pr-stack` (`.agents/skills/pr-stack/SKILL.md`)
**Product docs**: the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`)
