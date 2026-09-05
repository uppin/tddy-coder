---
description: Split a PR (or unpushed branch) into a stacked PR sequence with correct bases - A→B→C split B becomes A→B1→B2→C
---
## Split PR to Stack — Carve one PR into stacked PRs

Turn **one** branch/PR into a **stack** of smaller PRs, each based on its predecessor — not on
`master`. This is the stacked counterpart of `/split-branch`.

Load the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`) first — it collects the stack model,
base tracking, the per-PR documents, the forward-only doc linking rule, the golden rules (a deleted
base branch **closes** its dependent PR), and the worktree-pinning constraint. The authoritative
product specs are `docs/ft/coder/pr-stacking.md` (stack data model, PR-management tools, the PR
boundary contract), `docs/ft/coder/pr-stack-docs.md` (the per-PR `PRD.md` / `changeset.md`), and
`docs/ft/coder/pr-stack-live-status.md` (live status, repoint, base sync). This command assumes
those definitions.

**Use this, not `/split-branch`**, when the slices must land as dependent PRs
(`B1` mergeable first, `B2` based on `B1`). `/split-branch` produces sibling branches, typically
both off `master`, and registers no stack.

**Plan the stack, not split it**, when the work is not written yet — that is `/plan-pr-stack`. This
command carves an **existing** PR; it does not plan from requirements. Note that `plan-pr-stack` is
also the name of a tddy **workflow recipe**, which is a separate product feature documented in
`docs/ft/coder/pr-stacking.md`; a
different thing from the slash command — see the `pr-stack` skill § *Two ways to plan a stack*. This
command carves up **existing** commits/files.

`$ARGUMENTS` may name the PR/branch to split (`#123`, a URL, a branch) and/or describe the two
slices. Default: current branch. **This command always produces exactly two new layers (B1, B2).**
A third slice is a second invocation, on B1 or B2. Confirm the distribution before touching git.

## The boundary contract governs the split — read this before proposing slices

This is the rule that decides whether a split is allowed at all, and it is not negotiable. From
`docs/ft/coder/pr-stacking.md` § [PR boundary contract: every node is
self-contained](../../docs/ft/coder/pr-stacking.md#pr-boundary-contract-every-node-is-self-contained):

> A planned PR must be **independently reviewable and independently mergeable**: the API/schema
> change, the code implementing it, and its tests land in **one** node.

**Splitting by layer is forbidden.** Both halves of a layer split are invalid PRs, so a split that
produces one is worse than not splitting at all:

| ✗ Layer split (invalid B1 → B2) | ✓ Split by capability |
|---|---|
| B1 adds proto RPCs → B2 implements them | B1 attachment staging end-to-end: proto + daemon handler + tests; B2 the next capability |
| B1 adds an endpoint → B2 adds its handler | B1 the endpoint serving real responses; B2 a second endpoint or a second source variant |
| B1 adds a data model → B2 persists it | B1 the model **with** its persistence; B2 the reader that consumes it |
| B1 changes a signature → B2 fills in the body | B1 the working function; B2 the second call site / second enum case |

A slice that ships only **surface** — RPCs returning `unimplemented`, a field nothing reads, a trait
with stub impls — is not a valid PR. It cannot be reviewed for correctness (there is no behaviour to
check) and it cannot be tested beyond compiling.

**When the vertical slice is too large, cut by capability, not by layer**: one source variant rather
than all of them, one enum/scope case rather than the full set, one screen or entry point, the happy
path before the edge cases. Each slice carries its own contract *plus* behaviour *plus* tests.

**Two narrow exceptions**, and only these: a purely mechanical rename/move/extraction with no
behaviour change, or a regeneration of already-committed generated code exposing no new surface.
Do not invent a third — if the boundary is debatable, say so in the node's `description` and let a
human decide.

**Practical consequence for this command.** In Step 2 you must be able to name, for **B1 alone**,
what a reviewer can judge and which tests prove it. If the honest answer is "you have to see B2 to
know whether B1 is right", the proposed boundary is a layer split: re-cut it, or report that this
branch should not be split and stop. Do **not** paper over it with `unimplemented!()`, a `TODO(pr-2)`
stub, or a fallback.

The `## Draft PR contract` heading in a node's `changeset.md` is **not** a licence to ship a
stubs-only PR. It means: publish the interface early *inside a PR that will still ship its own
implementation*, so dependents can branch off a real ref while the implementation continues in that
same PR.

## What owns the topology

The topology **is** the set of PR base refs on GitHub, plus the **registered stack** that groups and
orders them (`gh stack`, see the `pr-stack` skill). A split therefore has three outputs, not two:
the git rewrite, the retargeted bases, and a **re-registration** so the new PR takes its place in the
stack.

Detect the stack you are in before Step 1 and say so out loud:

```bash
gh pr list --state open --json number,headRefName,baseRefName   # a base that is not master/main
```

Branch naming: every branch of one stack shares `feature/<stack-slug>/<node>`. Keep the new slice in
that namespace.

**A split never destroys and rebuilds the stack.** `gh stack link` is additive — it reuses existing
PRs and removes none — so re-running it with the full list in order is the whole registration step.
Never close a PR to reshape a stack: that loses its review history, and a closed PR cannot be
re-based via the API.

## Topology this command produces

```
Before (B may or may not have a PR; C, D may or may not exist):

  A  →  B  →  C  →  D
        ▲
     the PR/branch being split

After:

  A  →  B1  →  B2  →  C  →  D
        ▲       ▲
   B's existing    new PR
   PR (kept)       (base = B1)
                   C's base moves B → B2
                   D's base stays C (C is rewritten; D is rebased onto the new C)
```

- **B1** is the **same GitHub PR as B** when B already has one (same number, same head branch, same
  predecessor A). Its diff shrinks to the bottom slice. Do **not** close B and open two new PRs —
  that drops review history, and a closed PR cannot be re-based via the API.
- **B2** is a **new** branch + PR whose base is B1. After the split, **B2's tree equals original B**
  so every child of B rebases onto it without picking up a second copy of B1.
- **Every child of B** (C, and any sibling of C) is retargeted onto **B2**. Descendants further down
  (D, …) **keep their existing parent** — D stays based on C — but each must be rebased onto that
  parent's new tip. Never leave a child of B basing on B1: its diff would swallow B2.
- **A** is unchanged. Resolve it — it is **not** always `master`.

If B has **no PR**, both B1 and B2 are created as a two-PR chain, with B1 based on the resolved
predecessor A (which may itself be an open PR).

**The stack is a DAG, not a line.** Unlike a linear stack model, two open PRs that both base on B are
a legitimate shape here, and a node may have several parents. So sibling children are **not** an
error — the default is that *all* of them move onto B2 (B2's tree equals original B, so their diffs
are unchanged). Ask only when a sibling genuinely wants the B1 slice alone as its base. What you must
never do is silently leave one child on B1 and another on B2.

## Step 0: Preflight

```bash
gh auth status
git status --porcelain                 # uncommitted tracked files → commit first, never stash silently
git branch --show-current
git worktree list
gh pr list --state open --json number,headRefName,baseRefName
```

Then establish the stack context:

- Read the chain off the open PRs' base refs — that, plus the registered stack, is the topology
  source of truth for Step 1c. Note that `gh pr list` cannot show work that is planned but
  nodes that have no PR yet but still belong in the order.
- **Ad-hoc chain** — the `gh pr list` output above plus `git merge-base --is-ancestor` is all there
  is; use the detection procedure `.agents/commands/pr.md` § "Determine the PR base (stack-aware)"
  already specifies, and treat any base that is not `master`/`main` as a stack parent.

Worktree pinning is re-checked in Step 1d **after** A, B, the child set, and trunk are known.

## Step 1: Resolve B, A, the children, and local refs — MANDATORY

### 1a. Resolve B and A

Identify **B** from `$ARGUMENTS` or the current branch:

```bash
gh pr view <B> --json number,url,title,isDraft,state,baseRefName,headRefName,reviewDecision
```

- **PR exists** → `headRefName` is B, `baseRefName` is the GitHub base (candidate for A).
- **No PR** (`gh pr view` fails) → B is the current (or named) branch. Resolve A in this order:
  1. Explicit argument.
  2. The closest open-PR head branch that is an ancestor of B, per the `.agents/commands/pr.md`
     procedure. A merged predecessor contributes nothing — the base may have already collapsed to
     `origin/master`.
  4. This branch's changeset in `docs/dev/1-WIP/`, if it records a base.
  5. `git merge-base` / upstream vs `origin/master` — **ask** if it is not obvious.
  6. **Never silently default to `master`.** If B was cut from an open PR, A is that PR's head.

### 1b. Materialize every needed branch locally

`gh pr view` can succeed for a URL/number while `git rev-parse` / `switch` / `rebase` fail if the
ref is remote-only. **Fetch and create a local branch** for B, for A when A is a branch (not trunk),
and for every child discovered in 1c. Do this **before** any `git rev-parse` of those names.

```bash
git fetch origin <branch>
if git show-ref --verify --quiet "refs/heads/<branch>"; then
  git merge-base --is-ancestor "origin/<branch>" "<branch>" \
    && echo "<branch> contains origin/<branch>" \
    || { echo "local <branch> does not contain origin/<branch> — stop, do not force-update a diverged local"; exit 1; }
else
  git branch --track "<branch>" "origin/<branch>"
fi
```

If the local branch has **unpushed** commits the remote lacks, **stop** and ask — never discard them
to match origin. A branch may live only inside somebody else's worktree; `git fetch origin <branch>`
still works once they have pushed, and if they have not, they own unpushed work and you must ask
before rewriting anything.

### 1c. Walk the descendants

Discover the **direct children of B** first:

```bash
# Ad-hoc chain — direct children of a branch:
gh pr list --state open --base <branch> --json number,headRefName,url
```

Work that is planned but has no PR yet is invisible to `gh pr list`. If the plan names a PR that
will be cut off B, it must be re-pointed at B2 too — ask before assuming the list is complete.

1. **Zero children** → B is the top; there is nothing below to move.
2. **One or more children** → **all** of them are retargeted onto B2 by default (B2's tree equals
   original B, so no child's diff changes). Confirm the list with the user. Only leave a child on B1
   when the user explicitly wants it based on the B1 slice alone — and then say in the output that
   the stack has forked.
3. **Recurse one hop at a time** for each child: C's own children (D, …) **keep C as their parent**.
   They are not retargeted; they are only rebased onto C's new tip after C is rewritten.
4. **A multi-parent (diamond) node** among the descendants keeps its other parents untouched. When
   you re-parent it, you must name the **complete** new parent list (`[B2, X]`, not `[B2]`) — see
   Step 6.

Record each descendant's **original tip SHA** (`orig_C`, `orig_D`, …) after 1b, before any rewrite.
State A, B, the descendant tree, the retarget decisions, and whether B already has a PR **before**
splitting.

### 1d. Free every branch the split will move — before any rewrite

A `git rebase` / `git switch` cannot touch a branch that another worktree has checked out, and in
this repo child sessions each hold their own worktree under `<repo>/.worktrees/`. Discovering this
mid-rewrite leaves half a split on disk.

Build the pin-set: **trunk** (usually `master`), **A** (if it is a branch), **B**, the to-be-created
**B2**, and **every descendant** you will rebase.

```bash
git worktree list
```

If any pin-set branch is checked out in **another** worktree, free it, or run from a worktree that
does not pin them. If **this** worktree is on a pin-set branch, that is fine — the git split checks
B out itself. **Never rewrite a branch whose child session is actively running**: stop the session
or ask its owner first. `--force-with-lease` will abort rather than clobber a concurrent child push,
which is the desired failure, not a workaround for skipping this check.

## Step 2: Agree the two slices — MANDATORY

Do not split until the user confirms:

1. **What stays on B1** (bottom slice — independently reviewable and independently mergeable, lands
   first).
2. **What moves to B2** (top slice — based on B1).
3. **Branch name for B2.**
   - `feature/<stack-slug>/<node>`, using the **same** `<stack-slug>` as every other branch in the
     stack. The shared namespace is what groups a stack in a branch list, and it pairs with the
     `(#<slug> K/N)` group in the titles.
   - **Keep B's branch name as B1** so the existing PR's head does not move, and — in a planned
     stack — so the node stays bound to it. The branch is the durable link key.
4. **Boundary check against the contract** (see the section above). Write down, in one sentence
   each: what B1 delivers that a reviewer can judge on its own, and which tests in B1 prove it. If
   B1's tests would have to assert B2's behaviour, or B1 would ship an `unimplemented!()` / a
   `TODO(pr-2)` stub as its deliverable, the boundary is a **layer split** — re-cut by capability or
   stop.
5. **Independence check.** B1 must not call symbols that only exist on B2. If it currently does,
   those calls belong on B2, or the split is inverted. Do not paper over this with a fallback —
   fallbacks are forbidden without explicit developer consent (`CLAUDE.md`).

**Exactly two layers.** If the user wants B3, say so and stop: finish this split, then run
`/split-pr-to-stack` again on B1 or B2. Do not invent a third branch in this invocation.

Present the proposed topology (`A → B1 → B2 → [C → D → …]`) and wait for approval.

## Step 3: Backup and split document

**Keep backups frozen (never modify):**

```bash
ts=$(date +%Y%m%d-%H%M%S)
git branch backup/${B}-${ts} <B>
# backup every descendant that will be rebased (C, D, …)
```

Record `orig_B=$(git rev-parse <B>)` and `orig_<X>` for each descendant. Create
`tmp/split-pr-stack-${ts}.md` (`tmp` is gitignored — do **not** add it to git) with: original SHAs,
A/B/descendant branches and PR numbers, the two slice lists, the retarget decisions, and a checklist
of the procedure below.

## Step 4: Git split — B2's tree must equal original B

Work in **one** worktree. B is checked out here; other worktrees must not pin the pin-set.

```bash
orig_B=$(git rev-parse <B>)
git branch <B2> "$orig_B"            # B2 starts as a copy of original B
```

**4a. Shrink B to B1** (bottom slice only):

```bash
git switch <B>
# Remove B2-only files/changes. Shared files keep the B1 version.
git add -A
git commit -m "Split: keep [B1 slice] on this PR"
```

Never `--no-verify`. B1 must **build**:

```bash
cargo build
cargo clippy -- -D warnings
```

If it does not build, stop — the slice boundary is wrong. Do not force-push a broken B1, and do not
add a stub to make it compile: a stub that exists only to let B1 build is the layer split the
boundary contract forbids.

**4b. Point B2 at B1 and commit only the B2 delta**, preserving original B's tree:

```bash
git switch <B2>
git reset --soft <B>                 # HEAD = B1; index + worktree still orig_B
git commit -m "Split: [B2 slice] stacked on predecessor"
```

**Invariant — verify before continuing:**

```bash
git diff "$orig_B" <B2>              # MUST be empty: B2 tree == original B
git log --oneline <A>..<B>           # B1's own commits only
git log --oneline <B>..<B2>          # B2's own commits only
```

If the diff against `orig_B` is not empty, the split corrupted the combined tree and descendants
will not rebase cleanly. Fix or restore from backup before touching GitHub.

**4c. Rebase every descendant**, each onto **its own parent**'s new tip. Direct children only is not
enough: after C is rewritten, D (based on C) is stale until it is rebased onto the new C.

```bash
# C was based on orig_B; replay C's unique commits onto B2
git rebase --onto <B2> "$orig_B" <C>
# D was based on orig_C; replay D's unique commits onto the new C (not onto B2)
git rebase --onto <C> "$orig_C" <D>
# …one hop at a time, in stack order, preserving each descendant's parent
```

Every direct child of B (including siblings of C) is rebased the same way as C
(`--onto <B2> "$orig_B"`) unless the user asked in Step 1c to leave it on B1. Do **not** rebase D
onto B2 — that would fold C into D's diff.

If a hop conflicts, restore from backup and re-examine the slice (usually a shared file that no
longer composes to `orig_B`). Resolve in the worktree that owns the branch, and only once the tree
here is sane.

**4d. Build and test each rewritten branch** (B1, B2, every rebased descendant):

```bash
./test -p <touched-package>          # fast loop while iterating
./test                               # full workspace before pushing
cargo clippy -- -D warnings
cargo fmt
```

`./test` also writes its output to `.verify-result.txt`; read that file rather than trusting an exit
code you cannot see. B1 must be independently mergeable: **its tests must not assert B2 behaviour**,
and it must not have lost the tests that prove its own slice. A B1 whose tests all moved to B2 is a
layer split that got through Step 2.

## Step 5: Point the GitHub bases — PRs that already exist

```bash
gh pr edit <N> --base <new-base>
```

Plain `gh pr edit --base` is fine in this repo. Verify afterwards with
`gh pr view <N> --json baseRefName`.

Order — **move the children onto B2 on GitHub before rewriting B's remote**. Otherwise a child stays
based on B while B shrinks to B1, and the child's PR diff swallows B2:

1. **Push B2**: `git push -u origin <B2>`. Open its PR now, based on B1:

   ```bash
   gh pr create --base <B> --head <B2> --title "<title>" --body "<summary + test plan>"
   ```

   (`<B>` here is B1's branch name — unchanged from the original B.) Follow
   `.agents/commands/pr.md` for the body shape; open it as a draft if the slice is not finished.
2. **Retarget every direct child of B** from B → **B2**. **Do not** retarget D, E, … — their base
   stays their current parent. Force-push the rebased descendants **top-down from B2**
   (`C`, then `D`, …) with `--force-with-lease`, back-to-back with each retarget so a child is never
   publicly based on a head whose tree does not match. **Golden rule: never delete B** — deleting a
   base branch closes every PR based on it.
   After this, C's diff is unchanged because B2's tree equals original B, and D's diff is unchanged
   because it still diffs against C.
3. **Force-push B1**: `git push --force-with-lease origin <B>`. B's existing PR now diffs against A
   as the B1 slice. **Warn first** if the PR is not a draft and has an approval — a force-push can
   dismiss it; you will need to re-request review, and `#automerge` armed on that PR should be
   re-checked against `docs/dev/guides/ci.md` § Automerge before you rely on it.

If B had **zero children**, skip step 2. If B had **no PR**, there is nothing to retarget and no
force-push of B1 — push both branches with `git push -u` and open both PRs with
`gh pr create --base …` (B1 based on A, B2 based on B1).

Never plain `--force`; always `--force-with-lease`.

## Step 6: Record the split in the stack — MANDATORY

Two things to record: the bases you set in Step 5, and the stack registration.

**Verify the bases:**

```bash
gh pr list --state open --json number,headRefName,baseRefName,changedFiles \
  --jq '.[] | "#\(.number) \(.headRefName) → base=\(.baseRefName) files=\(.changedFiles)"'
```

Each PR must show only its own files. An inflated count means a base is still pointing at the
pre-split branch.

**Re-register the stack**, bottom to top, with B2 in its new position:

```bash
gh stack link --base master <pr-A> <pr-B1> <pr-B2> <pr-C> ...
```

Pass `--base master` explicitly (it defaults to `main`) and **never `--open`** on a stack that
contains drafts — it marks them ready for review, which triggers each one's wrap.

## Step 7: Update the documents

Each PR carries its own PRD and changeset in `docs/dev/1-WIP/`, and the changeset must carry **all
four** headings:

```
## Responsibility        what this PR owns
## Boundaries            what it explicitly does not do
## Dependencies          per predecessor: what that PR delivers that this one consumes
## Draft PR contract     what lands first (API + failing tests) to unblock successors
```

Splitting B splits its documents too:

- **B1** keeps B's existing pair, narrowed to the bottom slice. `## Responsibility` shrinks;
  `## Boundaries` gains "does not do what B2 does".
- **B2** gets a new pair under its own slug. `## Responsibility` is the top slice, and its
  `## Dependencies` names what B1 delivers that it consumes — the section that stops whoever
  implements B2 from rebuilding B1's half.

If the work has a changeset in `docs/dev/1-WIP/`, split it the same way the code was split: the B1
changeset keeps the bottom slice, a new one covers B2, and each states what the other owns. Record each
new changeset as its own file in `docs/dev/changesets/` — `YYYY-MM-DD-<slug>.md`, one per node with
a **distinct slug**, per `docs/dev/guides/changelog-merge-hygiene.md`. **Never edit `packages/*/docs/` directly** — that goes
through the changeset workflow.

If there is no changeset, do not invent a whole planning pass. Report the gap; the PR bases are the
topology source of truth.

Commit document updates on the branch that owns them, then push. Never `--no-verify`.

## Step 8: Retitle B1 and B2, and renumber the whole stack — MANDATORY

A split changes `K` for everything above B and `N` for everything in the stack, so every title's
trailing `(#<slug> K/N)` is now wrong. A stale position is read as fact, so this is not optional.

1. **Read the slug** — it is the `<stack-slug>` in the stack's shared `feature/<stack-slug>/…` branch
   namespace. It was chosen at planning and does **not** change when a stack is split: B1 and B2
   belong to the same stack B did.
2. **B1 keeps its PR number and gets a new subject.** It now delivers only the bottom slice, and its
   old title described the whole of B. Rewrite it to what B1 actually ships.
3. **B2 gets a title for the top slice**, same slug, position `K+1`.
4. **Renumber every other open PR** in the stack — subject unchanged, trailing group only.

```bash
gh pr edit <N> --title "<type>(<pkg>): <subject> (#<slug> <K>/<newN>)"
```

**Skip merged PRs.** Their titles are already commits on `master`; editing the PR page only suggests
history was corrected when it was not.

Subjects must state the delivery **as shipped** — never `red`, `stubs`, `WIP`, `phase N`. There is no
commit-message length hook in this repo, but keep titles under ~70 characters as
`.agents/commands/pr.md` asks.

## Output

```markdown
## Split PR → stack complete

### Context
- Stack: `<stack-slug>`, <n> PRs

### Topology
A (`<A>`) → B1 (`<B>`, PR #<n1>) → B2 (`<B2>`, PR #<n2>) → C (`<C>`, PR #<n3>) → D (`<D>`, PR #<n4>)

### Boundary contract
- B1 is independently reviewable/mergeable: <one sentence + the tests that prove it>
- B2 is independently reviewable/mergeable: <one sentence + the tests that prove it>
- Split axis: capability (not layer) ✅

### GitHub
- B1 is the original PR (kept): #<n1>
- B2 opened with base B1: <url>
- Direct children retargeted onto B2: <C, …>
- Descendants rebased onto their existing parents' new tips (bases unchanged): <D, …>
- Children deliberately left on B1: <none | list>
- Each PR `changedFiles` is its own slice: ✅/❌

### Stack
- Bases verified via `gh pr list` ✅
- Re-registered: `gh stack link --base master <prs…>` ✅

### Invariant
- `git diff orig_B B2` empty: ✅
- B1 builds and its own tests pass (`./test`, `.verify-result.txt`): ✅
- `cargo clippy -- -D warnings` clean on every rewritten branch: ✅

### Backups (FROZEN)
- `backup/<B>-<ts>` @ <sha>
- descendant backups…

### Docs
- B1's `docs/dev/1-WIP/` pair narrowed to the bottom slice, four headings present: ✅
- B2's `docs/dev/1-WIP/` pair written under its own slug, four headings present: ✅
- Children's `## Dependencies` now name B2: ✅
- Each new changeset filed in `docs/dev/changesets/` with a distinct slug: ✅ / none existed

### Titles
- Renumbered `(#<slug> K/N)` across the stack: ✅ (merged PRs skipped)

### Next
- CI: `scripts/ci-status.sh <PR#> --failures` (add `--watch` to block until the run finishes)
- `/green` per PR in its own worktree (create worktrees only for PRs being implemented)
- A force-pushed PR may have lost its approval — re-request review before arming `#automerge`
- Further slices: `/split-pr-to-stack` again on B1 or B2 (this command is two layers only)
```

## Recovery

```bash
git switch backup/<B>-<ts>
# restore B, delete the failed B2 if needed
git branch -D <B2>
```

Do **not** delete B2 while any open PR still bases on it (C may) — deleting a base branch **closes**
its dependent PRs. Retarget those PRs back to B first. This command should not delete B at all.
`CLAUDE.md` says ask before deleting files; the same applies to branches somebody else's session owns.

Also undo the registration: re-run `gh stack link` with the original list, and retarget the children
back onto B.

## Rules

- **Split by capability, never by layer.** A slice that ships only surface is not a valid PR. See
  `docs/ft/coder/pr-stacking.md` § PR boundary contract.
- **Never add a stub or a fallback to make B1 build.** That is the layer split, wearing a disguise.
- **Re-register the stack** after the split — `gh stack link --base master <prs…>`, additive, never
  `--open`. Do not claim the stack was updated when only the bases were.
- **Never close a PR to reshape a stack.** It loses the review history, and a closed PR cannot be
  re-based via the API.
- **Both slices need all four changeset headings** in `docs/dev/1-WIP/`.
- Keep B's existing PR as B1; create B2; retarget **all** direct children of B onto B2; rebase the
  rest of the tree onto their existing parents' new tips.
- **Never delete a branch** an open PR still bases on — it closes that PR.
- **Exactly two layers** per invocation. Fetch remote heads before `rev-parse` / `switch`.
- B2's tree **equals** original B before any descendant rebase.
- Resolve A — never silently use `master`.
- Free trunk, A, B1, B2 and every descendant from other worktrees before rewriting; never rewrite a
  branch whose child session is running.
- Never `--no-verify`, never plain `--force` (use `--force-with-lease`).
- Do not add the `tmp/split-pr-stack-*.md` document to git.

## Related

**Commands**: `/split-branch` (sibling branches, not a stack), `/add-to-pr-stack` (new draft PR on
the tail — do not split), `/follow-up-branch` (branch only on top — do not split),
`/pr-stack-rebase`, `/repoint`, `/merge-pr-stack`, `/squash-pr`, `/update-pr`, `/fix-pr`, `/draft-pr`,
`/merge`, `/pr`, `/pr-wrap`
**Skill**: `pr-stack` (`.agents/skills/pr-stack/SKILL.md`)
**Specs**: `docs/dev/guides/ci.md`, `docs/dev/guides/changelog-merge-hygiene.md`
**Planning a stack from scratch**: `/plan-pr-stack`
