# Building the integration branch

The mechanics behind [`SKILL.md`](../SKILL.md) § 2. The goal is a single throwaway branch whose tree
is the changeset's finished state and whose diff against one base commit is the whole change — so
that a stack can be measured the way a single PR is.

Everything here happens in `tmp/eval-changeset/<slug>/` (gitignored) on a branch named
`eval/<slug>`. Neither is ever pushed.

## Pick the base — the fork point, not today's trunk

```bash
git fetch origin
BASE=$(git merge-base origin/master origin/<tip>)
```

`origin/master` moves. Diffing against its current head attributes every commit that landed while
the stack was in flight to the stack. `git merge-base` gives the commit the work actually forked
from, which is what the change is a delta *to*.

If a branch drifted out of line (below), `git merge-base` accepts n commits and returns their best
common ancestor:

```bash
BASE=$(git merge-base origin/<top> origin/<stray>)
```

Rebased at different times, that ancestor can sit further back than either branch's own fork point.
Check how much unrelated trunk work that pulls in, and say so if it is material:

```bash
git rev-list --count "$BASE..origin/master"   # trunk commits since the fork point
```

## Verify the line before you squash

A stack in this repo is a registered `gh stack` object and is therefore **linear**: every PR's branch
should be an ancestor of the top one. Verify that — do not assume it from the PR bases, which
describe intent rather than history:

```bash
TOP=<top-branch>
for N in <stack-branches>; do
  git merge-base --is-ancestor "origin/$N" "origin/$TOP" \
    && echo "covered: $N" || echo "NOT covered: $N"
done
```

Every branch `covered` means one squash captures the whole changeset. Any `NOT covered` is a branch
that drifted out of line — a missing restack, or a shape that was never linearized — and needs the
extra path below.

## The normal case — one squash

```bash
SLUG=<slug>; WT=tmp/eval-changeset/$SLUG
git worktree add --detach "$WT" "$BASE"
git -C "$WT" checkout -b "eval/$SLUG"
git -C "$WT" merge --squash "origin/$TOP"
git -C "$WT" commit -m "eval: $SLUG squashed changeset"
```

`git diff origin/master...origin/$TOP` produces the identical diff, and for a quick number that is
enough. The branch exists for what a diff cannot give: a browsable integrated tree, and somewhere to
run `/analyze-clean-code`, `tddy-tools analyze` or a per-package `./test`.

## A branch out of line — squash it in separately

Squash the top branch first, then each stray in stack order (lowest first):

```bash
git worktree add --detach "$WT" "$BASE"
git -C "$WT" checkout -b "eval/$SLUG"
for L in "origin/$TOP" <strays in stack order>; do
  git -C "$WT" merge --squash "$L" || echo "CONFLICT at $L — resolve, and record it"
done
git -C "$WT" commit -m "eval: $SLUG squashed changeset (reconstructed)"
```

**A conflict here is a finding, not a setup problem.** It means two PRs of the stack edited the same
code without one being restacked on the other. Record it under *parallel change* friction with the
conflicted paths, resolve in favour of the later branch, and state in the report that the integrated
tree is a reconstruction rather than a tree that ever existed.

## Local, unpushed branches

Use local refs throughout — `<branch>` instead of `origin/<branch>` — and say in the report that the
evaluated state is local and may differ from what any PR shows. Check for the gap:

```bash
git rev-list --count "origin/<branch>..<branch>"   # unpushed commits
git status --porcelain                             # uncommitted work is NOT in the eval branch
```

Uncommitted changes are invisible to every command in this skill. Either commit them first or say
they were excluded.

## A single PR needs no worktree

```bash
BASE=$(git merge-base origin/master origin/<head>)   # or the base branch, for node-alone evaluation
git diff --numstat "$BASE"..origin/<head>
```

Build the worktree anyway when you intend to run analyzers or tests on the tree — that is the only
thing it buys here.

## Cleanup

```bash
git -C "$WT" status --porcelain    # expect empty — nothing of value lives here
git worktree remove --force "$WT"  # --force: build artifacts, if any, are expected
git branch -D "eval/$SLUG"
git worktree prune
```

- **Never push `eval/<slug>`.** It is a reconstruction with a fabricated commit; on the remote it
  would look like a real branch someone could base on.
- **Never delete a stack branch.** It is its successor's base, and deleting it closes that PR
  unreopenably (`pr-stack` golden rule 2). This cleanup touches only the eval branch.
- If the worktree removal takes a while, it is deleting a `target/` you created with the optional
  deep pass; reasonable to run in the background.
