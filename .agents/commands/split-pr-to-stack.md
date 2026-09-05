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
both off `master`, and registers nothing with any orchestrator.

**Use the `pr-stack` workflow recipe**, not this command, when the work is not written yet and you
are planning a stack from requirements. Start a `pr-stack` session (`tddy-coder --recipe pr-stack`,
or the recipe dropdown on the web New-session screen); its `analyze-stack` → `write-stack-plan` →
`write-stack-docs` pipeline plans the whole DAG and writes each node's documents. `plan-pr-stack` is
a **legacy CLI alias of that recipe**, not a slash command — do not write it as one. This command
carves up **existing** commits/files.

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

## Which kind of stack is this? — decide first, it changes Steps 5–8

A branch can be part of a stack in two ways, and this command handles both. Establish which one
applies **before** Step 1 and say so out loud; every later step is annotated for both.

| | **Planned stack** | **Ad-hoc chain** |
|---|---|---|
| What owns the topology | a `pr-stack` **orchestrator session**: the DAG lives in its `Changeset.stack` | nothing — only the PRs' `baseRefName` on GitHub |
| How to detect | the branch's session records `orchestrator_session_id`, or the branch appears as a node in an orchestrator's planned-PR panel | `gh pr list --state open --json number,headRefName,baseRefName` shows a base that is not `master`/`main` |
| Topology tool | the `pr_*` tools, available **to the orchestrator agent** during its `orchestrate` goal | plain `git` + `gh` |
| Inserting a layer | `pr_add_planned` / `pr_adopt`, then `pr_set_parents` | `gh pr create --base <predecessor>` + `gh pr edit --base` |
| Per-PR documents | `artifacts/prs/<node_id>/{PRD.md,changeset.md}` on the orchestrator, four required headings | none required; a `docs/dev/1-WIP/` changeset if the work has one |
| Branch naming | **`feature/<stack-slug>/<node>`** — a validated convention, one namespace per stack | no enforced convention; use the same shape anyway |

**The `pr_*` tools belong to the orchestrator agent, not to you.** A child session working one node
has its `PRD.md` / `changeset.md` attached under `artifacts/attachments/` but does **not** have the
tools. If you are in a child session and the split needs a stack mutation, do the git and `gh` work
here, then hand the orchestrator the exact tool calls to run (Step 6 spells them out). Do not claim
the stack was updated when only GitHub was.

There is **no `gh stack` extension in this repo, and no registered "stack object" to destroy and
rebuild.** In the ad-hoc case the topology *is* the set of `baseRefName`s, so a split is pure git
plus two `gh` calls. In the planned case the orchestrator's DAG is edited in place, node by node.
Either way, nothing here needs an unstack-and-reinit ceremony.

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
  that drops review history, and in a planned stack it orphans the node bound to that branch.
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

Then establish the stack context from the table above:

- **Planned stack** — find the orchestrator. In a session worktree, its id is
  `orchestrator_session_id` in the session's `changeset.yaml`; on the web it is the parent row in the
  session drawer's stack group. Ask the orchestrator agent for `pr_stack_status`, which lists every
  node with its live GitHub state, its effective base, and its internal status. That output is the
  topology source of truth for Step 1c — richer than `gh pr list`, because it also shows **planned**
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
  2. **Planned stack** — the node's effective base from `pr_stack_status` (which climbs `parents`,
     skipping merged ancestors). A merged parent contributes nothing; the effective base may have
     already collapsed to `origin/master`.
  3. **Ad-hoc chain** — the closest open-PR head branch that is an ancestor of B, per the
     `.agents/commands/pr.md` procedure.
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
to match origin. In a planned stack, a node's branch may live only inside a child session's worktree
(`<repo>/.worktrees/<name>`); `git fetch origin <branch>` still works once that session has pushed,
and if it has not, the child owns unpushed work and you must ask before rewriting anything.

### 1c. Walk the descendants

Discover the **direct children of B** first:

```bash
# Ad-hoc chain — direct children of a branch:
gh pr list --state open --base <branch> --json number,headRefName,url
```

**Planned stack** — read the children off `pr_stack_status` instead: every node whose `parents` list
contains B's node. This catches planned nodes that have no PR yet; `gh pr list` cannot see them, and
a planned node that is about to be spawned off B must still be re-parented onto B2.

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
   - **Planned stack**: `feature/<stack-slug>/<node>` using the **same** `<stack-slug>` as every
     other branch in the stack — the shared namespace is validated (`validate_stack_plan`), so a
     one-off name breaks the next plan refinement.
   - **Ad-hoc chain**: no enforced convention; use the same `feature/<slug>/<node>` shape anyway.
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
A/B/descendant branches and PR numbers, the orchestrator session id and node ids if this is a planned
stack, the two slice lists, the retarget decisions, and a checklist of the procedure below.

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
longer composes to `orig_B`). In a planned stack you may also hand the conflict to the orchestrator's
`pr_resolve_conflicts` tool, which syncs the branch, reports the unmerged paths, and marks the node
`has-conflicts` — but only after the tree here is sane.

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

### Ad-hoc chain

There is nothing further to register. The topology **is** the set of `baseRefName`s you set in
Step 5. Verify it and move on:

```bash
gh pr list --state open --json number,headRefName,baseRefName,changedFiles \
  --jq '.[] | "#\(.number) \(.headRefName) → base=\(.baseRefName) files=\(.changedFiles)"'
```

Consider offering to bring the chain into a planned stack later via the orchestrator's `pr_adopt` —
it is the only way these PRs get live status, repoint and the per-PR documents.

### Planned stack

The orchestrator owns the DAG, and **stack order is derived from `parents`** — there is no separate
ordering to rebuild. Inserting B2 between B and its children is therefore two kinds of call, run by
the **orchestrator agent** (you are almost certainly not it — hand it this list):

1. **Bind B2 to a node.** Because B2 already has a branch **and** an open PR (Step 5), the tool is
   `pr_adopt`: it creates a node from the existing PR, bound to its head branch and PR reference.
   Adoption is **refused** if that head branch is already bound to a node, or if any node already
   records that pull number — which is the guard against double-tracking, not a bug to work around.

   `pr_add_planned` is the wrong tool **here**: it appends a *planned* node
   (`branch: None`, `session_id: None`) and never touches an existing one, so it cannot describe a
   branch that already exists. Use `pr_add_planned` only for the other order of operations — when
   you want the node in the plan **before** the branch exists and intend the orchestrator to create
   it (`pr_spawn_child`). If you already created a placeholder planned node for B2 and then adopted
   the real PR, remove the placeholder with `pr_delete_planned` (it refuses a node whose PR is open,
   and it reparents that node's children onto its parents rather than cascading).

2. **Re-parent.** `pr_set_parents` is *the only reorder primitive* — it gives a node a whole new
   parent list:

   | Node | New `parents` |
   |---|---|
   | B1 (B's existing node) | unchanged — still `[A]` (or `[]` if A is the stack bottom) |
   | B2 (the adopted node) | `[<B1's node_id>]` |
   | each direct child of B | its **complete** current list with B's id replaced by B2's — `[B2]`, or `[B2, X]` for a diamond node that also depends on X |
   | D, E, … | unchanged |

   Name the **complete** set every time; an empty list makes the node a root. `pr_set_parents` is
   *not* `pr_repoint`: repoint answers "the base branch drifted, keep the parent"; set-parents
   answers "the plan changed, this node belongs here now". Both share the same git+GitHub tail
   (`realign_node_to_effective_base`: rebase onto the new effective base, `--force-with-lease` push,
   re-target the open PR) — which is why you did the git rebase in Step 4c **first**: the realign
   then finds the branch already on its new base and is a no-op rather than a second rewrite. A
   rejected mutation writes nothing.

3. **Confirm** with `pr_stack_status`: every node lists its expected `parents`, its effective base
   matches the PR's `baseRefName`, and nothing reports `needs-repoint` or `has-conflicts`.

Verify each PR shows **only its own files**:

```bash
gh pr view <N> --json number,isDraft,baseRefName,changedFiles \
  --jq '"#\(.number) draft=\(.isDraft) base=\(.baseRefName) files=\(.changedFiles)"'
```

B1's base is A, B2's base is B1, C's base is B2, D's base is C. A file count on C that includes B2's
work means C was not retargeted. A file count on D that includes C means D was not rebased onto the
new C (or was wrongly retargeted onto B2).

## Step 7: Update the documents

### Planned stack — the per-PR documents are not optional

Every node in the stack needs `artifacts/prs/<node_id>/PRD.md` **and** `changeset.md` on the
orchestrator session, and `changeset.md` must carry **all four** headings — the host checks their
presence structurally and a submit missing any of them is **rejected without writing anything**
(`docs/ft/coder/pr-stack-docs.md` § Validation):

```
## Responsibility        what this PR owns
## Boundaries            what it explicitly does not do
## Dependencies          per parent node: what that PR delivers that this one consumes
## Draft PR contract     what lands first (API + failing tests) to unblock dependents
```

Validation also requires that **every node in the stack has an entry** — a partial pass is refused,
not half-written. So adding B2 to the stack means the next `write-stack-docs` pass must cover B2 too,
or the whole pass fails. Get the documents written in the same sitting as the split.

What to write:

- **B1** — rewrite `Responsibility` and `Boundaries` to the bottom slice only, and add to
  `Boundaries` what has just moved out to B2. Its `Dependencies` (about A) are unchanged.
- **B2** — a new `artifacts/prs/<B2 node_id>/` pair. `Responsibility` = the top slice.
  `Dependencies` names **B1** and states what B1 delivers that B2 consumes — this is the
  duplicate-development guard, and it is the reason the section exists: the child agent building B2
  must know which surfaces are B1's to create. Do **not** back-link to B1's documents (forward-only
  rule).
- **each direct child of B** — its `Dependencies` section now names **B2**, not B, and states what
  B2 delivers. Everything else stays.
- **D, …** — unchanged; their parent did not change.

The documents are authored through the orchestrator's `write-stack-docs` goal
(`tddy-tools submit --goal write-stack-docs --data-stdin`, JSON, `prd` and `changeset` as JSON
strings holding markdown). Note that a **plan refinement re-runs the docs pass** — if the
orchestrator re-emits `stack-plan.yaml` after this split, the state returns to `StackPlanned` and the
next turn regenerates every document before returning to `orchestrate`. That is deliberate; do not
try to route around it.

If the orchestrator agent is not reachable, report the gap explicitly: the PRs are correctly based
but the stack's documents describe the pre-split shape, and the next child spawned off one of these
nodes will read stale boundaries.

### Ad-hoc chain

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

**Planned stack**: prefer `pr_update_planned` on the orchestrator instead — it edits a node's `title`
and `description` (allowed at any time, including once the node owns a branch, a session and an open
PR) and its opt-in `sync_pr` publishes the new title/body to the node's PR, so the plan and GitHub
cannot drift apart. `branch_suggestion` may only be edited while the node owns no branch, so leave it
alone for B1/B2.

**Skip merged PRs.** Their titles are already commits on `master`; editing the PR page only suggests
history was corrected when it was not.

Subjects must state the delivery **as shipped** — never `red`, `stubs`, `WIP`, `phase N`. There is no
commit-message length hook in this repo, but keep titles under ~70 characters as
`.agents/commands/pr.md` asks.

## Output

```markdown
## Split PR → stack complete

### Context
- Stack kind: planned (orchestrator `<session-id>`) | ad-hoc chain

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
- Planned: `pr_adopt` bound B2 → node `<id>`; `pr_set_parents` re-parented <nodes>; `pr_stack_status` clean ✅
- Ad-hoc: bases verified via `gh pr list` ✅ (no orchestrator — offer `pr_adopt` later)

### Invariant
- `git diff orig_B B2` empty: ✅
- B1 builds and its own tests pass (`./test`, `.verify-result.txt`): ✅
- `cargo clippy -- -D warnings` clean on every rewritten branch: ✅

### Backups (FROZEN)
- `backup/<B>-<ts>` @ <sha>
- descendant backups…

### Docs
- `artifacts/prs/<B1>/{PRD.md,changeset.md}` updated, four headings present: ✅
- `artifacts/prs/<B2>/{PRD.md,changeset.md}` written, four headings present: ✅
- Children's `## Dependencies` now name B2: ✅
- (ad-hoc) `docs/dev/1-WIP/` changeset split + indexed in `docs/dev/changesets/`: ✅ / none existed

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

In a planned stack, also undo the stack edits: `pr_set_parents` the children back onto B's node, and
`pr_delete_planned` the adopted B2 node **after** its PR is closed (the tool refuses an open PR).

## Rules

- **Split by capability, never by layer.** A slice that ships only surface is not a valid PR. See
  `docs/ft/coder/pr-stacking.md` § PR boundary contract.
- **Never add a stub or a fallback to make B1 build.** That is the layer split, wearing a disguise.
- **Say which stack kind you are in** at every step that differs — planned vs ad-hoc.
- **The `pr_*` tools belong to the orchestrator agent.** From a child session, hand over the calls;
  do not claim the stack was updated when only GitHub was.
- **`pr_set_parents` is the reorder primitive** — order is derived from `parents`. `pr_adopt` binds
  an existing PR to a node; `pr_add_planned` only appends a node with no branch;
  `pr_delete_planned` reparents children and refuses an open PR.
- **Every node needs its four `changeset.md` headings** — `write-stack-docs` refuses a partial pass.
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
**Specs**: `docs/ft/coder/pr-stacking.md`, `docs/ft/coder/pr-stack-docs.md`,
`docs/ft/coder/pr-stack-live-status.md`, `docs/dev/guides/ci.md`
**Planning a stack from scratch**: the `pr-stack` workflow recipe (`tddy-coder --recipe pr-stack`,
or the recipe dropdown on the web New-session screen)
