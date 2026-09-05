---
name: pr-stack
description: >-
  Principles for planning, tracking, and landing a STACK of dependent PRs — each PR based on another
  PR's branch, not master. Covers the two contexts a stacked branch can live in: a PLANNED stack,
  whose DAG lives in a `pr-stack` orchestrator session's changeset and is driven with the
  `mcp__tddy-tools__pr_*` tools, and an AD-HOC chain, detected from open PR bases with `gh pr list`
  plus `git merge-base --is-ancestor`. Defines the stack as a DAG (a node's `parents` is a list, not
  a single predecessor), the PR boundary contract (every node is independently reviewable and
  independently mergeable — splitting by layer is forbidden, a stubs-only PR is not a valid node),
  the per-PR documents on the orchestrator (`artifacts/prs/<node_id>/{PRD.md,changeset.md}`) with
  their four structurally-validated headings `## Responsibility`, `## Boundaries`,
  `## Dependencies` (do not implement a parent's surface here) and `## Draft PR contract`, the
  shared stack-level `artifacts/pr-stack-plan.md`, base tracking and effective-base derivation,
  forward-only doc linking, the golden rules for bottom-up landing (a deleted base branch CLOSES its
  dependent PR — which is what `pr_repoint` exists to prevent), the worktree rules (plan in the
  current worktree, one branch checked out at a time, restore the original branch when done, delete
  a temporary worktree as soon as its branch is pushed), per-worktree rebase versus a stack-wide
  cascade, the in-stack implementation loop (`/green` rebases onto its base and checks its
  dependencies first, commits and pushes; `/validate-changes` and `/pr-wrap` rebase before any code
  diff), milestone pushes, the not-on-its-base's-latest-tip trap, recurring shared-file conflicts,
  approval dismissal, the `#automerge` / `#forcemerge` comment gate, CI reading with
  `scripts/ci-status.sh`, and recovery. Load this whenever working a stack. The
  `/add-to-pr-stack`, `/split-pr-to-stack`, `/pr-stack-rebase`, `/merge-pr-stack`, `/green`,
  `/validate-changes`, `/pr-wrap`, `/merge`, `/repoint`, `/squash-pr`, `/fix-pr`,
  `/follow-up-branch` and `/pr` commands are the operational entrypoints; this skill is the shared
  model they all follow. Triggers: stacked PRs, PR stack, pr-stack orchestrator, planned PR,
  "PR based on another branch", add a PR to a stack, ad-hoc stacked PR, split a PR into a stack,
  A→B→C split B, base branch deleted / PR auto-closed, retarget a PR base, repoint, follow-up branch
  off an open PR.
allowed-tools:
  - Bash(git *)
  - Bash(gh *)
  - Bash(scripts/ci-status.sh *)
  - Read
  - Grep
  - Glob
  - Edit
  - Write
---

# pr-stack — the stacked-PR model

Canonical mechanics for planning, tracking, and landing a **stack** of dependent PRs. The commands
`/add-to-pr-stack`, `/split-pr-to-stack`, `/pr-stack-rebase`, `/merge-pr-stack`, `/green`,
`/validate-changes`, `/pr-wrap`, `/merge`, `/repoint`, `/squash-pr`, `/fix-pr`, `/follow-up-branch`
and `/pr` are the operational entrypoints; they all follow the definitions below, so follow them
exactly and keep the commands and this skill in agreement.

Planning a stack is **not** a slash command in this repo. A stack is planned by the **`pr-stack`
workflow recipe** — start a session on it (`tddy-coder --recipe pr-stack …`, or pick `pr-stack` in
the web New-session screen) and its `analyze-stack` → `write-stack-plan` → `write-stack-docs` pass
produces the plan and the per-PR documents. `plan-pr-stack` and `orchestrate-pr-stack` are **legacy
CLI aliases of that same recipe**, not slash commands; never write them with a leading `/`. The
authority is [`docs/ft/coder/pr-stacking.md`](../../../docs/ft/coder/pr-stacking.md).

## Two stack contexts — say which one you are in

A branch can be part of a stack in **two** ways. Almost every mistake in this file comes from
running one context's move in the other, so establish which one applies before touching anything.

| | **Planned stack** | **Ad-hoc chain** |
|---|---|---|
| Where the topology lives | `Changeset.stack` on a `pr-stack` **orchestrator session** — a DAG of `StackNode`s | nowhere; it is implied by each PR's `baseRefName` |
| Who owns it | the orchestrator agent, in its `orchestrate` free-prompting loop | whoever opened the PRs |
| How you read it | the `mcp__tddy-tools__pr_*` tools, or the Planned PRs panel on the PR-Stack chat screen | `gh pr list` + `git merge-base --is-ancestor` |
| How a node's base is decided | derived on demand from `parents`, skipping merged ancestors (`Stack::effective_base_refs`) | whatever the PR's base ref happens to say |
| Branch naming | **required**: `feature/<stack-slug>/<node>`, validated by `validate_stack_plan` | unconstrained — use the same shape anyway |
| Per-PR docs | `artifacts/prs/<node_id>/{PRD.md,changeset.md}` on the orchestrator, attached to each child | none unless you write them |

**A child session working one node is not the orchestrator.** It does **not** have the `pr_*` tools.
It has its attached documents (`artifacts/attachments/PRD.md`, `changeset.md`, `pr-stack-plan.md`,
`exploration.md`), plain `git`, and `gh`. When a child needs a stack-level act — a merge, a repoint,
a new node — it reports upward and the operator drives the orchestrator; it does not improvise one.

### Tooling for the orchestrator — the `pr_*` tools

Available only during the `pr-stack` recipe's `orchestrate` goal, auto-allowed without a permission
prompt, and each one operating on the orchestrator session's changeset plus live git + GitHub. Full
table in [`pr-stacking.md` § PR-management tools](../../../docs/ft/coder/pr-stacking.md#pr-management-tools);
the ones this skill leans on:

| Tool | What it does |
|---|---|
| `pr_stack_status` | Every node with its live GitHub state and its computed **internal status** (`up-to-date`, `needs-repoint`, `has-conflicts`, `ready-to-merge`, `blocked`, `merged`); writes the derived statuses back. **This is the cheapest correctness check on a planned stack — read it before and after every stack operation.** |
| `pr_merge` | Merge a node's PR **into its base**. Journalled (`StackOpJournal`) so a crash mid-merge resumes rather than re-merges. |
| `pr_repoint` | Retarget a node onto its recomputed effective base after an ancestor merged: rebase `--onto`, `--force-with-lease` push, `PATCH` the open PR's base. |
| `pr_resolve_conflicts` | Sync a node's branch with its base, detect unmerged paths, mark the node `has-conflicts`, and return the conflicted paths for the agent to resolve **in that node's worktree**. |
| `pr_add_planned` | Append a planned node. Additive only — it never edits an existing node. |
| `pr_update_planned` | Edit a node's `title` / `description` (any time) and its `branch_suggestion` (only while it owns no branch). `sync_pr` also publishes the new title/body to the node's PR. |
| `pr_delete_planned` | Remove a node, reparenting its children onto that node's parents. **Refuses a node whose PR is open.** Branch, worktree and child session are left alone. |
| `pr_set_parents` | Give a node a whole new parent list — the plan-level move, and the **only** reorder primitive, since stack order is derived from `parents`. |
| `pr_adopt` | Create a node from an **existing** PR, bound to its head branch and PR reference. This is how an ad-hoc chain is brought into a planned stack. |
| `pr_spawn_child` | Start a child coding session for a node — the same effect as the web row's "Start session" CTA. |
| `pr_read` / `pr_search` / `pr_comments` | One PR in full; find PRs in this repo (including untracked ones); a PR's reviews and comment threads. |
| `pr_set_status` | Agent override of a node's internal status (`source = "override"`), which derivation will then not overwrite. |

Two contract limits worth remembering: a `pr_search` hit carries **no head or base branch** (GitHub's
search API does not report them — follow up with `pr_read`), and `pr_comments` reports **no thread as
resolved** (resolution is GraphQL-only).

### Tooling for a branch worked by hand

No extension, no plugin — plain `gh` and `git`. Detect the chain exactly the way
[`.agents/commands/pr.md`](../../commands/pr.md) § 2 already does:

```bash
# 1. the trunk
git symbolic-ref --short refs/remotes/origin/HEAD    # fallback: master, then main

# 2. does this branch already have a PR? then its base is already decided
gh pr list --state open --head "$(git branch --show-current)" --json number,baseRefName

# 3. otherwise, find the stack ancestors among the open PRs
gh pr list --state open --json number,headRefName,baseRefName
#    for each headRefName that is not the current branch:
git fetch origin <headRefName>
git merge-base --is-ancestor "origin/<headRefName>" HEAD && echo "ancestor: <headRefName>"
#    the stack parent is the CLOSEST ancestor — the smallest:
git rev-list --count "origin/<headRefName>..HEAD"
```

**Any base that is not `master` / `main` means you are in a stack**, whether or not anyone planned
one. Treat it as such: bottom-up landing, base-branch lifetime, repointing after a merge — all of it
applies. If several candidate parents tie, or the detected base is not the trunk, **confirm with the
user** before acting; basing a PR on the wrong branch misroutes the whole chain.

An ad-hoc chain can be promoted later: `pr_adopt` on an orchestrator binds each existing PR to a
node. Adoption is refused when the PR's head branch is already bound to a node, or when any node
already records that pull number — a PR is tracked once or not at all.

## What a stack is

A stack is a set of PRs where each PR is branched on top of **one or more other PRs**, not on
`master`:

```
origin/master
   └── feature/auth/token-store        (base: origin/master)          ← n1
        ├── feature/auth/middleware    (base: feature/auth/token-store) ← n2
        └── feature/auth/session-api   (base: feature/auth/token-store) ← n3
             └── feature/auth/web-login  (parents: [n2, n3])          ← n4
```

- A node's **parents** is a **list**. Empty = root (branches off the stack bottom). More than one
  entry = a genuine DAG node integrating several unmerged parents.
- A node's **effective base** is derived on demand and never stored: climb `parents`, skip every
  ancestor whose PR is merged, and take the nearest surviving ancestor's branch as `origin/<branch>`.
  When every ancestor is merged the base collapses to the stack bottom (`origin/master`).
- PRs **land bottom-up**: a node may merge only once every one of its parents has merged (which is
  the same as saying: only once its effective base has collapsed to `origin/master`).
- A non-merged parent that owns **no branch** contributes nothing and blocks its descendants — there
  is no synthesized `origin/<node_id>` ref, because nothing ever created one.

### It is a DAG, not only a chain

Do not assume `PRₖ₋₁`. Assume a **parent list**, and write commands that iterate it. The places this
bites:

- **Diamond nodes.** A node with several unmerged parents still has one GitHub base ref, so the PR's
  base points at the **first** non-skipped parent (the primary spine) and the other parents' commits
  arrive through a local `stack-int/<node_id>` integration ref. Its PR is only offered for merge once
  **all** its parents are merged.
- **Choosing a base at spawn.** The Start-session dialog on a multi-parent row renders a **Base
  branch** selector, ordered by the dependency's own **depth** in the DAG (deepest first — a parent
  that itself depends on another parent is the more complete base), then the stack's other
  materialized branches, then the project default branch. See
  [`pr-stacking.md` § Operator-selectable base branch for a diamond node](../../../docs/ft/coder/pr-stacking.md#operator-selectable-base-branch-for-a-diamond-node-added-2026-07-27).
- **Siblings are legal.** Two open PRs sharing one base are **not** a defect here — they are two
  children of the same node. What is a defect is treating them as a linear order and merging one
  "before" the other on that basis alone.

### Branch naming

A planned stack's `branch_suggestion` is **required** and validated: `feature/<stack-slug>/<node>`,
with every node in the stack sharing one `feature/<stack-slug>/` namespace — e.g.
`feature/auth/token-store`, `feature/auth/middleware`. That shared namespace is what makes the
stack's branches group together in every branch list. `validate_stack_plan` rejects a plan that
breaks it, so a plan submit that invents `pr-2-auth` fails outright.

An ad-hoc chain has no enforced convention. Use the same shape anyway.

### The plan is the stack object; the base refs are what GitHub sees

Setting each PR's base to its parent buys one concrete thing: GitHub computes each PR's diff against
its base, so **reviewers see only that PR's own delta**. That alone is most of the review benefit and
is why stacking beats one giant branch.

What GitHub does **not** do, in this repo, ever:

- **Group or order the PRs.** There is no GitHub-side stack object. The ordering lives in
  `Changeset.stack` on the orchestrator, or nowhere at all.
- **Enforce merge order.** Nothing stops merging a node before its parent.
- **Restack anything after a merge.** A dependent left pointing at a merged parent's branch is a
  dead end that somebody has to repair — `pr_repoint` / `/repoint` is that repair, and it is
  **manual by design**. Do not wait for GitHub to do it.
- **Make a successor's CI meaningful for `master`.** Checks build the head merged into **its own
  base**, so green CI on a stacked PR says nothing about `master` until that PR is repointed onto it.

### Adding a layer — `/add-to-pr-stack`

`/add-to-pr-stack` adds a new node **on top of** a named parent (or the stack's tip) so new work can
start without waiting for its parents to merge.

- **Planned stack**: `pr_add_planned` appends the node (`branch: None`, `session_id: None`,
  `pr_status: None` — planned only, nothing spawns), then `pr_spawn_child` starts its session, whose
  worktree is cut off `origin/<parent branch>` through `Stack::base_ref_for_spawn`. The web
  equivalent is the "+ New planned PR" form and its **Add & start session** submit.
- **Ad-hoc chain**: cut the branch off the parent's branch, commit, push, and open the PR with
  `gh pr create --base <parent-branch>`. `/pr` does the base detection for you.

A node may be added **anywhere** in a planned stack, not only at the tip: `parents` is
just a list, and `pr_set_parents` can reshape it afterwards. What you must not do is append a node
and leave its `parents` empty when it actually depends on somebody — an empty list means *root*, and
its child worktree will be cut off `master`.

`/follow-up-branch` is the branch-only counterpart: it creates and switches to a branch off a named
parent, never opens a PR, and never silently defaults the base to the current branch or `master`.

### Splitting a layer — `/split-pr-to-stack`

The `pr-stack` recipe plans a stack from requirements. `/add-to-pr-stack` (planned node, optionally
spawned) and `/follow-up-branch` (branch only) add a node on top of an existing one. `/split-branch`
produces **sibling** branches, typically both off `master`. None of those carve an **existing** PR
into stacked slices.

**Splitting B in `A → B → C` produces `A → B1 → B2 → C`:**

- **B1** is B's existing PR when B already has one (same number, same head branch, same parent A).
  Its diff shrinks to the bottom slice. Do not close B and open two new PRs.
- **B2** is a new PR whose base is B1. After the git split, **B2's tree equals original B**, so C
  rebases onto it without picking up a second copy of B1.
- **C** — and every other child of B — is reparented onto **B2**. On a planned stack that is
  `pr_set_parents` on each child (the plan-level move), which then realigns any child that already
  owns a branch exactly as a repoint does: rebase onto the new effective base, `--force-with-lease`
  push, re-target the open PR. On an ad-hoc chain it is `/repoint` per child.
- Descendants deeper than C keep their parents and are rebased onto their parent's new tip.

Each invocation produces **exactly two nodes** (B1, B2). A third slice is another
`/split-pr-to-stack` on B1 or B2.

If B has **no PR yet**, open a two-node stack: B1 based on the resolved parent A (never silently
`master`), B2 based on B1.

**Both slices must still satisfy the boundary contract** (next section). "Split B" is not permission
to cut B into an interface half and an implementation half — if the only seam you can find is a
layer seam, B is one PR and the answer is a smaller B, not two.

**A split is a plan change, and the plan refuses to be re-planned once work is real.**
`reseed_stack_from_plan_if_unspawned` rejects a whole-plan rewrite the moment any node owns a
`branch` or a `session_id`, precisely because overwriting such a node would orphan a real branch and
an in-progress session. So a split of a live stack is done with the surgical tools
(`pr_add_planned`, `pr_set_parents`, `pr_update_planned`), never by asking the orchestrator to
re-emit `stack-plan.yaml`.

## The PR boundary contract — every node is self-contained

This is the rule the whole model rests on, and it is stricter than "each PR is small". The authority
is [`pr-stacking.md` § PR boundary contract: every node is
self-contained](../../../docs/ft/coder/pr-stacking.md#pr-boundary-contract-every-node-is-self-contained).

> A planned PR must be **independently reviewable and independently mergeable**: the API/schema
> change, the code implementing it, and its tests land in **one** node.

**Splitting by layer is forbidden.** These pairs are one node, never two:

| ✗ Layer split (invalid) | ✓ One self-contained node |
|---|---|
| `n1` add proto RPCs → `n2` implement them | `n1` attachment staging: proto + daemon handler + tests |
| `n1` add an endpoint → `n2` add its handler | `n1` the endpoint, serving real responses |
| `n1` add a data model → `n2` persist it | `n1` the model with its persistence |
| `n1` change a signature → `n2` fill in the body | `n1` the working function |

**A node that ships only surface is not a valid PR** — RPCs returning `unimplemented`, a field
nothing reads, a trait with stub impls. It cannot be reviewed for correctness, because there is no
behaviour to check; it cannot be tested beyond compiling; and it leaves a contract in the tree that
misrepresents what the system does.

**When a vertical slice is too large, split by capability, not by layer.** Cut along user-visible
increments where each part is still end-to-end: one source variant rather than all of them, one
enum case or scope rather than the full set, one screen or entry point, the happy path before the
edge cases. Each such PR carries its own contract *plus* behaviour *plus* tests, and the next node
extends it.

**Two narrow exceptions**, and the agent is explicitly told not to invent a third: a purely
mechanical rename / move / extraction with no behaviour change, or a regeneration of
already-committed generated code exposing no new surface. Anything else that looks like it needs an
exception goes in the node's `description` for a human to decide.

**The contract is advisory, not machine-enforced.** It is carried in the `analyze-stack` **and**
`write-stack-plan` system prompts — in both deliberately, because `write-stack-plan` is what
re-runs on every chat-driven refinement, and a rule present only in `analyze-stack` would be
silently dropped the first time an operator refined the plan. `validate_stack_plan` sees only
`node_id`, `title`, `description`, `branch_suggestion` and `parents` — never the diff — so it cannot
tell a vertical slice from a layer split, and any check would reduce to a keyword heuristic over
`description`. Which means: **it is on you to hold the line**, in planning and in review.

### What "independently mergeable" therefore means here

A node's PR can merge with green CI without waiting on anything above it. Achieve it the boring way:

- **The node owns the whole vertical slice** it delivers — schema, code, tests.
- **Its tests exercise only what it owns.** They never assert behaviour a *descendant* will
  implement.
- **It does not implement what a parent owns.** Not even when the seam is in front of you and the
  fix looks like one line — see `## Dependencies` below.

There is no second "independently greenable" property to trade against, because there are no
stubs-as-a-deliverable to be blocked on: a parent either shipped a capability or it did not. If your
node genuinely cannot be finished until a parent's capability exists, that is a **sequencing fact**,
not a licence to build the parent's half. State it in the node's `description` and in the child's
`## Dependencies`, and schedule accordingly.

### `## Dependencies` — do not implement here

Every non-root node's `changeset.md` carries this section, so whoever runs `/green` learns the
boundary from the document they are already reading rather than from anywhere else. It is the
**duplicate-development guard**: the child agent implementing `n3` has never seen the plan's
reasoning, does not know that `n2` is already adding the trait it is about to add, and has no other
way to find out. Two children building the same abstraction in parallel is exactly the failure this
section exists to prevent, and it is not a planning failure — the plan can be perfect and the
duplication still happens.

State, **per parent node**, what that PR delivers that this one consumes:

```markdown
## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` token-store | `TokenStore::{put,get}` in `packages/tddy-github/src/token_store.rs`, persisted | middleware calls `get` on every request | add persistence, change the trait, or widen the key type |
| `n2` session-api | `SessionService::resume` RPC serving real sessions | the web client calls it | touch the proto or the daemon handler |
```

**Never implement a symbol another node owns.** Two PRs implementing the same symbol is a guaranteed
conflict, and the owning PR's tests are the ones specifying it. If a parent's surface is wrong for
you, say so upward and let the owner change it — a signature change is theirs to push (see
**Milestone pushes**).

### `## Draft PR contract` — early publication, not a stubs PR

A stacked PR blocks its dependents for as long as it is unfinished. The mitigation is to publish the
**interface early inside the PR that will still ship its own implementation**: the API surface plus
its failing tests, enough to open a **draft PR** against, so dependents can branch off a real ref and
compile against a real signature while the implementation continues in the same PR.

Read that sentence twice, because it is the one place this model is easy to misread:

| ✓ Draft PR contract | ✗ What it is not |
|---|---|
| The first push of a PR that will go on to implement the thing | A PR whose deliverable is stubs, with the implementation deferred to a later PR |
| Failing tests that this PR will make pass | Tests a *descendant* is expected to make pass |
| A ref for dependents to branch off, days before merge | A merge candidate |
| Reviewed and merged as one self-contained node | Merged half-done |

**A draft PR must never merge in that state.** The boundary contract governs what lands; the draft
contract governs only what is *visible early*.

Opening a PR as a draft is a **human act** — `gh pr create --draft`. `GithubPrApi::create_pr` has no
`draft` parameter and gains none; the document says what should be in it. Draft PRs are *read*
correctly everywhere: they map to `PrState::Draft`, and `pr_status.phase` deliberately records a
draft as `open`, so a draft node is live in the stack, not planned.

Details: [`pr-stack-docs.md` § Draft-PR contract](../../../docs/ft/coder/pr-stack-docs.md#draft-pr-contract).

## Per-PR documents — where stack context lives

Every planned PR has **two authored documents**, written by the orchestrator during
`write-stack-docs` and **attached to the child session** at spawn:

```
{orchestrator_session_dir}/artifacts/
  stack-plan.yaml         the machine-readable plan
  pr-stack-plan.md        the shared stack-level document, derived from the plan
  exploration.md          the code-discovery map
  stack-status.md
  stack-status.json
  prs/
    n1/
      PRD.md              what this PR delivers — behaviour, API surface, acceptance criteria
      changeset.md        how, and where the edges are
    n2/
      PRD.md
      changeset.md
```

Paths are **derived from `node_id` by convention**, not recorded on the node — `StackNode` gains no
field for them, and neither do `PlannedPr`, `AddPlannedPrRequest` or the web wire type.

### The four required headings

`changeset.md` is validated **structurally** on submit: the host asserts the headings are present,
never that their contents are correct. All four are required, in every node's document:

```
## Responsibility        what this PR owns
## Boundaries            what it explicitly does not do
## Dependencies          per parent node: what that PR delivers that this one consumes
## Draft PR contract     what lands first (API + failing tests) to unblock dependents
```

A submit missing any of them is rejected **and writes nothing** — as is a submit whose `prd` or
`changeset` is blank, one naming a `node_id` that is not in the stack, or one that skips a node.
A partial pass is refused rather than half-written, because a node with no boundaries document is
precisely the duplicate-development hazard the documents exist to close.

This line — presence enforced, content not — is deliberate. "Does this document have a
`## Boundaries` heading" needs no semantics at all; "is this a vertical slice" needs the diff the
node has yet to produce.

### What the child sees

At spawn the child receives four attachments, materialized into its `artifacts/attachments/`:

| Source on the orchestrator | Destination basename |
|---|---|
| `prs/<node_id>/PRD.md` | `PRD.md` |
| `prs/<node_id>/changeset.md` | `changeset.md` |
| `pr-stack-plan.md` | `pr-stack-plan.md` |
| `exploration.md` | `exploration.md` |

The child's `initial_prompt` names the attached changeset by path, so the agent reads its boundaries
before writing code rather than finding the file by chance. A missing document is **skipped, not
fatal** — a node started before the docs pass ran attaches whatever exists, and the operator is told
what was unavailable.

The child's own `artifacts/PRD.md` (the `tdd` recipe's manifest artifact) is a **different file**
from the attached `artifacts/attachments/PRD.md`. Do not conflate them, and do not "tidy" one into
the other.

### There is a shared stack-level document — and only one

`artifacts/pr-stack-plan.md` **is** the stack-level document, and it is generated from
`stack-plan.yaml` on every plan write. It is not hand-authored and not supplemented by a second epic
document, because a hand-authored twin could only drift.

What there is **no** room for is a shared *per-PR status* manifest — a committed `STACK-*.md` in
`docs/dev/1-WIP/` with a row per PR. Three separate reasons rule it out:

- It would have to **outlive every individual PR**, so it could not be wrapped: no State B, and a
  lifetime spanning the whole stack. It would have to be handed from each wrapping PR up to the next
  open one — a cross-branch push during what should be a self-contained wrap.
- Every PR would edit the same file to update its own row, so **every rebase produces a union
  conflict** on it, and each wrap a modify/delete conflict on top.
- Its status column would be a hand-maintained copy of what `pr_stack_status` and `gh pr list`
  already know, going stale between the moment a PR landed and the moment somebody remembered.

`pr-stack-plan.md` avoids all three by being **derived** and living on the **orchestrator session**,
not in any PR's diff. Nothing in a PR's own tree describes the stack's status, so no two PRs ever
edit the same planning file.

**Rules that follow:**

- **No `docs/dev/1-WIP/STACK-*.md`.** If you find one, fold each row into the matching node's
  `changeset.md` on the orchestrator and delete it (ask before deleting, per CLAUDE.md).
- **A node's document states its own row and nothing else.** Never record a sibling's status — that
  is the stale-copy problem returning.
- **Nothing hands off at wrap.** `/wrap-context-docs` transfers this PR's own changeset and PRD and
  deletes them. There is no third document and no "action required" notice.
- **When `pr_stack_status` disagrees with a document, live status is right.** Fix the document —
  with `pr_update_planned` for title/description, or a `write-stack-docs` re-run for the boundaries.

### Where the manifest's global views come from now

| The manifest used to hold | Where to get it now |
|---|---|
| stack order, branches, bases | `pr_stack_status`; or `gh pr view <N> --json baseRefName` per PR |
| which PRs have landed | `pr_stack_status` (`pr_status.phase`), `gh pr list --state merged` |
| what each PR owns | that node's `changeset.md` → `## Responsibility` / `## Boundaries` |
| what each PR depends on | that node's `changeset.md` → `## Dependencies` |
| whether a node needs action | `internal_status` — `needs-repoint`, `has-conflicts`, `ready-to-merge`, `blocked` — derived on every `pr_stack_status`, overridable with `pr_set_status` |
| the whole-stack shape and rationale | `artifacts/pr-stack-plan.md` on the orchestrator |
| why the work was split this way | each node's own document states its seam; the durable whole-stack rationale lands in the package docs when the bottom PR wraps |

## The in-stack implementation loop — `/green` → `/validate-changes` → `/pr-wrap`

Implementing a PR **inside a stack** is not the same as implementing a standalone branch, because
other people (and other worktrees) are building on top of what you push. Extra obligations apply to
every stack branch, on top of what
[`.agents/commands/green.md`](../../commands/green.md) already does.

0. **Rebase, then check dependencies, before writing code.** `/green` on a stack branch **always**
   runs `/pr-stack-rebase` (this branch only) first, so the branch is in sync with the latest pushed
   state of its base. Then it **MUST** read the node's `## Dependencies` and confirm that every
   capability this PR consumes actually exists at `HEAD`. If a required parent capability is not
   there, **stop and ask the user** how to proceed — do not implement the parent-owned surface, and
   do not green against a lie. (Under the boundary contract this gate fails rarely: a parent either
   shipped its slice or has not merged its work yet. When it does fail, it is a real sequencing
   problem and deserves a human decision.)
1. **Commit and push at the end of `/green`.** A stack branch's descendants are cut from it, and
   reviewers read it as part of a chain. Work that exists only in one worktree is invisible to both.
   So when the green phase of a **stacked** PR finishes, `/green` commits and pushes that branch — it
   does not stop at "tests pass locally". (For a standalone branch `/green` still leaves committing
   to the user; this obligation is stack-specific.)
2. **Validate implementation against the plan before wrapping.** Each node was planned with its own
   `PRD.md` + `changeset.md` + responsibility + boundaries + dependencies. `/validate-changes` is the
   checkpoint that the branch matches *that* plan — and, just as important, that it has not quietly
   grown into a **parent's or a child's** territory.
3. **Rebase before any code diff.** `/validate-changes` and `/pr-wrap` **always** run
   `/pr-stack-rebase` (this branch only) before they execute a code diff. A clean-looking status is
   not a skip: leaked ancestor commits can still sit in `HEAD` relative to a stale merge-base, and
   treating those as this PR's work — or deleting the "extra" files to shrink the diff — is how
   parent code gets destroyed. If the branch already contains the latest base tip and
   `origin/<base>..HEAD` is this PR's commits only, the rebase is verify-and-return (no rewrite).
   **No `git diff` until that leak check passes.** Unplanned deletions of parent-owned files are
   defects, same as implementing a parent-owned symbol.

The loop each stack worktree runs:

```
/green            → ALWAYS /pr-stack-rebase first; dependency gate (STOP and ask if a
                    capability this PR consumes does not exist yet); then implement,
                    commit, push →
/validate-changes → ALWAYS /pr-stack-rebase first; then implementation vs plan;
                    commit + push the updated status; then either
                      gaps remain  → back to /green (finish them), re-run /validate-changes
                      no gaps      → /pr-wrap
/pr-wrap          → ALWAYS /pr-stack-rebase first (again); then validate/refactor cycle,
                    docs, correct the PR title, mark the PR ready
```

`/validate-changes` on a stack branch checks, beyond its normal analysis:

- **Every changeset item and PRD acceptance criterion for THIS node** is implemented, or explicitly
  deferred with a reason.
- **`## Responsibility` is fully delivered** — no `TODO` / `FIXME` stub bodies left where this node's
  own surface should be. A node that ships only surface is not a valid PR, so an unimplemented owned
  symbol is a blocker, not a follow-up.
- **`## Boundaries` held** — nothing here does what the document says this PR explicitly does not do.
- **No dependency was implemented here.** Cross-check `## Dependencies`: each listed surface must
  still be the parent's, untouched in this branch's diff. Implementing one is a defect, not a bonus.
- **No descendant behaviour crept in**, and no test asserts something a later node will implement.
- **The diff contains only this node's files** (`gh pr view <N> --json changedFiles`), against the
  right base, **after** `/pr-stack-rebase` has made `origin/<base>..HEAD` this PR's commits only. Do
  not execute that diff until the rebase/leak gate passes.
- **No unplanned deletions.** Parent-owned files still exist at `HEAD`. Extra files in the diff are
  leaked ancestor work — rebase, never `git rm` them to "clean" the PR.

Because the branch is pushed, the state a reviewer or a child sees is always the state
`/validate-changes` reported on — which is why it commits and pushes before routing.

Local gates before any push, all of them cheap next to a CI round trip:

```bash
cargo build                      # the branch compiles — descendants inherit this tree
cargo fmt                        # `Rust lint` runs `cargo fmt --all --check`
cargo clippy -- -D warnings      # `Rust lint` runs this workspace-wide with -D warnings
./test -p <package>              # this node's own tests
```

Never `--no-verify`, on a commit or a push. If a hook blocks you, fix what it found.

### Push at every milestone — descendants are waiting on your commits

The end of `/green` is the **latest** a stack branch may be pushed, not the cadence. A child inherits
its parents' branch, so **behaviour that is not pushed does not exist for the PRs above you**. Hold a
day's work locally and every descendant keeps building against something that could already have been
real.

**A milestone is production code that already passes its tests** — a finished slice of this node's
own responsibility, not a checkpoint of work in progress. That is the whole point: a child inherits
whatever you push, so what you hand them must be behaviour they can build on and trust, verified by
the tests that specify it.

**Commit and push each such milestone as it lands**, rather than batching:

| Milestone | Why a descendant cares |
|---|---|
| A capability named in a child's `## Dependencies` goes real, **its tests green** | the child blocked on it can start against real behaviour instead of waiting |
| A **signature** of an owned symbol changes | descendants compile against it — a late signature change breaks them long after the decision, in someone else's worktree |
| A test group goes green | the behaviour it pins is now safe to depend on |
| A proto / schema field lands | wire-level consumers can generate against it |
| A wiring task completes and is verified | integration points become real |

A signature change is the urgent one. Push it on its own, immediately, ahead of the implementation
behind it — the compile-level contract is what the whole stack is built on.

**Two preconditions, both hard:**

1. **The slice's own tests pass.** Run them (`./test -p <package>`, or `./test -- <test_name>`); do
   not push production code you have not seen go green. Other tests in this PR may still be red — the
   ones covering parts you have not implemented yet — but never the ones covering what you just
   pushed.
2. **The branch builds.** `cargo build` before every push, and `cargo clippy -- -D warnings` before
   you claim it is clean. Descendants inherit the tree; a broken compile stops every worktree above
   you, and a clippy error stops CI for all of them.

**Never push half-finished implementation to "share progress."** Scaffolding, commented-out branches,
a function that works for one case, code whose tests you have not run — all of it is worse for a
child than nothing at all. Half-working behaviour silently misleads every PR above you and shows up
as *their* bug. If a slice is not finished and green, leave it out and push when it is.

**Tell the people above you.** When a milestone unblocks a node whose document says it is blocked,
say so in the push report — and on a planned stack, ask the operator to clear the node's
`pr_set_status` override if one was set. That node's own document is theirs to update, not yours to
edit from a sibling branch. The blocked worktree then runs `/pr-stack-rebase` to pick the behaviour
up; without the message it waits on something that is no longer missing.

**Yes, this leaves your descendants behind their base.** Every commit you push does. That is the
intended trade: frequent small rebases in each worktree (`/pr-stack-rebase`, bottom-up) beat one
enormous one at the end, and it is exactly what the per-worktree rebase path exists to make cheap.

### Planning pushes each node as it is planned

The `pr-stack` recipe's planning phase runs in the **orchestrator's own worktree** and writes only
orchestrator-side artifacts — the plan, the per-PR documents, the exploration map. It never commits
to a child's branch, because at planning time no child branch exists: a planned node has
`branch: None` until its child session creates one.

That removes a whole class of problems a planner that commits to node branches would create: nothing
pins a child branch during planning, no layer is force-pushed under a worktree that is mid-`/green`,
and no planner ever commits to a node it does not own.

The first commit on a node's branch is therefore the **child's own first push**: when a child session
starts, it cuts
`feature/<stack-slug>/<node>` off its effective base, does its own red/green work, and pushes. The
`## Draft PR contract` section is what tells that child what to publish first so its dependents can
start.

## PR titles — the delivered state, the stack slug, and the position

A PR's title is not a label on a work-in-progress branch. On squash merge it **becomes the commit
message on `master`**, permanently, and that is the only form most people will ever read. The repo's
squash defaults are `COMMIT_OR_PR_TITLE` / `COMMIT_MESSAGES`, which is what produces the
`… (#433)` subjects in `git log`.

Note the trap in `COMMIT_OR_PR_TITLE`: for a PR with **exactly one commit** GitHub uses the *commit*
subject, not the PR title. A one-commit PR therefore lands under whatever the commit message said,
silently ignoring the title the stack was careful about. Either keep the commit subject identical to
the PR title, or pass the title explicitly when merging.

### The format

```
<type>(<scope>): <what this PR delivers> (#<stack-slug> K/N)
```

Type and scope stay exactly the repo's existing conventional-commit form — look at `git log
--oneline -25` before writing one. In this repo the scope is the package short name with the
`tddy-` prefix dropped, comma-joined when a change genuinely spans packages, or a feature area when
that reads better:

```
feat(pr-stack): per-PR documents attached to the child session (#420)
fix(coder,daemon,web): a planned PR started on another host stops disappearing from its own stack (#428)
feat(service,daemon): infer a peer agent session's status from its conversation (#419)
fix(web): stop a workflow session re-attaching on every list poll (#426)
ci: merge a PR on a #automerge comment (#408)
```

So a five-node stack whose slug is `attach-docs` reads:

```
feat(core): per-node PR and changeset documents on the orchestrator (#attach-docs 1/5)
feat(daemon): attach a node's documents to its child session at spawn (#attach-docs 2/5)
feat(web): pre-populate the Start-session dialog with the node's documents (#attach-docs 3/5)
fix(tools): reject a docs submit that skips a node (#attach-docs 4/5)
docs(coder): the per-PR documents and their four headings (#attach-docs 5/5)
```

All stack metadata lives in **one trailing group**, so anything that parses type and scope —
changelog tooling, release notes, a human skimming — sees what it has always seen.

**`#<stack-slug>`** identifies which stack a PR belongs to. Several stacks are usually open at once,
all touching the same packages; without it, `feat(daemon): …` on two open PRs says nothing about
whether they are related, sequenced, or independent. The slug is:

- **short** — one or two words, kebab-case, no date and no issue number;
- **associative** — it should mean something to somebody skimming a PR list six weeks later.
  `attach-docs`, `sandbox-split`, `base-sync` are good; `stack-3`, `daemon-refactor`, `phase-two`
  are not;
- **chosen once, at planning**, and **never changed** — it is how the stack is recognised, so
  renaming it mid-flight loses the association it exists to create;
- **the same slug as the branch namespace.** `feature/<stack-slug>/<node>` and `(#<stack-slug> K/N)`
  use one word, so a branch name and a PR title identify the same stack without a lookup.

**`K/N` closes the group**, always last **in the PR title**. It tells a reviewer that #428 is the
fourth of six and cannot merge before three others — which a base-branch name does not say. On a DAG,
`K` is the node's position in the plan's **reading order** (`display_order`, which is what the
Planned PRs panel shows and what move-up/move-down changes), not a claim that the stack is a line;
the real dependency edges are `parents`, and `pr_stack_status` is where you read them.

**On `master` it is not last, and that is expected.** GitHub's squash merge appends ` (#<pr>)` to the
subject, so the commit lands as:

```
feat(daemon): attach a node's documents to its child session at spawn (#attach-docs 2/5) (#431)
```

Two groups saying different things: `(#attach-docs 2/5)` is the stack and the position, `(#431)` is
the pull request GitHub merged. Do not try to pre-empt the second by leaving it out of the first, and
do not treat its presence as the format having been broken.

### The subject states what the PR delivers, in its finished state

Write the title for the reader of `git log` on `master`, who will never know a planning phase
happened. It names the capability as shipped:

| Instead of | Write |
|---|---|
| `…: machinery and red tests (1/6)` | `feat(core): derive a node's effective base from its parents (#attach-docs 1/6)` |
| `…: stubs + red tests — attachment plumbing` | `feat(daemon): materialize a node's documents as child attachments (#attach-docs 2/6)` |
| `pr 4 production wiring` | `fix(web): send the operator's chosen base branch on spawn (#attach-docs 4/6)` |
| `…: docs honesty (3/6)` | `feat(tools): reject a docs submit that skips a node (#attach-docs 3/6)` |

**Never** put a TDD phase or a process artifact in a title: `red`, `green`, `stubs`, `failing tests`,
`WIP`, `phase 1`, `M1c`. Those describe where the work was when the branch was cut, not what the
branch delivers — and under the boundary contract, a PR whose honest title is "stubs" should not be
merging at all. **Never** let a title be a branch slug (`pr 4 production wiring`); that happens when
a tool derives one and nobody replaces it.

The node's `PRD.md` — or the changeset's Summary — is usually the best source: it already says what
this PR delivers, in delivered terms.

### Length is a readability convention here, not a hook

There is **no `commit-msg` hook** in this repo and no enforced character cap; nothing will reject a
long title. That makes it your judgement, so use the same one the existing history uses: `/pr` asks
for a title under ~70 characters, and merged subjects in `git log` run to roughly a hundred at the
outside. Budget for GitHub appending ` (#<pr>)` — about 7–8 more characters — and remember that
GitHub, `git log --oneline`, and every notification e-mail truncate. Put the load-bearing words
first; if the whole thing will not fit comfortably, the body is where the detail belongs.

### Who sets and who fixes a title

| Moment | Command / tool | Obligation |
|---|---|---|
| A node is planned | the `pr-stack` recipe, `pr_add_planned`, the web "New planned PR" form | Set the node `title` to the full format immediately. The subject states the *planned* delivery — write it as shipped. |
| The PR is opened | `/pr`, `gh pr create` | Carry the node's title through verbatim. |
| A node is added, split, or reordered | `/add-to-pr-stack`, `/split-pr-to-stack`, `pr_set_parents`, move up/down | **Renumber every PR in the stack** (see below). |
| Before ready for review | `/pr-wrap` | **Re-read the title and correct it** to what the branch actually delivered. Scope drifts during green; the title written at planning is a guess, and this is the last moment it can be fixed. |

`/pr-wrap`'s check is the load-bearing one. Everything else is a default that a green phase may have
invalidated.

Set a title with plain `gh pr edit` — no `gh api -X PATCH` workaround is needed on
`uppin/tddy-coder`:

```bash
gh pr edit <N> --title "feat(daemon): attach a node's documents at spawn (#attach-docs 2/5)"
```

On a **planned** stack, prefer `pr_update_planned` with `sync_pr: true`: it edits the node's `title`
/ `description` **and** publishes them to the PR in one act, so the plan and GitHub cannot drift.

### Renumbering when the stack changes shape

`K/N` is only true for one stack shape. Adding a node changes `N` for **every** PR; splitting one
changes `K` for everything after it. A stale `2/6` on a seven-node stack is worse than no number,
because it is read as fact.

So the commands that change the shape own the renumber, and must do it in the same run — bottom-up,
subject unchanged, only the trailing group moving:

```bash
# planned stack: keeps node and PR in step
#   pr_update_planned { node_id: "n3", title: "feat(web): … (#attach-docs 3/6)", sync_pr: true }

# ad-hoc chain, or a node whose PR you are fixing directly:
gh pr edit <pr> --title "feat(<scope>): <unchanged subject> (#<slug> <K>/<N>)"
gh pr view <pr> --json number,title --jq '"#\(.number) → \(.title)"'
```

**Do not renumber a merged PR.** Its title is already on `master` and the PR page no longer matters;
editing it creates the false impression that history was corrected. Leave it, and let `N` refer to
the stack as planned.

## Base tracking — source of truth

The authoritative base of a branch that has a PR is its **GitHub PR base** (`baseRefName`):

```bash
gh pr view <branch> --json baseRefName,number,url --jq '.baseRefName'
```

Commands MUST resolve a branch's base in this order:

1. **An explicit argument passed by the user** (e.g. `/merge some-branch`, `/repoint --onto master`).
2. **The planned stack**, when there is one — `pr_stack_status`, whose per-node view carries the
   effective base computed from `parents` with merged ancestors skipped. This is the most
   authoritative source for a planned stack, because it is the only one that knows what the base
   *should* be as opposed to what it currently says. A node whose PR base ≠ its effective base is
   exactly the `needs-repoint` internal status.
3. **The node's own document**, from a child session: `changeset.md` → `## Dependencies` names the
   parents, and the attached `pr-stack-plan.md` shows the shape. Use it to sanity-check, not to
   override live status.
4. **The open PR's base** — `gh pr view <branch> --json baseRefName`.
5. **Ad-hoc detection** — the `gh pr list` + `git merge-base --is-ancestor` recipe above.
6. If none resolve, **ask the user** — never silently fall back to `master`.

> `/merge` brings a branch up to date with its **resolved base**, not with `master`. Only `/repoint`
> (or `pr_repoint`) *changes* a base — and after a parent merges, that change is mandatory, because
> nothing performs it for you.

## Document linking rule (forward-only)

Each PR carries its **own** PRD + changeset in `docs/dev/1-WIP/`. Because a stacked branch inherits
its parents' commits, a child's tree contains its parents' working documents — but the reverse is not
true, and PRs are **wrapped when they are set ready for review**, bottom-up, so a parent's documents
leave `docs/dev/1-WIP/` before its children's.

To avoid dangling links in `1-WIP` as PRs land, references flow **parent → child only**:

- ✅ A parent's PRD/changeset MAY link forward to its children's PRD/changeset (add a
  `## Successor PRs` section listing the next node's documents).
- ❌ A child's PRD/changeset MUST NOT link back to a parent's documents.

Rationale: the parent is wrapped and removed from `1-WIP` first. A backward link from a still-open
child would immediately dangle. A forward link lives only in the document that is itself about to be
wrapped, so nothing left in `1-WIP` ever points at a removed file.

**This is why readying a stack bottom-up matters.** Wrapping is triggered by setting a PR ready for
review (see [`.agents/commands/wrap-context-docs.md`](../../commands/wrap-context-docs.md)), so the
ready order *is* the wrap order. Ready a child before its parent and the child's documents leave
`1-WIP` first, which is exactly the case the forward-only rule cannot survive.

The orchestrator's per-PR documents under `artifacts/prs/<node_id>/` are **not** part of this rule:
they live on the orchestrator session, never in a PR's diff, and are not wrapped at all.

## Golden rules for landing (read before merging anything)

The dependency is encoded in **branches**, so branch lifetime and merge order matter — get them wrong
and GitHub silently **closes** PRs.

1. **Merge bottom-up.** Land the roots first, then their children, and never a node before every one
   of its parents. Nothing in GitHub enforces this. On a planned stack, `pr_stack_status` marks a node
   `ready-to-merge` only when its PR is open, all its dependencies are merged, and it has no
   conflicts — trust that, not the order the PRs happen to be listed in.
2. **A branch must stay intact until no open PR bases on it.** A parent's branch is its children's
   base — deleting it while a child's PR is open **closes that PR**, and a PR whose base branch no
   longer exists **cannot be reopened or re-based via the API**. This is the #1 way stacks break, and
   it is exactly why `pr_repoint` exists: **repoint every dependent first, then delete the branch.**
   Never pass `--delete-branch` to `gh pr merge` while any open PR still bases on that branch, and be
   careful with a repo-level `delete_branch_on_merge` — nothing in this repo restacks a dependent for
   you.
3. **Merging is `pr_merge` on a planned stack, and a normal merge otherwise.** There is no GitHub
   stack object here and no `merge-async` requirement: `pr_merge` goes through
   `GithubPrApi::merge_pr` (plain REST), journalled by `StackOpJournal` so a crash between "merged"
   and "dependents repointed" resumes instead of re-merging. By hand it is the `#automerge` comment
   gate (see **Landing sequence**) or `gh pr merge <N> --squash`. Whichever you use, **one PR at a
   time** — never start the next before the current one reports merged.
4. **After a merge, repoint every dependent — nothing does it for you.** The parent's branch is now
   dead weight (and about to be deleted), so each dependent's PR base must move to its recomputed
   effective base, and its history must move with it: `pr_repoint` (or `/repoint`) does
   `git rebase --onto <new base> <old base> <branch>`, `git push --force-with-lease=<branch>:<sha>`,
   and `PATCH` on the PR base, with a `git merge-base` fallback guarding a stale `<old base>` and
   `git rerere` replaying earlier conflict resolutions. Every step is idempotent, so re-running a
   half-finished repoint is safe.
5. **A repoint that conflicts stops; it does not guess.** `pr_resolve_conflicts` syncs the branch,
   detects unmerged paths, marks the node `has-conflicts` and returns the conflicted files; the
   orchestrate goal runs with `AcceptEdits` so the agent resolves them **in that node's own
   worktree** and re-runs the tool to confirm a clean tree. `--force-with-lease=<branch>:<expected>`
   means a concurrent child push **aborts** the repoint rather than clobbering the child's work — when
   that happens, rebase in the child's worktree instead of retrying the force-push.
6. **Check the diff after every merge.** An inflated `changedFiles` on a dependent is the tell that
   its repoint did not really happen and it is carrying an already-merged parent's files:
   `gh pr view <N> --json changedFiles,baseRefName,mergeable`.
7. **Each PR owns only its own diff.** Don't cherry-pick a parent's commits forward, and never
   implement a symbol another node owns.
8. **Never re-plan a stack whose nodes own real work.** `reseed_stack_from_plan_if_unspawned` refuses
   a whole-plan rewrite once any node owns a `branch` or a `session_id`, because the overwrite would
   orphan a live branch and an in-progress session. Reshape with `pr_add_planned` /
   `pr_set_parents` / `pr_update_planned` / `pr_delete_planned` instead — and note that
   `pr_delete_planned` refuses a node whose PR is open, on purpose.

## Landing sequence

### Planned stack — the default path

**`/merge-pr-stack` automates this.** Reach for it rather than driving the steps by hand; on a planned
stack it drives `pr_stack_status` → `pr_merge` → `pr_repoint` in a loop.

**Sweep the open review comments across the whole stack first**, before any PR merges. Reviewers
comment on whichever PR's diff showed them the code, so a thread raised on PR X often has to be fixed
in a **different** PR — the one that owns that surface, by the same ownership rule that governed
planning. The direction is asymmetric and only one case is recoverable later:

| Comment left on | Fix belongs to | Consequence |
|---|---|---|
| PR X | a **descendant** of X | fine — X merges, the fix rides in the later PR |
| PR X | a **parent** of X | **blocking** — that PR merges first, so the fix must land in it *before* it does |

A merged PR's threads are archived in practice, so a defect raised there and not triaged is lost.
Read threads with `pr_comments` (or `gh pr view <N> --comments`) and validate each comment against
the code **at HEAD**, never against the diff hunk quoted in it — every `/pr-stack-rebase` moves lines
under comments. Signal each verdict as a reaction on the comment itself (👍 actionable, 👎 plus an
evidence reply for not-a-defect, 🚀 fix pushed — the same protocol `/fix-pr` uses). `/merge-pr-stack`
records the triage and its progress in a living plan at `tmp/merge-pr-stack-<slug>.md`, which is
gitignored (`tmp` is in `.gitignore`) so it can never enter a PR's diff — for the same reason a
committed shared manifest was rejected above.

**Then apply the fixes in their own bottom-up pass, before any merge, gated locally only** —
`cargo build`, `cargo clippy -- -D warnings`, `./test -p <package>` per node, push, next node,
**without waiting for CI**. Each push starts that PR's CI in the background, so the merge loop below
consumes runs that are already warm; each node rebases onto its just-pushed parent first, with the
recorded-tip `--onto` mechanics from `/pr-stack-rebase` cascade mode.

Then the merge loop: bottom-up, one PR at a time — never start the next before the current one
reports merged:

1. **`pr_stack_status`** — confirm the order, that this node is `ready-to-merge`, and that its PR
   base is what it should be. Re-sweep this PR's threads (reviewers comment while CI runs) and apply
   any comment fixes the plan still assigns to this PR, including ones raised on another PR's thread.
2. **Wait for CI, and read it properly.**

   ```bash
   scripts/ci-status.sh --watch <PR#>       # block until the run finishes, then report
   scripts/ci-status.sh --failures <PR#>    # failing test names, files, assertions, log tails
   ```

   The four required checks are `Rust lint`, `Rust build`, `Rust tests`, `Web tests`. **Do not treat
   "zero pending checks" as green** — a check is registered on the rollup a moment *after* a push, so
   "no checks pending" is true before CI has even started. That false positive is worst right after a
   force-push, which is exactly when a stack is being repaired. `--watch` exists so you do not have
   to invent your own polling.

   Remember what the gate deliberately does **not** cover: VM-backed tests, cgroups sandbox tests,
   Cypress e2e, `tddy-desktop`. Green CI is not a substitute for `./vm-tests` when you touched that
   area — see [`docs/dev/guides/ci.md`](../../../docs/dev/guides/ci.md).
3. **Merge.** On a planned stack that is `pr_merge` on the node. Driving it by hand instead, the
   repo's merge gate is a **comment**, handled by `.github/workflows/automerge.yml`:

   | Comment | Effect | Reaction |
   |---|---|---|
   | `#automerge` | merge — squashed — as soon as the four required checks pass | 🚀 |
   | `#automerge-cancel` | disarm it again | 👀 |
   | `#forcemerge` | merge **now**, past red or still-running checks | 🎉 |

   ```bash
   gh pr comment <N> --body '#automerge'
   ```

   Only commenters with `write`, `maintain` or `admin` are obeyed; an unauthorised comment gets **no
   reaction at all**, so "nothing happened" can mean "not permitted", not "not seen". A malformed or
   failed request gets 😕.

   > **The trap that is specific to stacks: `#automerge` merges a PR into its own base, which for a
   > stacked PR is its parent's branch, not `master`.** Arming it on a non-bottom node folds that
   > node's work into its parent's PR — sometimes what you want, usually not. Arm `#automerge` on a
   > node **only** once its base is `master` (i.e. it is a root, or it has already been repointed
   > after its parents merged).

   `#forcemerge` is held to the same `write` bar as `#automerge` and leaves a durable trace: before
   merging, the workflow comments naming who asked, linking their comment, and quoting the full check
   state. Use it for a known-irrelevant red check, never to outrun a failure you have not read.
4. **Repoint every dependent** — `pr_repoint` per dependent node, or `/repoint`. Then verify: each
   dependent should now report the new base, be mergeable, and show **only its own file count**
   (`gh pr view <N> --json changedFiles,baseRefName,mergeable`). If it does not, repair it (below).
5. **Only now delete the merged branch**, and only if nothing else bases on it.
6. Repeat from 1 with the next `ready-to-merge` node.

A red check is work, not a stop: pull the failure (`scripts/ci-status.sh --failures <PR#>`), fix it
**in the PR that broke it**, push, wait again. Never `#forcemerge` past a real failure and never
patch it in a descendant.

**`/merge-pr-stack` does not pause for the user while CI runs.** An armed `#automerge` that has not
fired yet, a repoint in progress, a check still queued — those are the command's job; stay in the
loop until the requested PRs report merged. Yield only for a human decision: the first merge's
confirmation, an unresolvable conflict, a product or scope choice, or anything that would delete a
branch or a file.

#### Repairing a failed repoint

```bash
# The dependent may NOT descend from the parent's final tip — its implementer
# may have rebased or amended after it was cut. Find the real fork point: the
# parent commit of the dependent's first OWN commit. Do not assume.
git log --oneline origin/master..origin/<dependent>       # where its own work starts
git log --format='%h parent=%p' -1 <first-own-commit>

git checkout -B <dependent> origin/<dependent>
git rebase --onto origin/master <that-parent> <dependent>
# resolve, then
git push --force-with-lease origin <dependent>
gh pr edit <N> --base master
```

`--onto` is **required**. A plain `git rebase master` replays the parent's commits too, because a
squash merge lands their content under a commit unrelated to theirs — and the result is a dependent
PR whose diff contains its parent's whole delta a second time.

When a conflict is two branches bumping the same literal — a count, an enumeration, a proto field
number — **neither side is right**. Compute the value from the source of truth (and for a proto field,
pick the next genuinely free number), then ask whether an assertion that merely restates its own
computed input is worth keeping at all.

Documents are **not** wrapped here — each PR's changeset and PRD were already wrapped when that PR
was set ready for review, bottom-up. By the time a PR merges its documents have left `1-WIP`.

Between steps, re-read `pr_stack_status`. If it disagrees with a node's document, **live status is
right** and the document needs updating.

### Ad-hoc chain — manual

No orchestrator, so no `pr_merge` and no `pr_repoint`; you are the loop. `/merge` keeps a branch
current with its **still-open** base mid-flight; `/repoint` **changes** the base to `master` (and
rebases the history to match) after the parent lands. Two consistent strategies:

- **Strategy A — repoint the whole chain to `master` up front.** Before merging anything, `/repoint`
  every PR except the bottom one onto `master`. Now each PR is independent, branch deletes cannot
  cascade, and you merge bottom-up resolving conflicts per PR. Prefer this when the chain is short
  and the PRs are nearly independent anyway.
- **Strategy B — just-in-time.** Merge the bottom PR **without deleting its branch**, `/repoint` its
  dependents onto `master`, verify each dependent is still open and mergeable, and only then delete
  the merged branch and move on. More steps, easier to get wrong, but it keeps each PR's diff honest
  for reviewers right up until it lands.

Per PR, bottom-up: `/repoint` onto `master` (if not already) → `/merge` (bring `origin/master` in) →
resolve → push → `gh pr comment <N> --body '#automerge'` (or `gh pr merge <N> --squash`) → wait for
merged → **then** delete the branch, once no open PR still bases on it.

Wrapping is not part of this sequence: each PR's documents were wrapped when it was set ready for
review.

Consider promoting the chain instead: an orchestrator plus `pr_adopt` per PR gives you
`pr_stack_status`, journalled merges, and repointing that cannot forget a dependent.

## Worktrees pin branches — plan around it

**Git refuses to update a branch that is checked out in another worktree.** This is a git invariant,
not a tooling quirk, and it constrains every stack-wide operation:

```
fatal: cannot force update the branch 'feature/auth/token-store'
       checked out at '/path/to/worktree'
```

Consequences:

- A stack-wide rebase **cannot rewrite a branch pinned by a worktree**, and cannot fast-forward the
  trunk if the trunk is checked out somewhere (typically the primary clone). Free the branch first,
  or delegate the rewrite into the worktree that owns it (`git -C`), which is what
  `/pr-stack-rebase` cascade mode does.
- `pr_repoint` rebases in a **dedicated scratch worktree** for the dependent branch, so it does not
  fight a live checkout — but its `--force-with-lease` will still abort if that worktree pushed in
  the meantime. That abort is the feature.
- `pull_base_into_node_branch` (the row's "pull the base in" control) operates in the node's **own**
  worktree and **refuses** when there is no worktree for the branch. It deliberately does not fall
  back to checking the branch out in the main repo.

So: **use one working tree for stack-wide operations**, with the stack's branches free. Per-branch
worktrees are fine for *editing* a branch — `/green` never needs a stack-wide tool to move a branch —
but free or delegate them before anything that rewrites several branches at once.

**Plan in the current worktree**, not one worktree per PR. Record the starting **named** branch
(refuse a detached HEAD), do the work, and **restore the original branch** when done — if that branch
was `master`, this worktree ends on `master` (`git checkout master`, not a detached `origin/master`).
Do not keep a checkout of a node between visits: it pins the branch and blocks whoever should be
greening it.

A worktree in this repo is expensive — a populated `target/` plus `node_modules` runs to several
gigabytes, and half a dozen of them will exhaust a disk. Create per-node worktrees only when a child
session actually runs, and only for the nodes being worked. A node's worktree can always be
recreated later from its branch.

### Delete a temporary worktree as soon as its branch is pushed

**Default: once a branch is pushed and the work in that worktree is finished, remove it immediately**
— unless the user has asked to keep it. "Finished" means the in-stack loop has run its course:
`/green` pushed, `/validate-changes` found no gaps, and `/pr-wrap` is done. A push at the end of
`/green` is not by itself the signal to delete — the same worktree still runs `/validate-changes` and
`/pr-wrap`.

Nothing is at risk once the branch is on the remote: the worktree holds no unique state, and
everything untracked in it (`target/`, `node_modules`, `dist`, `.nix-profile`) is regenerable.

**Verify before removing** — cheap, and the only thing standing between "reclaim disk" and "lose
work":

```bash
W=<worktree-path>; B=<branch>
git -C "$W" status --porcelain | grep -v '^?? '        # must be empty: no tracked-file changes
git -C "$W" rev-list --count "origin/$B..HEAD"          # must be 0: nothing unpushed
git rev-parse "$B" && git rev-parse "origin/$B"         # must match
```

If anything is unpushed or uncommitted, **push first**. Never discard tracked work to reclaim space.

Then:

```bash
git worktree remove --force <worktree-path>   # --force: untracked build artifacts are expected
git worktree prune
```

Removal can take minutes while `target/` is deleted — reasonable to run in the background.

**Never delete the branch.** `git worktree remove` does not, and must not: a node's branch is its
children's base, and deleting it closes their PRs (golden rule 2). It is also the **durable link key**
for the whole stack — a node is bound to its branch, and a deleted session leaves the node orphaned
but recoverable precisely because the branch survived.

**Keep the worktree when:**

- The in-stack loop is still running in it — `/green` has pushed but `/validate-changes` or
  `/pr-wrap` has not finished. Remove it after `/pr-wrap`.
- The user asked to keep it, or is still working in it.
- It was handed to a human to continue in — e.g. opened in an editor. Removing that destroys their
  session; leave it.
- The branch has unpushed commits or uncommitted tracked changes. Push, then remove.

Per CLAUDE.md, **ask before deleting** anything you are not certain about.

To come back later: `git fetch origin <branch>` then `git worktree add <path> <branch>`, and
reinstall dependencies. Recreating is cheap; losing unpushed work is not.

## Keeping a branch current — per-worktree rebase vs a cascade

There are two ways to bring stack branches back onto their bases, and picking the wrong one is how
concurrent `/green` work gets clobbered.

| | `/pr-stack-rebase` (single) | `/pr-stack-rebase` (cascade) |
|---|---|---|
| Scope | **the current worktree's branch only** | **the nodes you named**, bottom-up |
| Where the rewrite runs | here | **in a worktree that owns each branch** — delegated via `git -C`, or borrowed here when nothing pins it |
| Pushes | that one branch, `--force-with-lease` | each branch individually, stopping at the first failure |
| Needs branches free | no — only the current one | no — pinned branches are delegated to, not fought |
| Use from | a per-node `/green` worktree | any clean worktree, including the orchestrator's |

**During concurrent implementation, each worktree rebases itself with single-mode
`/pr-stack-rebase`.** That is the shape that works when every stack branch is checked out somewhere,
and it is the mode every hard-gate caller (`/green`, `/validate-changes`, `/pr-wrap`) uses.

**Cascade mode covers the case single mode cannot**: several nodes went stale at once — typically
because a parent was greened and force-pushed — and nobody is sitting in each of those worktrees. It
rebases **only the named nodes**, runs each **inside a worktree that owns that branch**, refuses
outright when such a worktree has uncommitted or unpushed work, and pushes **one branch at a time**,
stopping at the first conflict.

Its load-bearing mechanic is `--onto` with each node's **recorded pre-rebase tip**: once node K has
been rewritten, node K+1's merge-base no longer exists on K, and a plain `git rebase origin/<base>`
there would replay **K's old commits** as K+1's own — silently duplicating a parent's work into its
child's diff. That is exactly what running single mode N times by hand gets wrong.

`/green`, `/validate-changes` and `/pr-wrap` **always** invoke `/pr-stack-rebase` on a stack branch
before they run implementation or a code diff — not only when something looks stale. Already-current
is verify-and-return (leak check, no rewrite). A leaked `origin/<base>..HEAD` is a stop: do not
analyze those files as this PR's work and do not delete them.

Rules for the per-worktree path:

- **Bottom-up.** A node is only worth rebasing once its parents' worktrees have rebased and pushed.
  Otherwise you rebase onto a tip that is about to move and pay for it twice.
- **Rebasing your branch leaves your children behind.** Expected, not a defect: each child clears it
  by running `/pr-stack-rebase` in its own worktree. Say so in your output so the next worktree knows
  it is their turn.
- **Never `git rebase --update-refs`** from a per-node worktree — it rewrites sibling stack branches
  all at once, which is the atomic, all-branches behaviour this path deliberately does not have.
- **A branch is only ever rewritten from a worktree that owns it.** Cascade mode delegates to the
  worktree that pins a branch and borrows the current one only when nothing pins it — restoring the
  original branch afterwards, including after a failure.
- **Cascade stops at the first conflict or failure**, and never continues onto a half-resolved or
  stale parent. Report which nodes completed, which stopped, and which were not attempted.
- **Never implement a parent-owned surface while resolving conflicts.** If the base's version of a
  file conflicts with your code, the base wins for anything the base owns.

### The gentler alternative: pull the base in

Rebasing is not the only way to catch up, and on a **reviewed** PR it is the expensive way (it
rewrites SHAs, force-pushes, and can dismiss approvals). A node that is cleanly behind its base can
instead **pull the base in** — `PullBaseIntoBranch` / the row's control, `merge` by default and
`rebase` on request. Its ordering is the safety design, and it is worth knowing before you reach for
a manual rebase:

1. Node absent, node owns no branch, or no base named → refused.
2. No worktree for the branch → refused, naming the branch.
3. **Worktree cleanliness is checked before the fetch and before anything touches git state.** Dirty
   refuses, unless the caller opted into committing first — in which case tracked changes are
   committed and pushed before anything else.
4. Fetch — **scoped to the base ref**, not a bare `git fetch origin`, so a rebase's
   `--force-with-lease` still sees the remote as it was.
5. Apply; a conflict is **aborted** and reported with its paths. Already-up-to-date changes nothing.
6. Push — plain for a merge, `--force-with-lease` for a rebase. A failed push is a **successful call
   reporting `pushed = false`** with the reason, not an error.

The gate is a clean tree, **not** session activity: an active session with a clean tree is the normal
case. Untracked files are deliberately ignored — git refuses loudly rather than clobbering one, and
blocking on them would make the control permanently dead in any real agent worktree.

## Not on its base's latest tip — the commit-after-branching trap

A branch is stale when **it does not contain the tip of its base**. The usual cause: a commit was
added to a parent after its child was already cut from it.

It is easy to miss because GitHub's PR diff still looks correct — the diff is computed from the
merge-base, so each PR keeps showing only its own delta. Nothing is visibly broken; the branch is
just built on a stale parent, and it will surface later as a conflict during landing.

Detect it, parent by parent:

```bash
git fetch origin <base> <branch>
git merge-base --is-ancestor "origin/<base>" "origin/<branch>" \
  && echo "OK: <branch> contains <base>" \
  || echo "STALE: <branch> is not on <base>'s latest tip"

# how far behind, and how far ahead, in one process:
git rev-list --left-right --count "origin/<base>...origin/<branch>"   # behind<TAB>ahead
```

On a planned stack you do not have to run that yourself: the PR-Stack view's **base-sync** leg polls
exactly this on every tick — `behind_count`, `ahead_count`, and a **non-mutating** conflict probe via
`git merge-tree --write-tree` (which writes a tree into `.git/objects` but touches no index, working
tree, `HEAD` or ref, and is therefore safe against a live checkout). `behind_count == 0`
short-circuits the probe. See
[`pr-stack-live-status.md` § Base sync](../../../docs/ft/coder/pr-stack-live-status.md#base-sync).

Two caveats on those numbers:

- **The probe never fetches.** The counts are only as fresh as the last fetch. The pull action
  fetches, and the next tick sees the truth.
- **`has-conflicts` from the live probe is a fact about right now**, not a persisted status. The
  `internal_status` derivation never writes it from the poll, and the pull path deliberately does not
  stamp it — a persisted copy could only go stale, and clearing it would risk stomping an agent's
  `source: "override"`.

Fix it with `/pr-stack-rebase` in the worktree that owns the branch, bottom-up, or with the pull-in
path above when the branch is cleanly behind. Rebasing rewrites SHAs and force-pushes — harmless on a
draft with no reviews, but on a reviewed PR it can dismiss approvals (see below), so **say so before
doing it**.

## Recurring shared-file conflicts

When every PR in a stack edits the **same file**, merging each onto the growing `master` conflicts on
that file every time. Resolve by **union** — keep every PR's contribution — not `ours` / `theirs`.
In this repo the usual suspects are:

| File | Why it conflicts | How to resolve |
|---|---|---|
| `Cargo.lock` | any dependency change touches it | regenerate — `cargo build` — do not hand-merge |
| `bun.lock` (repo root) | same, for the JS workspace | regenerate with `./dev bun install` from the root |
| `packages/tddy-service/proto/*.proto` | two nodes each claiming "the next" field number | **not a union merge**: renumber so every field number is unique and never reuse a retired one |
| a registry / match arm listing every variant | each node adds its own arm | union, then check the exhaustive matches still compile |

**Changelogs and changeset histories are deliberately not in this table.** They used to be the worst
offenders — every node of a stack prepending to the same `changesets.md` inside one merge window,
with git's `union` driver papering over it until it stranded a block mid-file. They are now
directories of one file per entry (`docs/dev/changesets/`, `packages/*/docs/changesets/`,
`docs/ft/*/changelog/`), so each node adds its own file and there is nothing to conflict on. Give
each node a **distinct slug** — several nodes of one stack land on the same date — and never edit a
sibling's entry. See [changelog-merge-hygiene.md](../../../docs/dev/guides/changelog-merge-hygiene.md).

Planning documents are **not** in this category either: each PR carries its own PRD and changeset in
`docs/dev/1-WIP/`, and the stack's own shared document lives on the orchestrator session, outside
every PR's diff. The stack itself contributes no shared file to conflict on.

## Approval dismissal after resolution commits

If branch protection dismisses stale approvals, a conflict-resolution commit — or any rebase
force-push — drops a prior approval. Re-trigger the gate **after** that commit, not before:

```bash
gh pr comment <N> --body '#automerge'
gh pr view <N> --json reviewDecision,mergeStateStatus,statusCheckRollup
scripts/ci-status.sh --watch <N>
```

Two things to know about arming it again:

- `#automerge` asks the **checks API** (`gh pr checks --required`), not `mergeStateStatus`, because
  that field answers "is this blocked *for you*" and the Actions app is a bypass actor. So re-arming
  it after a force-push is safe: it will queue rather than merge while checks are re-running.
- It is still a merge **into this PR's base**. On a non-bottom stacked PR, that base is a parent's
  branch. Re-read golden rule 2 before arming anything.

## Recovery — a PR closed because its base branch was deleted

The **head branch is usually still intact** (only the base was deleted). You cannot reopen or re-base
the closed PR — recreate it against `master` from the surviving head branch:

```bash
git push -q origin <head-branch>
gh pr create --base master --head <head-branch> --title "<same title>" \
  --body "Recreated after #<N> auto-closed when its stacked base branch was deleted."
```

Then reattach it to the stack. On a planned stack, the node still records the **closed** PR's URL, and
`pr_adopt` refuses a PR whose head branch is already bound to a node — so:

1. `pr_delete_planned` the stale node (allowed: its PR is closed, not open), which reparents its
   children onto its parents;
2. `pr_adopt` the recreated PR, binding a fresh node to the same head branch;
3. `pr_set_parents` on that node to restore its real dependencies, and on any child that needs
   re-pointing at it;
4. `pr_stack_status` to confirm the shape.

**This entire section is the thing `pr_repoint` exists to prevent.** Repoint every dependent *before*
deleting a merged parent's branch and none of the above ever happens.

## Recovery — the stack is real but not local

Worktrees deleted, a different machine, or a child session cleaned up. Nothing is lost: the stack's
durable link key is the **branch**, not the session.

**The orchestrator session still exists.** Read the truth back with `pr_stack_status` — it reconciles
`Changeset.stack` against live git and GitHub — and recreate whichever worktrees you actually need:

```bash
git fetch origin
git worktree add <path> <branch>        # per node you intend to work
```

A node whose child **session** was deleted keeps its `session_id` and becomes **orphaned**: the row
offers "Start session" again, and the resumed spawn re-links to the node through its **effective
spawn branch**, so the recovery sticks rather than producing another unlinked session each click.

**The orchestrator session is gone.** Rebuild it: start a new `pr-stack` session (or seed one from an
existing session with **Base the stack on**, which makes that session's branch the stack's bottom
node), then `pr_adopt` each surviving PR, then `pr_set_parents` to restore the DAG. Recover the shape
first with the ad-hoc detection recipe — `gh pr list --state open --json number,headRefName,baseRefName`
plus `git merge-base --is-ancestor` — so you are adopting into the right order rather than guessing.

Note the two refusals you will meet while seeding: a base session that carries an
`orchestrator_session_id` is already a node of another orchestrator's stack and is refused (two
orchestrators with repoint and merge authority over one branch is ambiguous ownership), and exactly
**one** session may seed a stack — a linear order over several pre-existing branches would declare
dependencies their git history does not have.
