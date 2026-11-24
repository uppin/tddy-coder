---
description: Plan a streak - a set of PRs planned together, joined into gh stacks only where they depend on each other - in two passes. Pass 1 decomposes and records the links (each node's dependencies and its stack, if any) in the node docs and the work queue. Pass 2 makes them real - cuts each branch on its base, commits the docs (commit 1), opens the draft PR, publishes the owned API surface and failing tests (commit 2), revises the links the code disproves, and registers each stack with gh stack link - then finishes with a per-stack cascade /pr-stack-rebase
---
## Plan Streak — From Requirements to a Set of Draft PRs, in Two Passes

Alternative to `/plan-red` when the work is large enough to split across **several PRs planned
together**. Interview the user, decompose the work into self-contained PRs, decide how they link, and
then build them in **two passes**, both in the **current worktree**, one branch at a time.

A **streak** is that planned **set** of PRs. It is **not** one stack:

- PRs that **depend on each other** are joined into a `gh stack` — one or several, each of them
  **linear**, exactly as the `pr-stack` skill defines a stack.
- PRs that depend on nothing else in the streak, and that nothing else in it depends on, are
  **standalone**: their branch is cut from the trunk, their PR is based on the trunk, and no stack
  registers them.

Load the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`) first. It is the principles skill for
**one stack** — registering it with `gh stack`, the **PR boundary contract**, the per-PR documents and
their headings, base tracking, doc linking by branch or PR number, the worktree-pinning constraint,
PR titles, *Order inside a wave* and the landing rules. This command applies it **to each stack the
streak contains**, and applies its boundary contract, documents and title rules to **every** node,
stacked or not. It never contradicts that skill.

> **Not the `pr-stack` workflow recipe.** `tddy-coder` once shipped a product feature by a similar
> name — a workflow recipe (`pr-stack`, legacy aliases `plan-pr-stack` / `orchestrate-pr-stack`), now
> retired — and still ships the **work queue** a tddy session can own. Both are tddy *product*
> features, documented in [`docs/ft/coder/pr-stacking.md`](../../docs/ft/coder/pr-stacking.md). This
> command is how *this* repository's contributors plan a streak by hand. It **uses** the work queue as
> a record when the session owns one (Step 4d); it never depends on it.

### The streak model

| Term | Meaning |
|---|---|
| **Node** | One planned PR. A vertical slice under the `pr-stack` boundary contract — never a layer, never stubs. |
| **Dependency** | `nⱼ → nₖ`: `nₖ` consumes something `nⱼ` delivers — its **surface** (a signature it compiles against) or its **behaviour** (a test of `nₖ` exercises it). Recorded per node in `## Dependencies`. |
| **Behaviour need** | The subset of dependencies where a test of `nₖ` exercises `nⱼ`'s behaviour. These, and only these, decide the **green waves** (`## Green wave`). |
| **Unit** | Where a node is placed: a **stack** (named after its root node, `stack <root-node>`) or **standalone**. Recorded per node in `## Placement`. |
| **Links** | A node's dependencies plus its placement — its base, its predecessor in its stack, the nodes it waits on. |

**The placement rules** — the links must satisfy all three before pass 2 cuts a branch, and again
whenever pass 2 revises them:

1. **Every unmerged dependency is an ancestor.** If `nₖ` depends on `nⱼ`, then `nⱼ` sits below `nₖ`
   in the **same stack**, so `nₖ`'s branch contains `nⱼ`'s code. This is a git fact, not a preference:
   a branch cannot compile against, or test, code it does not contain.
2. **Join only what depends.** Two nodes share a stack only when a chain of dependencies connects
   them. A node with no dependency and no dependent in the streak is **standalone**. Never put an
   independent PR into a stack for grouping or reading order — the streak slug in its title already
   groups it, and a stack would make it wait on merges it does not need.
3. **One connected group, one stack.** The nodes a chain of dependencies connects form one stack,
   which is linear (`pr-stack` § *A registered stack is linear*). Where that group branches, flatten
   it (`pr-stack` § *Why linear*) and say what it cost. When flattening costs too much — two large,
   independent subtrees on one small root — the alternative is to **take the subtree out of this
   streak**: it becomes a later streak (or PR) that starts once the root has merged, recorded as a
   `docs/dev/todo/` entry naming the node that releases it. A stack is never based on another stack's
   branch, and a branch is never based on another streak node without that node being its
   predecessor in a registered stack.

So a streak's shape is decided by its dependency graph: a chain becomes one stack, a set of
independent changes becomes standalone PRs, and anything in between becomes several stacks plus
standalone PRs.

### The two passes

- **Pass 1 — decomposition: plan the links.** Decide every node, its dependencies, its behaviour
  needs and its unit. Write each node's PRD + changeset + initial discovery as **drafts** in this
  worktree, and record the nodes and their links in the **work queue** when this session owns one.
  **Pass 1 creates no branch, no commit and no PR.** When it ends, the user has approved a streak map
  — every unit, every stack's line, every standalone PR — and nothing exists on GitHub yet.
- **Pass 2 — contracts and compilation: make the links real.** Visit the nodes in streak order. For
  each: cut its branch on its base (the trunk for a stack root or a standalone node, its
  predecessor's pushed tip otherwise), commit its docs (**commit 1**), push, open its **draft PR**,
  then land what its `## Draft PR contract` promised — **the API surface it owns plus its failing
  tests** — as **commit 2**, and push. Once every member of a stack has its contract commit, register
  that stack with `gh stack link`.
- **Pass 2 revises the links. Expect it.** Pass 1's links are a prediction made from documents;
  pass 2 writes the real signatures and the real tests, and they show the true dependencies. A
  dependency the plan named may turn out unneeded, an unplanned one may appear, a behaviour need may
  turn out to be surface only, and a standalone node may turn out to need a stacked one. Every such
  finding is applied in pass 2 (Step 6h), before the stack it touches is registered where possible,
  and reported. A pass 2 that changed no link is possible, but it is not the expected outcome, and it
  should be stated rather than assumed.
- **Completion gate.** When pass 2 ends, restore the starting branch, re-check every stack's wave
  order, and run a **cascade `/pr-stack-rebase` per stack**. Every layer is expected to come back
  *already current, leak-free*; anything else is brought current before the streak is handed off.

> **Pass 2 is not a stubs phase, and no node's deliverable is stubs.** The boundary contract is
> absolute here: *a node that ships only surface is not a valid PR*. What pass 2 publishes is the
> **first push of a PR that will go on to implement the thing** — the interface plus its failing
> tests, early, so dependents can branch off a real ref. `/green` finishes each node in the same PR.
> **A node must never merge in its pass-2 state.** If you catch yourself planning a node whose whole
> job is to declare surface for a later node to implement, that is a layer split — go back to Step 1
> and cut by capability instead.

Another agent may pick up `/green` on any node **only after the completion gate** — pass 2 cuts and
rewrites branches here, and a branch pinned in another worktree cannot be checked out here. After the
gate, **this worktree is back on the branch it started on** — if that was `master`, it must be on
`master` again (a named branch checkout, not detached `origin/master`, not a streak branch).
`/yolo-streak` is the exception: it starts each node's implementation while pass 2 is still running,
under the hooks it defines.

**Prerequisites**
- `gh` CLI authenticated (`gh auth status`).
- The user has described a body of work that plausibly spans more than one PR.
- The `gh stack` extension available (`gh extension list | grep gh-stack`) — needed as soon as the
  streak contains a stack.

> **Every stack ends up registered on GitHub; no standalone PR is.** Base refs alone give correct
> per-PR diffs and nothing else — no grouping, no ordering, no stack view for reviewers. Step 7
> registers each stack with `gh stack link`. Because `gh stack` models a **line**, each stack's
> order must be linear: see the `pr-stack` skill § *A registered stack is linear*. Do not run
> `gh stack sync` / `gh stack rebase` here — they rewrite branches other worktrees may own;
> `/pr-stack-rebase` is the tool for that.

## Execution Flow

**Record the starting branch before anything else.** This worktree is the planning worktree — do
**not** create a second one — and it must be restored when the streak is complete, **including when
that starting branch is `master`**:

```bash
ORIGINAL_BRANCH=$(git branch --show-current)
TRUNK=$(git symbolic-ref --short refs/remotes/origin/HEAD 2>/dev/null | sed 's|^origin/||')
TRUNK=${TRUNK:-master}   # override after the interview if the user named a different trunk
git status --porcelain | grep -v '^?? '   # must be empty: no tracked-file changes
```

If `$ORIGINAL_BRANCH` is **empty**, HEAD is detached. **Stop.** A detached start cannot be restored
to a named branch in Step 8, and the worktree would be left on the last streak branch, pinning it.
Ask the user to check out a named branch first — usually `master`.

If the tree has tracked-file changes, **stop** and ask the user to commit them (never bare
`git stash` — see CLAUDE.md on the shared stash stack). If `ORIGINAL_BRANCH` is already a streak
branch from a previous attempt, confirm with the user before continuing.

**Pass 1's drafts are untracked files in this worktree** until pass 2 commits each one on its own
branch. Never `git clean` this worktree while planning, and never `git add -A` here.

### Step 1: Interview — MANDATORY

Run the interview from `.agents/skills/planning/references/planning-phase.md` **Step 1**
(What / Why / Where / Constraints / UX / Scope). Then ask the **streak-specific** questions:

1. **Decomposition** — What are the natural, independently-shippable slices? Each must be a
   **vertical slice**: the API/schema change, the code implementing it, and its tests in **one**
   node.
2. **Dependencies** — For each node, which other nodes does it consume something from — a surface it
   compiles against, or a behaviour its tests exercise? Record the real edges and mark which are
   **behaviour needs**. Do not invent an edge to make the picture tidier: an edge is what decides
   whether a node waits.
3. **Placement** — apply the three placement rules (*The streak model*). Group the nodes the edges
   connect; each group is one stack, each unconnected node is standalone. Where a group branches,
   decide between **flattening** it (state which siblings become predecessor and successor, and what
   that serializes) and **taking a subtree out** into a later streak (state what releases it).
4. **Ownership** — Which node owns which API surface / files? Each symbol has exactly one owning
   node, and no node ever implements a symbol another owns. Two nodes that would edit the same file
   heavily are a hint at a dependency the edges missed — or at a shared-file conflict the stack will
   carry (`pr-stack` § *Recurring shared-file conflicts*); a standalone pair that shares one will
   conflict at whichever merges second.
5. **Streak size** — How many nodes? Prefer the fewest that keep each independently reviewable and
   independently mergeable.
6. **Sequencing facts** — For each node, does it consume a capability another node has not shipped
   yet? That is a scheduling fact to record in its `## Dependencies`, **not** a licence to build the
   other node's half.
7. **Backlog prerequisites** — did Step 2b turn up an entry large enough to need its own node? If so
   it is a node like any other, linked to the nodes that need it: below them in their stack, or a
   standalone PR when nothing else in the streak needs it. A prerequisite node is a legitimate node: a
   vertical slice (the fix, its tests, its docs), independently mergeable. It is *not* the forbidden
   stubs-only shape, because it delivers behaviour.
8. **Green waves — and the streak order they impose** — from the behaviour needs, group the nodes
   into waves: wave 1 is every node needing no other node's *behaviour*, wave 2 every node whose only
   unmet needs are in wave 1, and so on. A standalone node is wave 1 by construction.

   **Then derive the streak order from the waves.** This is the step most easily skipped, and
   skipping it is what leaves a wave in whatever order its nodes happened to be named in. Authority:
   the `pr-stack` skill § *Order inside a wave*.

   - The streak order is the **waves concatenated**: all of wave 1, then all of wave 2, and so on.
   - **Inside a wave, blockers lead.** Sort by what waits on the node — most transitive dependents
     first, nodes nothing depends on (every standalone node among them) last. Break ties by
     **earliest dependencies first**, then by the most foundational node. A node that consumes another
     wave-mate's *surface* sits after it.
   - **Then label.** Node `n<k>` is the `k`-th node in the streak order, so the labels count up along
     it. Never carry interview-order labels into the plan.
   - **Each stack's line is the streak order restricted to that stack's members.** It is then
     automatically a valid topological sort, its labels count up along it (with gaps where other
     units' nodes sit), and its waves never step back.
   - **The waves never step back along a stack.** Read bottom to top, a stack's wave numbers form a
     non-decreasing run (`1 1 2 2 3`, never `1 2 1`). A wave-2 node below a wave-1 node in one stack
     is **illegal**, even when every real edge is still satisfied: the wave-1 node above it can no
     longer merge until the wave-2 node does. Steps 6h and 8b enforce this again after the plan is
     made. Standalone nodes have no line and cannot step back.

   ```
   edges    n1 → n2, n3      n2 → n4      n5 → n6      (n7, n8: none)
   units    stack auth-store: n1 · n2 · n3 · n4     stack web-login: n5 · n6     standalone: n7, n8
   waves    1: n1, n5, n7, n8     2: n2, n3, n6     3: n4

   ✓ streak order   n1 n5 n7 n8 · n2 n3 n6 · n4
         stack auth-store   n1 · n2 · n3 · n4    waves 1 2 2 3   n2 (1 dependent) before n3 (none)
         stack web-login    n5 · n6              waves 1 2
   ✗ one stack     n1 · n5 · n7 · n8 · …     n7 and n8 depend on nothing, yet would wait on n1 and n5
   ```

   Present the units, the waves **and** the derived order, name the transitive-dependent count that
   put each wave-leading node where it is, and say what any flattening cost.

**Hold the boundary contract while decomposing.** These pairs are one node, never two:

| ✗ Layer split (invalid) | ✓ One self-contained node |
|---|---|
| `n1` add proto RPCs → `n2` implement them | `n1` attachment staging: proto + daemon handler + tests |
| `n1` add an endpoint → `n2` add its handler | `n1` the endpoint, serving real responses |
| `n1` add a data model → `n2` persist it | `n1` the model with its persistence |
| `n1` change a signature → `n2` fill in the body | `n1` the working function |

When a vertical slice is too large, **split by capability, not by layer**: one source variant rather
than all of them, one enum case or scope, one screen or entry point, the happy path before the edge
cases. The two narrow exceptions — a purely mechanical rename/move/extraction with no behaviour
change, or a regeneration of already-committed generated code exposing no new surface — are the only
ones; **do not invent a third**. Anything that seems to need one goes in the node's description for
the user to decide.

Present the proposed **streak map**: the nodes (n₁…nₙ with one-line scope + owned surface each), the
real dependency edges with the behaviour needs marked, the **units** they imply (each stack's line,
each standalone node), the **green waves**, **how the order inside each wave was sorted** (blockers
first, with each leading node's dependent count), and anywhere a stack was flattened rather than
following a real dependency. Say plainly that these links are pass 1's prediction and that pass 2 is
expected to revise some of them. **Wait for user approval of the decomposition before writing any
node document.** If the user named a trunk other than the detected one, set `TRUNK` to it.

### Step 2: Analyze Existing Code

Pick a **whole-work discovery slug** before launching any exploration — there is no per-node slug
yet, but the dump must exist as soon as the first pass returns, so a rejected PRD or a stop before
Step 4b cannot lose it:

```
docs/dev/1-WIP/{YYYY-MM-DD}-{streak-name}-whole-work-initial-discovery.md
```

`{streak-name}` is a short kebab name for the whole streak (from the interview), not a node slug.
Create that file **before** the first Explore / Grep / Glob / Read, then persist each pass into it per
`.agents/skills/planning/references/initial-discovery.md` (combined conclusions at the top; each pass
as `## Exploration N` at the tail).

Then run `.agents/skills/planning/references/planning-phase.md` **Step 2** (code analysis → State A),
**Step 2b** (deferred-work cross-check — **both** records) and **Step 3** (product area), once for
the whole body of work.

**Step 2b now scans two records**, per
[`deferred-work/references/planning-cross-check.md`](../skills/deferred-work/references/planning-cross-check.md):
`packages/<pkg>/docs/code-issues/` as well as `docs/dev/todo/`. Two consequences specific to a streak:

- **A code issue this streak will fix is claimed by the node that fixes it.** After the streak's PRs
  exist, add `**Claimed by:** #NNN — `#<slug> K/N`` and `**Lands after:**` to each such record, so a
  concurrently-planned change finds the owner rather than re-planning the same refactor. Do this at
  **Step 7**, which is the first moment every PR number exists.
- **A code issue claimed by a *different*, already-running streak or stack is a 🚧 Claimed verdict**,
  and it forks the same way it does for a single PR: stop and ask the developer whether this streak
  proceeds on today's shape, waits for the named PR, or cuts a node's scope to avoid the collision. A
  stacked node is the *worse* place to guess, because the wrong answer is multiplied by every node
  above the collision.
Record which packages each node touches.

**Step 2b matters more for a streak than for a single PR**, because a streak has somewhere to put the
answer. Scan `docs/dev/todo/` for items in the path of this work, classify each — blocking /
during / answered / unrelated — then decide its **shape**:

| Size of the fix | Shape |
|---|---|
| Small, inside one node's own files | Do it in that node; record it in that node's `## Prerequisites` |
| Small, in shared code more than one node touches | **Its own node**, which the affected nodes depend on — so it sits below them in their stack, and joins them into one ([`planning-cross-check.md` § *A systemic restructure*](../skills/deferred-work/references/planning-cross-check.md#a-systemic-restructure-across-a-pr-stack-or-streak)). Name it in every affected node's `## Prerequisites` |
| Large | **Its own node**, linked to the nodes that need it (Step 1 question 7) — or its own PR outside the streak when it is orthogonal to the feature |

⚠ **Do not absorb a large refactor into a feature node.** It buries a reviewable change under a
mechanical one, and it breaks the boundary contract's promise that a node is independently
reviewable. The `connection_service.rs` entry in this repo records exactly that trade as the reason
nobody has split it.

**A blocking entry is one where every route around it is wrong** — not one you merely find annoying.
The test: name the two or three ways to proceed without it and check whether each is *incorrect*
rather than inconvenient. If a node ships a new secret at rest, and the shared write helper would
create it world-readable while the hand-rolled alternative deepens the very debt the entry records,
that is blocking.

Carry the verdicts into Step 4b: each node's `## Prerequisites` lists the entries **that node** runs
into — **each with a relative link to its file** — and a blocking one also earns a line in that
node's `## Scope`. Record the ones you decide not to fix too; the value is that a reviewer sees the
entry was considered rather than missed.

**Decide which node *claims* each entry it fixes**, and mark it ✅ RESOLVED HERE there only. That
node's `/wrap-context-docs` deletes the entry's file, so two nodes claiming one entry means the
second re-deletes a file another node already dropped. When a fix is split across nodes, the
**earliest** in streak order claims it and the others keep the reference with what their slice
covers. When those nodes are in different units, the split itself is a dependency the placement
rules must see — the later node depends on the earlier one.

This whole-work file is **temporary**. It is **not** a changeset companion, and wrap of any node must
not delete it as if it were. **Never commit it on a streak branch.** When each node's changeset is
drafted in Step 4b, **copy** the whole-work dump into **that node's** `{slug}-initial-discovery.md` as
Exploration 1 (full dump, not a pointer), and append any node-specific follow-up as later Exploration
sections — in pass 1 while drafting the changeset, and again in pass 2 (Step 6) if the contract needs
more exploration. After every node has its own companion, **delete** the whole-work source at the
pass-1 checkpoint (Step 5) — **ask the user first**, per CLAUDE.md. If planning stops before all 4b
companions exist, **keep** it.

### Step 3: Settle the plan — no streak document is committed

**There is no `docs/dev/1-WIP/STACK-*.md` or `STREAK-*.md`, and you must not create one.** The
`pr-stack` skill rules it out for three reasons — it would outlive every PR and so could never be
wrapped, every node would edit the same file and every rebase would conflict on it, and its status
column would be a stale hand-copy of what `gh pr list` already knows. Each node's links live in **its
own changeset**, and the whole graph, when this session owns a work queue, lives in that queue.

What this step produces is agreement, not a file. Settle and write down for your own use:

- the **streak order** and the labels `n1…nN` — the green waves concatenated, blockers leading each
  wave (Step 1 question 8). Write down the dependent count that justifies each wave-leading position,
  because Step 10 reports it and a reviewer can check it;
- the **units** — each stack's name (its root node's name), its members bottom to top, and every
  standalone node;
- **`TRUNK`**;
- the **streak slug** — one or two kebab-case words, associative, chosen **once and never changed**.
  It is both the branch namespace `feature/<streak-slug>/<node>` and the PR-title group
  `(#<slug> K/N)` for **every** node of the streak, stacked or standalone, so one word identifies the
  streak in a branch list and in a PR list without a lookup. `attach-docs`, `sandbox-split`,
  `base-sync` are good; `stack-3`, `daemon-refactor`, `phase-two` are not. See **PR titles** in the
  `pr-stack` skill;
- the **owned API surface** per node (this becomes each changeset's `## Responsibility` and
  `## Boundaries`);
- the **draft-PR contract** per node — what surface plus failing tests pass 2 publishes first.

`K/N` in a streak's titles is the node's **streak label** and the streak's node count: `n4` of a
nine-node streak is `4/9` whichever unit it sits in. Along each stack the `K`s ascend, with gaps where
other units' nodes sit — that is what tells a reviewer the PR belongs to a larger streak.

Branch names are `feature/<streak-slug>/<node>` — every node sharing one namespace. Do not invent
`pr-2-<slug>`; that shape belongs to a different repo. **`<node>` is a name, never a number or a
position** (`headless-driver`, not `n4-headless-driver`). A reorder changes positions and leaves
branch names alone, so a number in a branch name ends up contradicting the node's label.

## Pass 1 — decomposition: plan the links

**Pass 1 creates no branch, no commit and no PR.** It writes drafts and records links. Stay on
`$ORIGINAL_BRANCH` throughout.

### Step 4: For each node in streak order (n₁ → nₙ) — draft its documents, record its links

**4a. Fix the node's identity.** Its label `n<k>`, its branch name `feature/<slug>/<node>` (planned,
not created), its document slug — distinct per node, because several nodes land on the same date and
each adds its own files — and its planned title in the full `pr-stack` format.

**4b. Draft this node's PRD + changeset + initial discovery** in `docs/dev/1-WIP/`, untracked.

- Follow `.agents/skills/planning/references/planning-phase.md` **Step 4** (PRD) and **Step 5**
  (changeset), scoped to **this node only**. Present each PRD for approval before its changeset.
- **Add a `## Prerequisites` section** to any node whose Step 2b scan found something — the entries
  *that node* runs into, each with a verdict **and a relative link to the entry's file**
  (`[2026-08-02-slug.md](../todo/2026-08-02-slug.md)`), plus a `## Scope` line for anything blocking.
  Omit the heading on nodes the scan cleared; an empty section is noise. Keep any node list inside it
  **accurate** — "nodes 1, 3 and 6 touch this file" is a claim a reviewer will check, so verify it
  against the diffs rather than writing it from memory.

  The link is not decoration: an entry a node **fixes** is marked `✅ RESOLVED HERE`, and that node's
  `/wrap-context-docs` **deletes the file** — the section is the only memory the wrapping session has
  of what the scan found. **Exactly one node claims an entry**; the others keep it as a reference with
  their own verdict.
- **Every changeset MUST carry a `**Complexity**: 1–5 — <signals>` header line**, scored with the
  rubric in `.cursor/rules/changeset-doc.mdc` § Header Metadata. It sizes whoever implements the node
  (`/yolo-streak` picks each subagent's model and effort from it). Re-score it in 6g if the failing
  tests showed the node is bigger or smaller than planned.
- **Every changeset MUST carry six headings** — the five the `pr-stack` skill requires of every node,
  plus `## Placement`, which is the streak's own:

  ```
  ## Responsibility        what this PR owns
  ## Boundaries            what it explicitly does not do
  ## Placement             its unit (stack and position, or standalone), its base, and why
  ## Dependencies          per node it depends on: what that PR delivers that this one consumes
  ## Draft PR contract     what lands first (API + failing tests) to unblock dependents
  ## Green wave            whether this node can be greened now, and alongside which others
  ```

  A standalone node still carries all six; its `## Dependencies` says it has none.
- **`## Placement`** records the unit pass 1 decided, so an implementer can see where the node sits
  without reading the others' documents:

  ```markdown
  ## Placement

  **Streak:** `#auth` — n4 of 8
  **Unit:** stack `auth-store` — position 4 of 4 (n1 · n2 · n3 · **n4**)
  **Base:** `feature/auth/session-api` (n2)
  **Why stacked:** consumes `SessionApi::open` (n2's surface) and its tests exercise n1's token store

  <!-- or, for a standalone node: -->
  **Unit:** standalone
  **Base:** `master`
  **Why standalone:** depends on no node of the streak, and no node depends on it
  ```

  It is a **prediction** until pass 2 has cut the branch; 6b and 6h correct it.
- **`## Dependencies` is the duplicate-development guard.** Whoever runs `/green` on this node has
  never seen the plan's reasoning and has no other way to learn that another node is already adding
  the trait they are about to add. State it **per node it depends on**, in a table:

  ```markdown
  ## Dependencies

  What each node this PR depends on delivers that this PR consumes. These surfaces are **theirs to
  create**; implementing one here collides with the PR that owns it.

  | Node | What it delivers | How this PR consumes it | This PR does NOT |
  |---|---|---|---|
  | `n1` token-store | `TokenStore::{put,get}` in `packages/tddy-github/src/token_store.rs`, persisted | middleware calls `get` on every request | add persistence, change the trait, or widen the key type |
  ```

  Describing inherited state in *State A* is not a substitute — State A says what exists, this says
  what the implementer must **not** build. Under the placement rules, every node in this table is
  also below this one in its stack.
- **`## Draft PR contract`** states what pass 2 will publish for this node: the owned API surface plus
  the failing tests that specify it. Write it as *the first push of this PR*, never as this PR's
  deliverable.
- **`## Green wave`** answers three questions a would-be implementer asks before touching anything:
  can this node be greened now, which nodes can be greened *alongside* it, and which node is waiting
  on it. Write the first draft here from the planned decomposition, and **revisit it in pass 2** once
  the tests exist — the tests are what actually decide it:

  ```markdown
  ## Green wave

  **Wave:** 1 of 3
  **Greenable independently:** yes — every test mounts this node's own component or injects a double
  **Concurrent with:** n2, n3, n5, n6 (PR numbers replace these in pass 2)
  **Blocks:** n4 (its add-key test needs this node's agent client to actually perform the add)

  The streak's real dependency edges (behaviour needs marked `⇒`):

      n1 ⇒ n2, n4      n1 → n3      n5 ⇒ n6      (n7, n8: standalone)
  ```

  The discriminator is narrow, and it is **not** the branch order: a node is greenable now when its
  tests need only its own surface plus its dependencies' *published* surface, and not when a test
  exercises another node's **behaviour**. Always give the reason — "no" is actionable only when it
  names the behaviour that must exist first.
- **Reference other nodes by label, branch or PR number, never by document path; own files only.**
  Nodes wrap in whatever order they finish, so a path into another node's `1-WIP` document can dangle
  in either direction. In pass 1 there are no PR numbers yet — use the label and the planned branch;
  pass 2 adds the numbers.
- Check off the first TODO items in this node's changeset (`Record initial discovery`,
  `Create/update PRD documentation`, `Create changeset`). This node's `{slug}-initial-discovery.md`
  must exist next to the changeset — copy the Step 2 whole-work file into it as Exploration 1 (full
  dump), then append node-specific follow-up. Do **not** delete the whole-work source yet.

**4c. Do not commit.** The drafts stay untracked in this worktree until 6b commits each node's set on
that node's own branch. There is no branch to commit them to yet, and committing them on
`$ORIGINAL_BRANCH` would put every node's plan into an unrelated history.

**4d. Record the node and its links in the work queue — when this session owns one.** A tddy session
started *with work queue* (every orchestrator-role session is) owns a queue whose id is the session's
own id; `work_queue_list` shows it. A queue is created only at session start, so a session without
one cannot make one now — skip this step, and the node documents are the only record.

With a queue, per node:

- `work_item_add` — title (the planned PR title), description (the node's `## Responsibility` and
  `## Placement`, self-contained: the description is what a reader of the queue sees), and
  `branch_suggestion` = `feature/<slug>/<node>`. Note the item id it returns next to the label.
- `work_item_set_parents` — the node's **planned base**: `[<predecessor's item>]` for a stacked node,
  `[]` for a stack root or a standalone node. `parents` keeps its git meaning in the queue — where a
  branch is cut and rebased — so it holds the stack predecessor only, never the full dependency list.
- `work_queue_set_dependencies` — the node's **behaviour needs** (`needs`). These are the edges the
  queue derives waves and readiness from, so they hold exactly what `## Green wave` lists as needs —
  no surface-only dependency, which the stack ancestry already carries.

After the last node, read `work_queue_waves` and `work_queue_graph`: the waves, the derived line and
the line check must agree with Step 1's streak order. A disagreement is a planning error — fix the
plan or the record before Step 5, and say which was wrong.

### Step 5: Pass 1 checkpoint — the streak map, before anything exists on GitHub

1. **Check the links against the placement rules** — every dependency in a `## Dependencies` table
   sits below its dependent in the same stack; no standalone node has a dependency or a dependent;
   the dependency graph is acyclic; the labels count up along the streak order; no stack's waves step
   back. A failure is fixed in the drafts (and the queue) now, while nothing has been cut.
2. **Confirm no branch and no commit were made:**
   ```bash
   git branch --show-current                 # must equal ORIGINAL_BRANCH
   git status --porcelain | grep -v '^?? '   # must be empty — the drafts are untracked
   ```
3. **Delete the Step 2 whole-work discovery source** — every node now has its own
   `{slug}-initial-discovery.md` draft. **Ask the user before deleting** (CLAUDE.md). It was never
   staged, so this drops an untracked file. If any node still lacks a companion, **keep** the file and
   stop here.
4. **Report pass 1 to the user** — the streak map as the drafts now state it: every unit, each stack's
   line with labels and waves, every standalone node, each node's planned title and branch, and any
   link the drafting changed since the Step 1 approval. **If a link changed, wait for approval**;
   otherwise continue into pass 2. Say plainly that **no node is ready for `/green` yet** and that
   pass 2 is expected to revise some links.

## Pass 2 — contracts and compilation: make the links real

### Step 6: For each node in streak order (n₁ → nₙ) — cut, commit 1, open the PR, surface, failing tests, commit 2, push

This is the `/plan-red` red step (its Steps 6–7), run once per node, **in streak order**. The order is
not optional: a stacked node is cut on its predecessor's pushed tip, so the predecessor must already
carry **its** contract commit, and a node's tests compile against its dependencies' published
surface, which exists only after **their** commit 2. Streak order is a topological order of the
dependencies, so every predecessor has been through 6g before its successor reaches 6a.

**6a. Cut the branch on its base — from the remote tip, never from a local branch.**

```bash
BASE="$TRUNK"                                   # stack root or standalone node
BASE="feature/<slug>/<predecessor>"             # stacked node: its predecessor in its stack
git fetch origin "$BASE"
git checkout --detach "origin/$BASE"
git checkout -b "feature/<slug>/<node>"
git status --porcelain | grep -v '^?? '          # must be empty
```

Cutting from `origin/$BASE` picks up whatever the predecessor holds **now** — its contract commit,
and, under `/yolo-streak`, any green a pipeline has already pushed — and never pins the trunk or the
predecessor's local branch. If the branch name already exists (a resumed run), **stop and ask**: it
was cut before, and cutting it again would discard what is on it.

**Do not create a worktree per node here.** Two reasons, both from the `pr-stack` skill: a worktree
**pins its branch**, and git refuses to update a branch checked out elsewhere — pass 2 must be able to
check any streak branch out **here** when it revises a link; and a worktree in this repo costs several
GB once `target/` and `node_modules` exist. Per-node worktrees belong to whoever runs `/green` — see
Step 11.

**6b. Commit 1 — the node's planning docs — then push and open the draft PR.**

- Update this node's draft `## Placement` with the base just cut (it may differ from pass 1's
  prediction if 6h moved it), then stage **only** this node's PRD, changeset and discovery companion —
  by path, never the whole-work discovery file, never another node's draft, never anything under
  `src/`. Never use `--no-verify`.
- This commit is the node's plan. 6g adds its contract as a **separate, second** commit; do not amend
  this one later.

```bash
git push -u origin HEAD
gh pr create --draft \
  --base "$BASE" \
  --title "<type>(<scope>): <what this PR delivers> (#<slug> <K>/<N>)" \
  --body-file <planned-body-file>
```

- **`--draft` is not optional here** and it is a human act — nothing in the API layer sets it for
  you. A draft node is *live* (it reads as `open`), not planned.
- **`--base` is the base 6a cut from**: the trunk for a stack root or a standalone node, the
  predecessor's branch for every other stacked node. Getting this wrong misroutes the stack and
  inflates the diff.
- **Write the title as shipped, in this repo's conventional-commit form.** Look at
  `git log --oneline -25` first. The scope is the package short name with `tddy-` dropped,
  comma-joined when a change genuinely spans packages, or a feature area when that reads better. All
  streak metadata goes in **one trailing group** `(#<slug> K/N)`, always last. **Never** put a process
  artifact in a title — `red`, `green`, `stubs`, `failing tests`, `WIP`, `phase 1` — and never leave a
  branch slug as a title. On squash merge the title becomes the commit message on `master`,
  permanently.

  > **The one-commit trap.** This repo squashes with `COMMIT_OR_PR_TITLE`, and for a PR with
  > **exactly one commit** GitHub uses the *commit* subject, not the PR title. Between 6b and 6g the
  > node is a one-commit PR — so keep commit 1's subject identical to the PR title, or pass the title
  > explicitly at merge time.
- Pass the body from a file so newlines and backticks survive. The body may say the PR is at its
  planning stage; the **title** may not.
- **The body carries the streak map for this node**, so an implementer sees it without opening the
  changeset: its label and `K/N`, its unit (stack and position, or standalone), its base, the
  dependency edges that touch it, whether it is greenable now, the PRs it can be greened alongside,
  and the PR it blocks. A reader who has to open five documents to learn whether they can start is a
  reader who starts anyway and finds out the hard way.
- **The body carries this node's `## Progress` section** — the changeset's `## TODO` and `## Scope`
  checklists as 4b left them, between the `changeset-checklist` markers (`/update-context-docs` § 8).
  Every later tick on this node is mirrored there.
- Verify this PR:
  ```bash
  gh pr view --json number,url,isDraft,baseRefName,changedFiles \
    --jq '"#\(.number) draft=\(.isDraft) base=\(.baseRefName) files=\(.changedFiles)"'
  ```
  It must be `draft=true`, based on `$BASE`, and show **only its own files** — at this point its PRD,
  changeset and discovery companion, nothing else. A file count that includes another node's docs
  means the base is wrong.
- **With a work queue**, bind the item to the PR: `work_item_assign_pr { item_id, pull_number }`. The
  PR's head branch becomes the item's branch.
- **Note the number and URL.** 6g writes it into this node's own changeset, and every later node's
  documents refer to it by number.

**6c. Verify build and baseline — MANDATORY.** The branch carries only planning docs plus the
inherited surface, so the build **must** pass:

```bash
cargo build -p <package>                         # per package this node touches
cargo fmt --all --check
cargo clippy -p <package> -- -D warnings
./test -p <package>
./dev bun run cypress:component                  # only the specs of the web package this node touches
```

If it does not pass, **stop** and report the exact failing command. Do not write tests on a broken
baseline, and do not "fix" another node's surface from here. Record pre-existing failures so `/green`
does not mistake them for this node's own red tests. Scope the run to the packages this node touches
— a full-workspace run carries pre-existing noise; say which packages you verified.

**6d. Publish the API surface this node owns.**

- Create the public symbols this node owns with correct signatures and types. This is the
  **`## Draft PR contract`** — the interface of a PR that will implement it in the same PR, not a
  deliverable of its own. Annotate each unimplemented body `// TODO(<node>): implement`.
- The branch must **build and lint clean** on that surface alone (`cargo build -p`,
  `cargo clippy -p … -- -D warnings`) — that is what lets dependents compile against it and run
  `/green` concurrently.
- **Never touch a symbol another node owns**, however small the change looks. If this node needs a
  different signature from a dependency, that is a plan change: stop and raise it with the user. A
  signature change is the owner's to push, on its own branch, immediately.

**6e. Failing acceptance tests — with USER REVIEW.**

Before writing any tests, read `.agents/skills/fluent-tests/references/generic-guidelines.md` and the
framework-specific reference for the test type. Fluent-tests style is mandatory here.

- For each acceptance test in this node's changeset, write a fully implemented test — not a
  placeholder — that fails because **this node's** implementation is missing. Verify each fails for
  that reason.
- Use `mountWithRpc` + `anInMemoryRpcBackend` for Cypress component tests, not `cy.intercept`.
- **Present to the user** the test titles, file paths with line numbers, what each validates, and
  confirmation all are failing. **Wait for approval before 6f** — this gate is per node; it does not
  carry over from the previous one.
- Check off `Create failing acceptance tests`, `Run acceptance tests (verify they fail)` and
  `USER REVIEW — acceptance tests` in this node's changeset once approved, and mirror them into this
  node's PR description (`/update-context-docs` § 8).

**6f. Failing unit/integration tests.**

- Write comprehensive failing tests for **this node's own surface** — main functionality, edge cases,
  error scenarios, API boundaries. Define the public API through test usage.
- **Its tests exercise only what it owns.** A test here must never assert behaviour a **dependent**
  will implement, and must never depend on a dependent being green.
- **Check each test against the dependencies' published surface.** A test that drives a symbol another
  node owns, still marked `TODO(<other-node>)`, cannot pass until that node is green. For each such
  test, either **inject a double** at the seam when the other node is incidental to what the test
  asserts, or **keep the real object** when the integration *is* the point — and record that as a
  **behaviour need** in this node's `## Dependencies` and `## Green wave`. Do not silently leave it: a
  test blocked on another node looks identical to a broken test.
- A quick way to find them — every `TODO(` for a node other than this one that this node's tests
  reach:
  ```bash
  grep -rn "TODO(" packages/<pkg>/src | grep -v "TODO(<this-node>)"
  ```
- Verify: the build passes on the published surface, tests fail correctly, **and each failure is
  attributable to this node's own missing implementation** rather than another node's. Check off
  `TDD Red — write failing unit/integration tests` in the changeset, and mirror it into the PR
  description (`/update-context-docs` § 8).
- If this step needed more exploration, append it to this node's `{slug}-initial-discovery.md` as a
  new `## Exploration N` section.

**6g. Commit 2 — the draft-PR contract — and push.**

- Update this node's changeset: the TODO check-offs from 6e/6f, its PR number and URL, the PR numbers
  of the nodes it names, any `## Dependencies`, `## Placement` or `## Green wave` refinement from 6f,
  and the `**Complexity**` line re-scored if 6e/6f showed the node is bigger or smaller than planned.
  **Own files only** — never another node's changeset or PRD.
- Commit surface + tests + these doc updates as **one commit**, the node's second. Never amend commit
  1. If something must change after this commit is pushed, add a third commit — never amend, never
  `--no-verify`.
- Push **this branch only**: `git push origin HEAD`.
- Verify the two-commit shape and the diff scope:
  ```bash
  git log --oneline "origin/$BASE..HEAD"                        # exactly TWO commits: plan, contract
  gh pr view --json number,isDraft,baseRefName,changedFiles \
    --jq '"#\(.number) draft=\(.isDraft) base=\(.baseRefName) files=\(.changedFiles)"'
  ```
  Still `draft=true`, still based on `$BASE`, and `changedFiles` covering only this node's docs,
  surface and tests.

**6h. Revise the links — MANDATORY after every contract push.** The node now has real signatures and
real tests, and they are the evidence its links were predicted from. Compare them with this node's
`## Dependencies`, `## Placement` and `## Green wave`, and with the drafts of the nodes still to come:

| What the code showed | Before the affected stack is registered | After it is registered |
|---|---|---|
| A planned dependency is not needed — nothing of the other node is compiled against or exercised | Drop the edge. If this node now has no dependency and no dependent, it becomes **standalone**: rebase its two commits onto the trunk (`git rebase --onto "origin/$TRUNK" "origin/$BASE"`), push with lease, retarget its PR base (`gh api -X PATCH repos/{owner}/{repo}/pulls/<N> -f base=$TRUNK`) | `pr-stack` § *Reordering a registered stack* — ask first, it unstacks |
| A behaviour need is only a surface need (the test injects a double after all) | Move the node to its earlier wave; re-sort its stack if the waves now step back | the same, with the reorder procedure |
| A new dependency on a node **already below it** in its stack | Record it; no git change. Re-check the waves | the same |
| A new dependency on a **standalone** node, or on a node in **another stack** | Placement rule 1 is broken. **Stop and present the options**: join the units (the dependency becomes this node's ancestor — rebuild the affected branches in the new order), take this node out of the streak until its dependency lands, or remove the dependency (a double, a narrower test) | the same, and joining stacks also needs the unstack — ask first |
| A dependency for a node **not yet cut** changes | Correct that node's **draft** now — it is untracked and has no branch, so this is cheap | — |

- **Apply what changed to every record that holds it**: this node's changeset (a third commit if 6g
  was already pushed), the queue (`work_item_set_parents`, `work_queue_set_dependencies`), the PR body,
  and the drafts of the nodes not yet cut. A node that is **already cut** has its documents on its own
  branch: correct them there, as a commit on that branch, in this worktree — never from another
  node's branch — and say so in the report. With a queue, note that once an item owns a branch,
  `work_item_set_parents` also realigns it (rebase onto the new base, push with lease, retarget the
  PR): either let it make the move with the branch checked out nowhere, or make the move by hand
  first — never both blind; read its result.
- **Re-check the waves after every revision.** A new behaviour need can push a node into a later wave
  than a node above it in its stack. The stack then steps back a wave, which Step 1 question 8
  forbids. Record the new edge in both nodes' `## Green wave`, then **stop and reorder before moving
  on**: move the node up to its new wave's place, sorted blockers-first, relabel, and retitle. Fixing it
  now costs less than at Step 8b. Validity alone ("its dependencies are still below it") is **not** a
  reason to keep the order.
- **Say so in the report when this push unblocks a node** whose document records it as waiting. Under
  `/yolo-streak`, a dispatch is waiting on exactly that.

### Step 7: Register each stack and settle every title — MANDATORY

Run this once every member of a stack has its contract commit — for the whole streak at the end of
pass 2, or stack by stack as each one completes.

1. **Confirm the shape** — GitHub will not do this for you, so read the bases:
   ```bash
   gh pr list --state open --json number,title,headRefName,baseRefName \
     --jq '.[] | select(.headRefName | startswith("feature/<slug>/")) |
           "#\(.number) \(.headRefName) → \(.baseRefName)  \(.title)"'
   ```
   Every stack root and every standalone node is based on `$TRUNK`; every other stacked node is based
   on its predecessor's branch. Nothing is based on a streak branch that is not its predecessor.
2. **Register each stack — MANDATORY for every stack, never for a standalone node.** Bottom to top:

   ```bash
   gh stack link --base "$TRUNK" <pr-1> <pr-2> ... <pr-m>     # one call per stack
   ```

   Two traps: it defaults to `--base main`, so pass `$TRUNK` explicitly; and **never `--open`**,
   which would mark every draft ready for review before any of their docs are wrapped — readying is
   always *wrap the docs, then mark ready*, one node at a time through its own `/pr-wrap`. `link`
   reuses the PRs that already exist and never removes one, so re-running it with the full list is
   also how you extend a stack later. `gh stack view` reads *local* tracking and will say "not part
   of a stack" after a `link` — that is expected, not a failure; the stack lives on GitHub. A
   one-node "stack" is not registered: a node with no stacked dependency or dependent is standalone.
3. **Revise every PR title — MANDATORY.** Each was set in its own 6b, while later nodes were still
   predictions: `N` may have moved and labels may have been renumbered by 6h. A stale `2/6` on a
   seven-node streak is worse than no number, because it is read as fact. Read them all now, as one
   list, and check every one against **PR titles** in the `pr-stack` skill:

   - format `<type>(<scope>): <what it delivers> (#<slug> K/N)`;
   - the **same slug** on every node of the streak — a typo makes one PR look like a different streak;
   - `K` is the node's current label, `N` the **final** node count, and along each stack the `K`s
     ascend;
   - the subject states the delivery **as shipped**;
   - length is a readability convention here, not a hook — put the load-bearing words first.

   Fix every one that is wrong, in the same run, with plain `gh pr edit <N> --title "…"`.
4. **Claim the code issues** this streak fixes (Step 2b): `**Claimed by:** #NNN — `#<slug> K/N`` and
   `**Lands after:**` on each record, now that every number exists — as part of the claiming node's
   own changeset commit, never from another node's branch.

## Completion gate

### Step 8: Restore the original branch — MANDATORY

Every node now carries its plan and its contract. This worktree's job as planner is done; do not leave
it sitting on a streak branch, which would pin it.

1. Confirm a clean tree and that the current branch is pushed:
   ```bash
   git status --porcelain | grep -v '^?? '                            # must be empty
   git rev-list --count "origin/$(git branch --show-current)..HEAD"   # must be 0
   ```
2. **Restore the starting branch — including `master`:**
   ```bash
   git checkout "$ORIGINAL_BRANCH"
   git branch --show-current   # must equal ORIGINAL_BRANCH
   ```
   Leaving this worktree on `origin/master` (detached), on a streak branch, or anywhere else is a
   defect — it pins that branch and is not a restore. The 6a detached `origin/$BASE` checkout is only
   for cutting a branch; it is not the restored state.
3. **No pass-1 draft may be left behind.** `git status --porcelain docs/dev/1-WIP/` must show none of
   this streak's drafts: each was committed on its node's branch. A leftover means a node was never
   cut — say which and why.

   **Do not delete this worktree.** **Never delete a streak branch** — a stacked one is a successor's
   base, and deleting it **closes** that successor's PR, which cannot then be reopened or re-based via
   the API.

### Step 8b: Validate the links and each stack's wave order — MANDATORY

Recompute every node's wave from the **final** behaviour needs — the `## Green wave` sections as
pass 2 left them, not the pass-1 guess — and re-check the placement rules against the final
`## Dependencies`. Then read each stack's waves off its line, bottom to top:

```
stack auth-store   K     1 2 4 5 7
                   wave  1 1 2 1 2     ✗ steps back at K=5: wave 1 above a wave-2 node
```

**Each stack's run must never decrease**, every dependency must sit below its dependent in one stack,
and no standalone node may have a dependency or a dependent. If any check fails, the streak is **not**
complete, even though every branch still builds. Fix it before Step 9:

1. Derive the corrected units and lines (Step 1 questions 3 and 8). Show the user the old and the new
   map together, with the edge that moved each node.
2. Rebuild the branches in the new order, cutting each one from its new predecessor's tip and
   replaying its own commits. For a registered stack follow the `pr-stack` skill § *Reordering a
   registered stack*: first `gh stack unstack` (**ask first**), then repoint each PR's base with
   `gh api -X PATCH`, and only after that force-push the rebuilt branches with lease. If you push
   before repointing, a PR can auto-merge into its old base.
3. **Renumber** every PR whose `K` changed — title, each commit subject and body, PR body and
   discovery header that carries `#<slug> K/N`. **Relabel** every node as well, so each `n<k>` in the
   changesets and discovery files is its new position. Update the queue. Then re-run
   `gh stack link --base "$TRUNK"` for each stack in its new order.
4. Re-run this check. It must pass before Step 9 runs.

### Step 9: Cascade `/pr-stack-rebase` over each stack — MANDATORY

With this worktree on `$ORIGINAL_BRANCH` and every streak branch free, run one cascade **per stack**
from here:

```
/pr-stack-rebase from #1 to #<m>     # positions within that stack, bottom-up
```

Say explicitly that `#1..#m` are **positions in that stack**, not PR numbers and not streak labels.
Cascade mode borrows this worktree for each layer (nothing pins the branches), rebases bottom-up
using each layer's **recorded pre-rebase tip** with `--onto`, and **restores `$ORIGINAL_BRANCH`** when
it finishes or stops. A standalone node has no layer to cascade: it was cut from a fresh trunk tip in
6a, and a trunk that moved since is not staleness to fix here.

**Expected result: every layer reports *already current, leak-free, no rewrite*.** Pass 2 cut each
stacked node from its predecessor's tip right after the predecessor's contract commit, so the cascade
is the proof, not the fix. Read its report per layer:

- **all current** → the stack is coherent.
- **a layer was stale and got rebased** → something moved after it was cut (a 6h revision, a third
  commit below it). The cascade brought it and everything above it current; note in Step 10 which
  layer and why. Report its build/test verdict, never assume it.
- **the cascade stopped** (conflict, pinned worktree with unpushed work, unverifiable build) → the
  streak is **not** complete. Report which layers were done, which stopped, and which were not
  attempted, and do not hand that stack off to `/green`.

Then confirm the restore held: `git branch --show-current` must equal `ORIGINAL_BRANCH`.

### Step 10: Present the Streak

Present a complete summary:

- **The streak map** — every unit: each stack's line bottom to top with labels, PR numbers and the
  `gh stack link` output confirming it is registered, and every standalone PR with its trunk base.
  Note explicitly that GitHub enforces no merge order inside a stack and restacks nothing after a
  merge, registered or not, and that standalone PRs may land in any order.
- **How the links moved in pass 2** — every 6h revision: the edge, the evidence (a test, a
  signature), what moved. "No link changed" is stated as a finding, with the evidence it rests on.
- **The final title of every PR**, as one list, showing the shared slug and each node's `K/N`. If
  anything shifted scope after Step 7, fix the affected title with `gh pr edit` and report the
  corrected list.
- **The Step 8b check**: each stack's wave run, bottom to top, shown as non-decreasing, plus any
  reorder it forced and the edge behind it.
- **The Step 9 cascade verdict per stack and per layer** — already current or rebased, and each
  layer's build/test state.
- **This worktree is back on `$ORIGINAL_BRANCH`** (confirm `git branch --show-current`). `/green` on
  these PRs is unblocked now that the completion gate has run.
- Per node: label, number + URL, unit, branch, base, the **two commits** (plan, contract), PRD,
  changeset, discovery companion, owned surface, failing acceptance + unit/integration test titles
  and paths, and confirmation the build passes on the published surface while tests fail for this
  node's own missing implementation.
- **The green waves** — wave 1 (everything greenable now, concurrently, standalone nodes included),
  then each later wave with the behaviour it is waiting on. Say plainly that a wave cuts across units:
  every node in it can be greened at once. Show that the streak order **is** the waves concatenated
  with blockers leading each one, naming each wave-leading node's transitive-dependent count. Name the
  per-node sequencing facts recorded in `## Dependencies`.
- **What concurrency costs**: every `/green` on a stacked node adds commits, so a wave of N concurrent
  greens inside one stack is N cascades of `/pr-stack-rebase`; standalone nodes cost no cascade. Each
  concurrent node needs its own worktree (several GB here). Offer the smaller-batch trade rather than
  leaving it to be discovered.
- **Where a stack was flattened** — any pair that is only sequential because `gh stack` needs a line,
  so a reviewer knows those two could have gone in parallel — and any subtree taken out of the streak,
  with the `docs/dev/todo/` entry that records what releases it.
- Anything you could not prove locally (server-side checks, org policy), stated as a risk.

### Step 11: Hand off to `/green`

**Do not create per-node worktrees from this command**, and do not check a streak branch out here
again after the restore — that would pin it. Each branch is already on the remote with an open draft
PR; this worktree stays on `$ORIGINAL_BRANCH`. Whoever picks up a node creates a **different**
checkout:

```bash
git fetch origin "feature/<slug>/<node>"
git worktree add <path> "feature/<slug>/<node>"
cd <path> && ./dev bun run local-registry-install     # plus a cargo build to warm target/
```

Start only the nodes whose behaviour needs have been delivered; the rest wait. Standalone nodes and
wave-1 stack nodes can all start at once. Each changeset's `## Dependencies` tells its implementer
which symbols to leave alone, and `/green` stops and asks if a capability this node consumes does not
exist at `HEAD`.

**In a stacked node's worktree, the loop is the `pr-stack` in-stack loop:**

```
/green            → ALWAYS /pr-stack-rebase first, then the dependency gate;
                    then implement, commit + push this branch
/validate-changes → ALWAYS /pr-stack-rebase first (no code diff until leak-free);
                    then implementation vs plan; commit + push the status; then
                      gaps remain → /green again → /validate-changes
                      no gaps     → /pr-wrap
/pr-wrap          → ALWAYS /pr-stack-rebase first (again), then wrap, correct the title,
                    mark the PR ready for review
```

**A standalone node's loop is the ordinary single-PR one** — `/green` → `/validate-changes` →
`/pr-wrap` — on a branch based on the trunk; it is not a stack member, and nothing above it is
waiting on its pushes.

Both `/green` and `/validate-changes` push, because a stacked branch is a successor's base — unpushed
work is invisible to everyone above you. **Push at every milestone**, not only at the end of `/green`:
a milestone is production code whose own tests already pass. A **signature change is the urgent one**
— push it on its own, immediately, ahead of the implementation behind it.

**Ready each node on its own gate.** Readying a PR means **wrapping its docs, then marking it ready
for review** — never the mark alone — and each node wraps and is readied by its own `/pr-wrap` when
its planned scope is done, independent of the state of the nodes it depends on. Nodes link to each
other by branch/PR number only, so wrap order never leaves a dangling reference. *Landing* is the part
that stays bottom-up inside each stack.

**To pick up a moved base, each stacked worktree rebases itself with `/pr-stack-rebase`** (single
mode) — bottom-up, one branch at a time. Never `git rebase --update-refs` from a per-node worktree.

**Each `/green` worktree is temporary.** Remove it once that node's loop is finished — `/green` pushed,
`/validate-changes` clean, `/pr-wrap` done — unless the user asked to keep it. Verify nothing is
unpushed first, then `git worktree remove --force <path> && git worktree prune`. **Never delete the
branch.**

Landing: each stack with **`/merge-pr-stack`** — bottom-up, one PR at a time, with the whole-stack
comment sweep first, then the local-gated fix pass, then the CI gate and the `#automerge` squash gate,
then the repoint that nothing does for you once a parent lands. Each standalone PR lands on its own,
like any single PR. `/merge` and `/repoint` are the by-hand fallback.

> **Never arm `#automerge` on a stacked node that is not its stack's root.** It merges a PR into
> **its own base**, which for a stacked PR is its predecessor's branch — folding that node's work into
> its predecessor's PR. Arm it only once a node's base is `master`.

## Out-of-Scope Ideas

During planning and code analysis, if you identify enhancements outside the current streak's scope,
add **a new file** to `docs/dev/todo/` — `YYYY-MM-DD-<slug>.md`, `**Category:** Future enhancement`,
`**Source:**` the streak slug. Never append to an existing file.

`docs/dev/todo/` is read as well as written: Step 2b scans it for items this streak runs into and
Step 4b records them per node. The two directions are complementary — what this streak defers is what
somebody's Step 2b finds next, so write entries stating **why** the work was deferred, not only what
remains. That reason is what tells the next planner whether it blocks them.

**And it is read once more before this streak lands.** Every entry written here is re-examined at
close — `/pr-wrap` on the top node of each stack and `/merge-pr-stack` 1e ask whether it could be
closed by one extra node while the context is still loaded (`pr-stack` § *The backlog delta a stack
leaves*). A stated reason is what that sweep judges, and the reason worth stating precisely is the one
a later node in this same streak might invalidate: "deferred because there is no seam for it" becomes
actionable the moment some node builds the seam.

## Rules

- **A streak is a set, not a stack.** Join nodes into a stack only where they depend on each other;
  every other node is standalone, based on the trunk, and registered in no stack.
- **Every unmerged dependency is an ancestor.** A node that depends on another sits above it in the
  same stack. A stack is never based on another stack's branch.
- **One connected group, one linear stack.** Flatten a branching group and say what it cost, or take
  a subtree out of the streak and record what releases it.
- **Pass 1 plans the links; pass 2 makes them real and revises them.** Pass 1 creates no branch, no
  commit and no PR — it drafts the node docs and records the nodes and links in the work queue when
  there is one. Pass 2 cuts the branches, opens the PRs, registers the stacks, and applies every link
  the real code disproves (6h). Expect it to.
- **Register every stack** with `gh stack link --base "$TRUNK" <prs…>` once its members all have their
  contract commits (Step 7); never register a standalone node or a one-node stack. Do not run
  `gh stack sync` / `gh stack rebase` — `/pr-stack-rebase` owns rewriting branches here.
- **Order the streak from the waves, blockers first.** The streak order is the green waves
  concatenated, and inside each wave a node others depend on comes ahead of one nothing waits on.
  Each stack's line is that order restricted to its members.
- **Labels are streak positions; branch names carry none.** `n<k>` is the `k`-th node in streak order,
  and `K/N` in every title is that label. Relabel on every reorder. `<node>` in
  `feature/<slug>/<node>` and in worktree names is a descriptive name, never a number.
- **A stack never returns to an earlier wave.** Read bottom to top, its wave numbers never decrease.
  This holds at planning (Step 1), whenever pass 2 finds a new edge (6h), and at the completion gate
  (Step 8b).
- **Refuse a detached start.** `ORIGINAL_BRANCH` must be a named branch.
- **Cross-check `docs/dev/todo/` (Step 2b) before decomposing.** Record every relevant item in the
  affected node's `## Prerequisites` with a verdict **and a link to its file**; give a blocking one a
  `## Scope` line; give a large one its own node or its own PR. Never absorb a large refactor into a
  feature node. One claiming node per entry.
- **The boundary contract governs the decomposition.** Every node is a vertical slice — schema, code,
  tests, in one PR. Splitting by layer is forbidden; a node that ships only surface is not a valid
  node. Split by capability instead. Only the two named exceptions apply, and do not invent a third.
- **Pass 2 is a draft-PR contract, not a stubs phase.** What it publishes is the first push of a PR
  that goes on to implement the same thing. **A node must never merge in that state.**
- **Two commits per node, both in pass 2.** Commit 1 is the node's docs, commit 2 its surface + failing
  tests + changeset check-offs. Never amend; a correction after commit 2 is pushed is a third commit.
- **Cut every branch from the remote tip of its base** (6a), in streak order, so a stacked node always
  starts on its predecessor's contract — or on whatever green a pipeline has pushed since.
- **Nobody pins a streak branch until Step 9 has run.** If a branch is pinned, stop and ask; do not
  work around it with a second worktree.
- **Branch names are `feature/<streak-slug>/<node>`**, one namespace for the whole streak.
- **Plan in this worktree.** Do not create per-node worktrees during planning; they belong to `/green`.
- **Restore `$ORIGINAL_BRANCH` when pass 2 is done**, including when that is `master`, and **then run
  the per-stack cascade `/pr-stack-rebase`** as the completion gate. Do not delete this worktree.
  **Never delete a streak branch** — it closes a dependent PR, irrecoverably.
- **No `docs/dev/1-WIP/STACK-*.md` or `STREAK-*.md`.** Each node's context lives in its own changeset;
  the whole graph lives in the work queue. Never record another node's status.
- **All six changeset headings, on every node** — `## Responsibility`, `## Boundaries`,
  `## Placement`, `## Dependencies`, `## Draft PR contract`, `## Green wave`.
- **Never edit another node's changeset or PRD from a different node's branch.** A correction to an
  already-cut node is a commit on that node's own branch.
- **Never implement a symbol another node owns**, however small it looks. Its owner's tests specify
  it; if the surface is wrong for you, say so to its owner.
- **Title every PR at 6b, then revise all of them in Step 7.** A title is not planning output — it is
  the commit message that lands on `master`. Watch the one-commit squash trap. Use plain `gh pr edit`.
- **Renumber the whole streak whenever its shape changes** — adding a node changes `N` for every PR.
  Do not renumber a merged PR.
- **Each node's tests exercise only what it owns** — never a dependent's behaviour, never dependent on
  a dependent being green.
- **Give each node a distinct changelog/changeset slug** — several nodes of one streak land on the
  same date, and each adds its own file. Never edit another node's entry.
- **Ask before deleting** any file, including the whole-work discovery source (CLAUDE.md).
- Tests fully implemented, fluent-tests style, no conditional logic, no fallbacks, no try/catch to
  swallow errors.
- Never put "red phase" / "green phase" in code or test descriptions.
- Never use `--no-verify`, on a commit or a push.
- Verify per package and say which packages you verified — a full-workspace run carries pre-existing
  noise.

## Flow

```
/plan-streak
  record ORIGINAL_BRANCH (stop if detached) and TRUNK
  interview → decompose into vertical slices → real edges (behaviour needs marked)
  → UNITS: connected groups → one linear stack each; unconnected nodes → standalone
  → GREEN WAVES → streak order = waves concatenated, blockers leading each wave
  → label n1..nN; each stack's line = the streak order restricted to it
  → user approves the streak map
  → whole-work discovery → cross-check docs/dev/todo/ + code-issues
  → settle order, units, slug, owned surfaces, per-node draft-PR contracts
  → PASS 1 — decomposition (no branch, no commit, no PR)
        per node: PRD + changeset (6 headings incl. ## Placement) + discovery, as untracked drafts
        work queue (if owned): work_item_add, work_item_set_parents (base), work_queue_set_dependencies (needs)
        → work_queue_waves / work_queue_graph agree with the plan
  → checkpoint: placement rules hold, nothing cut, delete whole-work discovery (ask), report
  → PASS 2 — contracts and compilation (per node, in streak order, in this worktree)
        cut feature/<slug>/<node> from origin/<base>   (trunk for a root or standalone)
        commit 1 (its docs) → push → gh pr create --draft --base <base> → work_item_assign_pr
        build + baseline → owned API surface → failing acceptance tests (USER REVIEW)
        → failing unit/integration tests → commit 2 → push
        REVISE THE LINKS: what the code showed → placement, dependencies, waves, queue, drafts
  → per stack: gh stack link --base $TRUNK <prs…>; revise every title; claim code issues
  → git checkout ORIGINAL_BRANCH   # if that was master, this worktree is on master again
  → links + per-stack wave check, else reorder + renumber + relabel + re-link
  → /pr-stack-rebase from #1 to #m, per stack   # completion gate: every layer already current
  → (per node, in the implementer's own worktree, when its behaviour needs are delivered)
        stacked:    /green → /validate-changes → /pr-wrap, each /pr-stack-rebase first
        standalone: /green → /validate-changes → /pr-wrap, as a single PR
  → per stack: /merge-pr-stack; per standalone PR: its own merge
```

## Related

**Commands**: `/yolo-streak` (this command, with each ready node implemented while pass 2 runs),
`/plan-red` (its Steps 6–7 are what pass 2 runs per node), `/add-to-pr-stack` (a node on top of an
**existing** stack tail), `/split-pr-to-stack` (carve an **existing** PR into stacked slices; this
command plans from requirements), `/split-branch` (sibling branches from existing work),
`/follow-up-branch`, `/green`, `/validate-changes`, `/pr-stack-rebase` (cascade mode per stack as the
completion gate), `/merge-pr-stack`, `/merge`, `/repoint`, `/pr-wrap`, `/wrap-context-docs`
**Skills**: `pr-stack` (`.agents/skills/pr-stack/SKILL.md` — the model for each stack in the streak),
`orchestrate-work` (the queue model — stack, siblings or DAG — that `/yolo-streak` drives)
**References**: `.agents/skills/planning/references/planning-phase.md`,
`.agents/skills/planning/references/initial-discovery.md`,
`.agents/skills/fluent-tests/references/generic-guidelines.md`
**Feature docs**: [`docs/ft/coder/pr-stacking.md`](../../docs/ft/coder/pr-stacking.md) — the work
queue's items, `parents`, `needs`, waves and readiness, which Step 4d records into
