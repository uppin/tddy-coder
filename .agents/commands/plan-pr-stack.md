---
description: Plan a stack of dependent PRs by hand, in two waves - interview and decompose, open every draft PR with its PRD, changeset and discovery (commit 1), then revisit each node bottom-up for its draft-PR contract - owned API surface and failing tests (commit 2) - and finish with a cascade /pr-stack-rebase
---
## Plan PR Stack — From Requirements to a Stack of Draft PRs, in Two Waves

Alternative to `/plan-red` when the work is large enough to split across **multiple PRs planned
ahead**. Interview the user, decompose the work into a **linear sequence** of self-contained PRs,
then build the stack in **two waves**, both in the **current worktree**, one branch at a time.

Load the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`) first — it defines the stack model,
**registering the stack with `gh stack`**, the **PR boundary contract**, the `## Draft PR contract`,
base tracking, forward-only doc linking, the worktree-pinning constraint, PR titles, and the landing
rules. This command assumes those definitions and never contradicts them.

> **Not the `pr-stack` workflow recipe.** `tddy-coder` ships a product feature by a similar name —
> a workflow recipe (`pr-stack`, legacy aliases `plan-pr-stack` / `orchestrate-pr-stack`) that plans
> and drives a stack from inside a coding session, with its own state and its own tooling. That is a
> **separate implementation**, shipped to whatever repository a user runs tddy on, and documented in
> the `pr-stack` skill. This command is how *this*
> repository's contributors plan a stack by hand. Neither describes the other.

### The two waves

- **Wave 1 — plan every node (commit 1 of each).** For each node in dependency order: cut its branch
  off its parent, write its own PRD + changeset + initial discovery, commit those **docs only**,
  push, and open the **draft PR immediately** against its parent's branch. When wave 1 ends, every
  node of the stack is a draft PR on GitHub with a one-commit planning branch.
- **Wave 2 — publish each node's draft-PR contract, bottom-up (commit 2 of each).** Revisit n₁ → nₙ.
  For each: check it out, `/pr-stack-rebase` it onto its now-moved base, then land what its
  `## Draft PR contract` promised — **the API surface this node owns plus its failing tests** — as
  the node's **second commit**. Push. The next node's rebase picks that surface up, which is what
  lets it compile against a real signature.
- **Completion gate.** When wave 2 ends, restore the starting branch and run a **cascade
  `/pr-stack-rebase` over the whole stack** (`from #1 to #n`). Every layer is expected to come back
  *already current, leak-free*; anything else is brought current before the stack is handed off.

> **Wave 2 is not a stubs phase, and no node's deliverable is stubs.** The boundary contract is
> absolute here: *a node that ships only surface is not a valid PR*. What wave 2 publishes is the
> **first push of a PR that will go on to implement the thing** — the interface plus its failing
> tests, early, so dependents can branch off a real ref. `/green` finishes each node in the same PR.
> **A node must never merge in its wave-2 state.** If you catch yourself planning a node whose whole
> job is to declare surface for a later node to implement, that is a layer split — go back to Step 1
> and cut by capability instead.

Another agent may pick up `/green` on any node **only after the completion gate** — wave 2 rewrites
parents and rebases children, and a branch pinned in another worktree cannot be checked out here.
After the gate, **this worktree is back on the branch it started on** — if that was `master`, it must
be on `master` again (a named branch checkout, not detached `origin/master`, not a stack branch).

**Prerequisites**
- `gh` CLI authenticated (`gh auth status`).
- The user has described a body of work that plausibly spans more than one PR.

- The `gh stack` extension available (`gh extension list | grep gh-stack`).

> **The stack must end up registered on GitHub.** Base refs alone give correct per-PR diffs and
> nothing else — no grouping, no ordering, no stack view for reviewers. Step 5 registers it with
> `gh stack link`. Because `gh stack` models a **line**, the decomposition must be linear: see the
> `pr-stack` skill § *A registered stack is linear*. Do not run `gh stack sync` / `gh stack rebase`
> here — they rewrite branches other worktrees may own; `/pr-stack-rebase` is the tool for that.

## Execution Flow

**Record the starting branch before anything else.** This worktree is the planning worktree — do
**not** create a second one — and it must be restored when the stack is complete, **including when
that starting branch is `master`**:

```bash
ORIGINAL_BRANCH=$(git branch --show-current)
TRUNK=$(git symbolic-ref --short refs/remotes/origin/HEAD 2>/dev/null | sed 's|^origin/||')
TRUNK=${TRUNK:-master}   # override after the interview if the user named a different trunk
git status --porcelain | grep -v '^?? '   # must be empty: no tracked-file changes
```

If `$ORIGINAL_BRANCH` is **empty**, HEAD is detached. **Stop.** A detached start cannot be restored
to a named branch in Step 7, and the worktree would be left on the top stack branch, pinning it. Ask
the user to check out a named branch first — usually `master`.

If the tree has tracked-file changes, **stop** and ask the user to commit them (never bare
`git stash` — see CLAUDE.md on the shared stash stack). If `ORIGINAL_BRANCH` is already a stack
branch from a previous attempt, confirm with the user before continuing.

### Step 1: Interview — MANDATORY

Run the interview from `.agents/skills/planning/references/planning-phase.md` **Step 1**
(What / Why / Where / Constraints / UX / Scope). Then ask the **stack-specific** questions:

1. **Decomposition** — What are the natural, independently-shippable slices? Each must be a
   **vertical slice**: the API/schema change, the code implementing it, and its tests in **one**
   node.
2. **Dependencies** — For each node, which *earlier nodes* must have merged before it can? Record
   the real edges, then choose a **linear order that is a valid topological sort of them**. Where the
   logical graph branches, flatten it and say what that costs — the flattened siblings can no longer
   be worked or landed in parallel.
3. **Ownership** — Which node owns which API surface / files? Each symbol has exactly one owning
   node, and no node ever implements a symbol another owns.
4. **Stack size** — How many nodes? Prefer the fewest that keep each independently reviewable and
   independently mergeable.
5. **Sequencing facts** — For each node, does it consume a capability a parent has not shipped yet?
   That is a scheduling fact to record in its `## Dependencies`, **not** a licence to build the
   parent's half.

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

Present the proposed **sequence** (n₁…nₙ with one-line scope + owned surface each), the real
dependency edges behind it, and anywhere the order was forced by flattening rather than by a real
dependency. **Wait for user approval of the decomposition before creating anything.** If the user
named a trunk other than the detected one, set `TRUNK` to it.

### Step 2: Analyze Existing Code

Pick a **whole-work discovery slug** before launching any exploration — there is no per-node slug
yet, but the dump must exist as soon as the first pass returns, so a rejected PRD or a stop before
Step 4b cannot lose it:

```
docs/dev/1-WIP/{YYYY-MM-DD}-{stack-work-name}-whole-work-initial-discovery.md
```

`{stack-work-name}` is a short kebab name for the whole stack (from the interview), not a node slug.
Create that file **before** the first Explore / Grep / Glob / Read, then persist each pass into it per
`.agents/skills/planning/references/initial-discovery.md` (combined conclusions at the top; each pass
as `## Exploration N` at the tail).

Then run `.agents/skills/planning/references/planning-phase.md` **Step 2** (code analysis → State A)
and **Step 3** (product area), once for the whole body of work. Record which packages each node
touches.

This whole-work file is **temporary**. It is **not** a changeset companion, and wrap of any node must
not delete it as if it were. **Do not `git add` it onto a stack branch.** When each node's changeset
is created in Step 4b, **copy** the whole-work dump into **that node's**
`{slug}-initial-discovery.md` as Exploration 1 (full dump, not a pointer), and append any
node-specific follow-up as later Exploration sections — in wave 1 while writing the changeset, and
again in wave 2 (Step 6) if the contract needs more exploration. After every node has its own
companion, **delete** the whole-work source at the wave-1 checkpoint (Step 5) — **ask the user
first**, per CLAUDE.md. If planning stops before all 4b companions exist, **keep** it.

### Step 3: Settle the plan — no stack document is committed

**There is no `docs/dev/1-WIP/STACK-*.md`, and you must not create one.** The `pr-stack` skill rules
it out for three reasons — it would outlive every PR and so could never be wrapped, every node would
edit the same file and every rebase would conflict on it, and its status column would be a stale
hand-copy of what `gh pr list` already knows. Each node's stack context lives in **its own changeset**
in Step 4b, so nothing shared is created here and nothing has to be handed between branches later.

What this step produces is agreement, not a file. Settle and write down for your own use:

- the **order** — each node's branch and its predecessor's branch, which is the base its PR opens against;
- **`TRUNK`**;
- the **stack slug** — one or two kebab-case words, associative, chosen **once and never changed**.
  It is both the branch namespace `feature/<stack-slug>/<node>` and the PR-title group
  `(#<slug> K/N)`, so one word identifies the stack in a branch list and in a PR list without a
  lookup. `attach-docs`, `sandbox-split`, `base-sync` are good; `stack-3`, `daemon-refactor`,
  `phase-two` are not. See **PR titles** in the `pr-stack` skill;
- the **owned API surface** per node (this becomes each changeset's `## Responsibility` and
  `## Boundaries`);
- the **draft-PR contract** per node — what surface plus failing tests wave 2 publishes first;
- the **reading order** `K` used in titles (`display_order`), and separately the **real edges**
  (`parents`), which are what determine merge order.

Branch names are `feature/<stack-slug>/<node>` — every node sharing one namespace. Do not invent
`pr-2-<slug>`; that shape belongs to a different repo.

## Wave 1 — plan every node and open its draft PR (commit 1 of each)

### Step 4: For each node in dependency order (n₁ → nₙ) — branch, docs, push, open the PR

Process nodes in an order where every parent comes before its children (topological). Stay in **this
worktree**. After each node is on GitHub, move to the next branch — do not keep a checkout of a
branch that is already pushed. **Wave 1 writes docs only** — no `src/`, no tests. Those come in wave 2.

**4a. Create the node's branch in this worktree and check it out.**

- **Root node** (empty `parents`) — cut from `$TRUNK` without pinning the local trunk branch, since
  the primary clone usually has it checked out:
  ```bash
  git fetch origin "$TRUNK"
  git checkout --detach "origin/$TRUNK"
  git checkout -b "feature/<slug>/<node>"
  ```
- **Every other node** — while still on its predecessor's branch:
  ```bash
  git checkout -b "feature/<slug>/<node>"
  ```
  There is no diamond case: the plan is a line, so each node has exactly one predecessor. A node that
  consumes something from *two* earlier PRs simply sits after both of them — both are ancestors, and
  `## Dependencies` records what it takes from each.
- **Do not create a worktree per node here.** Two reasons, both from the `pr-stack` skill: a worktree
  **pins its branch**, and git refuses to update a branch checked out elsewhere — wave 2 must be able
  to check every branch out **here**; and a worktree in this repo costs several GB once `target/` and
  `node_modules` exist. The branch is on the remote as soon as 4d finishes, and recreating a worktree
  later is cheap. Per-node worktrees belong to whoever runs `/green` — see Step 10.

**4b. Create this node's PRD + changeset + initial discovery.**

- Follow `.agents/skills/planning/references/planning-phase.md` **Step 4** (PRD) and **Step 5**
  (changeset), scoped to **this node only**. Present each PRD for approval before its changeset.
- **Every changeset MUST carry the four headings** the stack model requires, in every node's
  document:

  ```
  ## Responsibility        what this PR owns
  ## Boundaries            what it explicitly does not do
  ## Dependencies          per parent node: what that PR delivers that this one consumes
  ## Draft PR contract     what lands first (API + failing tests) to unblock dependents
  ```

  A root node still carries all four; its `## Dependencies` says it has none.
- **`## Dependencies` is the duplicate-development guard.** Whoever runs `/green` on this node has
  never seen the plan's reasoning and has no other way to learn that a parent is already adding the
  trait they are about to add. State it **per parent node**, in a table:

  ```markdown
  ## Dependencies

  What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
  implementing one here collides with the PR that owns it.

  | Parent node | What it delivers | How this PR consumes it | This PR does NOT |
  |---|---|---|---|
  | `n1` token-store | `TokenStore::{put,get}` in `packages/tddy-github/src/token_store.rs`, persisted | middleware calls `get` on every request | add persistence, change the trait, or widen the key type |
  ```

  Describing inherited state in *State A* is not a substitute — State A says what exists, this says
  what the implementer must **not** build.
- **`## Draft PR contract`** states what wave 2 will publish for this node: the owned API surface plus
  the failing tests that specify it. Write it as *the first push of this PR*, never as this PR's
  deliverable.
- **Forward-only doc linking, own files only.** A parent's PRD/changeset MAY link forward to its
  children's (`## Successor PRs`, naming the child **branches**). A child's MUST NOT link back — the
  parent is wrapped and removed from `1-WIP` first, so a backward link would dangle immediately. Do
  **not** edit a parent's changeset or PRD from a child branch: those files are inherited, and a
  commit here puts the parent's docs in this PR's diff and conflicts when the parent wraps.
- Check off the first TODO items in this node's changeset (`Record initial discovery`,
  `Create/update PRD documentation`, `Create changeset`). This node's `{slug}-initial-discovery.md`
  must exist next to the changeset — copy the Step 2 whole-work file into it as Exploration 1 (full
  dump), then append node-specific follow-up. Do **not** delete the whole-work source yet.

**4c. Commit the planning docs — commit 1 of this node.**

- Stage **only** this node's PRD, changeset and discovery companion — never the whole-work discovery
  file, never anything under `src/`. Never use `--no-verify`.
- This commit is the node's plan. Wave 2 adds its contract as a **separate, second** commit; do not
  amend this one later.

**4d. Push and open the draft PR immediately — do not wait for the rest of the stack.**

```bash
git push -u origin HEAD
gh pr create --draft \
  --base "<parent branch, or $TRUNK for a root node>" \
  --title "<type>(<scope>): <what this PR delivers> (#<slug> <K>/<N>)" \
  --body-file <planned-body-file>
```

- **`--draft` is not optional here** and it is a human act — nothing in the API layer sets it for
  you. A draft node is *live* in the stack (it reads as `open`), not planned.
- **`--base` is the parent's branch**, never `$TRUNK`, except for a root node. Getting this wrong
  misroutes the whole chain and inflates the diff.
- **Write the title as shipped, in this repo's conventional-commit form.** Look at
  `git log --oneline -25` first. The scope is the package short name with `tddy-` dropped,
  comma-joined when a change genuinely spans packages, or a feature area when that reads better. All
  stack metadata goes in **one trailing group** `(#<slug> K/N)`, always last. **Never** put a process
  artifact in a title — `red`, `green`, `stubs`, `failing tests`, `WIP`, `phase 1` — and never leave a
  branch slug as a title. On squash merge the title becomes the commit message on `master`,
  permanently.

  > **The one-commit trap.** This repo squashes with `COMMIT_OR_PR_TITLE`, and for a PR with
  > **exactly one commit** GitHub uses the *commit* subject, not the PR title. Between 4d and the
  > wave-2 commit every node is a one-commit PR — so keep commit 1's subject identical to the PR
  > title, or pass the title explicitly at merge time.
- Pass the body from a file so newlines and backticks survive. The body may say the PR is at its
  planning stage; the **title** may not.
- Verify this PR:
  ```bash
  gh pr view --json number,url,isDraft,baseRefName,changedFiles \
    --jq '"#\(.number) draft=\(.isDraft) base=\(.baseRefName) files=\(.changedFiles)"'
  ```
  It must be `draft=true`, based on its parent (a root node on `$TRUNK`), and show **only its own
  files** — in wave 1 that is its PRD, changeset and discovery companion, nothing else. A file count
  that includes a parent's docs means the base is wrong.
- **Note this node's number and URL** for Step 6g — it is written into this node's own changeset in
  wave 2, not committed now (a docs-only third commit would break the two-commit shape).
- **Do not keep a worktree for this branch.** Wave 2 returns to it **exactly once**, in order.

### Step 5: Wave 1 checkpoint — the whole stack is visible for the first time

Every node is now a one-commit draft PR. Before wave 2 touches any branch:

1. **Confirm a clean tree and that the top branch is pushed:**
   ```bash
   git status --porcelain | grep -v '^?? '                            # must be empty
   git rev-list --count "origin/$(git branch --show-current)..HEAD"   # must be 0
   ```
2. **Confirm the chain's shape** — GitHub will not do this for you, so read the bases:
   ```bash
   gh pr list --state open --json number,title,headRefName,baseRefName \
     --jq '.[] | select(.headRefName | startswith("feature/<slug>/")) |
           "#\(.number) \(.headRefName) → \(.baseRefName)"'
   ```
   Every non-root node's base must be its parent's branch; each root's must be `$TRUNK`.
3. **Revise every PR title — MANDATORY.** Each was set in its own 4d, when the stack was still being
   planned: `N` was a projection and scope moved between nodes. A stale `2/6` on a seven-node stack is
   worse than no number, because it is read as fact. Read them all now, as one list:

   ```bash
   gh pr list --state open --json number,title,headRefName \
     --jq '.[] | select(.headRefName | startswith("feature/<slug>/")) | "#\(.number) \(.title)"'
   ```

   Check every one against **PR titles** in the `pr-stack` skill:

   - format `<type>(<scope>): <what it delivers> (#<slug> K/N)`;
   - the **same slug** on all of them — a typo makes one PR look like a different stack;
   - `K` ascending from 1 in reading order, `N` equal to the **final** count;
   - the subject states the delivery **as shipped**;
   - length is a readability convention here, not a hook — there is no `commit-msg` hook and no
     enforced cap. Use the history's judgement: `/pr` asks for under ~70 characters, merged subjects
     run to about a hundred at the outside, and GitHub appends ` (#<pr>)` on top. Put the load-bearing
     words first.

   Fix every one that is wrong, in the same run — plain `gh pr edit` works on `uppin/tddy-coder`; no
   `gh api -X PATCH` workaround is needed:

   ```bash
   gh pr edit <N> --title "<type>(<scope>): <delivery> (#<slug> <K>/<N>)"
   gh pr view <N> --json number,title --jq '"#\(.number) → \(.title)"'
   ```
4. **Register the stack on GitHub — MANDATORY.** Base refs alone give reviewers the right per-PR
   diffs and nothing else. Register the whole chain, bottom to top:

   ```bash
   gh stack link --base "$TRUNK" <pr-1> <pr-2> ... <pr-n>
   ```

   Two traps: it defaults to `--base main`, so pass `$TRUNK` explicitly; and **never `--open`**,
   which would mark every draft ready for review and trigger each one's wrap. `link` reuses the PRs
   that already exist and never removes one, so re-running it with the full list is also how you
   extend the stack later. `gh stack view` reads *local* tracking and will say "not part of a stack"
   after a `link` — that is expected, not a failure; the stack lives on GitHub.

5. **Delete the Step 2 whole-work discovery source** — every node now has its own
   `{slug}-initial-discovery.md`. **Ask the user before deleting** (CLAUDE.md). It was never staged,
   so this drops an untracked file; wrap of the root node must never be the thing that removes it. If
   any node still lacks a companion, **keep** the file and stop here.
6. **Report wave 1 to the user** — the final title list with PR numbers and URLs, each node's
   branch/base, and the order. State plainly that **no node is ready for `/green` yet**: the drafts are
   plans, and wave 2 is about to rewrite every branch above the roots. Then continue into wave 2
   without waiting — the per-node acceptance-test review in Step 6e is where the user gates each node.

## Wave 2 — publish each node's draft-PR contract, bottom-up (commit 2 of each)

### Step 6: For each node in dependency order (n₁ → nₙ) — rebase, surface, failing tests, commit 2, push

This is the `/plan-red` red step (its Steps 6–7), run once per node, **in dependency order**. The
order is not optional: a child's tests compile against its parent's published surface, which only
exists on the parent after **its** wave-2 commit. Each node first rebases onto its parent's new tip,
then does its own work on top. Every commit here leaves the nodes above it behind; the loop clears
each of them when it reaches it, and Step 8 proves nothing was missed.

**6a. Check the branch out here.** The branch exists locally from wave 1. Nothing may pin it
elsewhere:

```bash
git worktree list | grep -q "\[feature/<slug>/<node>\]" && echo PINNED   # must print nothing
git checkout "feature/<slug>/<node>"
git status --porcelain | grep -v '^?? '                                   # must be empty
```

If it is pinned, **stop and ask** for that worktree to be freed — `/green` on this stack has started
too early. Do not `git worktree add` a second copy and do not rebase from the other worktree.

**6b. `/pr-stack-rebase` — single mode, this branch only.** Run it exactly as the hard-gate callers
do, and read the result:

- A **root node** rebases onto `origin/$TRUNK`. Usually *already current, leak-free, no rewrite*; if
  the trunk moved since wave 1 it is rebased and force-pushed with lease, which is fine.
- A **child** rebases onto its parent's branch, which just received its second commit, so it is
  **expected to be behind**. The rebase replays this node's single planning commit on top of the
  parent's two commits and force-pushes with lease. Afterwards:
  ```bash
  git log --oneline "origin/feature/<slug>/<parent>..HEAD"   # exactly ONE commit — this node's plan
  ```
  Anything else is a leak; fix it per `/pr-stack-rebase` before continuing — **never** `git rm` the
  extra files to shrink the diff. The parent's surface is now in this working tree, which is what 6d
  and 6f compile against.

**6c. Verify build and baseline — MANDATORY.** The branch carries only planning docs plus the
inherited surface, so the build **must** pass:

```bash
cargo build
cargo fmt --all --check
cargo clippy -- -D warnings
./test -p <package>            # per package this node touches
./dev bun run cypress:component    # only if this node touches packages/tddy-web
```

If it does not pass, **stop** and report the exact failing command. Do not write tests on a broken
baseline, and do not "fix" a parent's surface from here. Record pre-existing failures so `/green`
does not mistake them for this node's own red tests. Scope the run to the packages this node touches
— a full-workspace run carries pre-existing noise; say which packages you verified.

**6d. Publish the API surface this node owns.**

- Create the public symbols this node owns with correct signatures and types. This is the
  **`## Draft PR contract`** — the interface of a PR that will implement it in the same PR, not a
  deliverable of its own. Annotate each unimplemented body `// TODO(<node>): implement`.
- The branch must **build and lint clean** on that surface alone (`cargo build`,
  `cargo clippy -- -D warnings`) — that is what lets children compile against it and run `/green`
  concurrently.
- **Never touch a parent-owned symbol**, however small the change looks. If this node needs a
  different signature from a parent, that is a plan change: stop and raise it with the user. A
  signature change is the parent's to push, and it goes out on its own, immediately.

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
  `USER REVIEW — acceptance tests` in this node's changeset once approved.

**6f. Failing unit/integration tests.**

- Write comprehensive failing tests for **this node's own surface** — main functionality, edge cases,
  error scenarios, API boundaries. Define the public API through test usage.
- **Its tests exercise only what it owns.** A test here must never assert behaviour a **child** will
  implement, and must never depend on a child being green.
- **Check each test against the parents' published surface.** A test that drives a parent-owned symbol
  still marked `TODO(<earlier-node>)` cannot pass until that node is green. For each such test, either
  **inject a double** at the seam when the parent is incidental to what the test asserts, or **keep
  the real object** when the integration *is* the point — and record that sequencing in this node's
  `## Dependencies`. Do not silently leave it: a test blocked on a parent looks identical to a broken
  test.
- A quick way to find them — every `TODO(` for a node other than this one that this node's tests
  reach:
  ```bash
  grep -rn "TODO(" packages/<pkg>/src | grep -v "TODO(<this-node>)"
  ```
- Verify: the build passes on the published surface, tests fail correctly, **and each failure is
  attributable to this node's own missing implementation** rather than a parent's. Check off
  `TDD Red — write failing unit/integration tests` in the changeset.
- If this step needed more exploration, append it to this node's `{slug}-initial-discovery.md` as a
  new `## Exploration N` section.

**6g. Commit 2 — the draft-PR contract — and push.**

- Update this node's changeset: the TODO check-offs from 6e/6f, the PR number and URL noted in 4d, and
  any `## Dependencies` refinement from 6f. **Own files only** — never a parent's changeset or PRD.
- Commit surface + tests + these doc updates as **one commit**, the node's second. Never amend commit
  1. If something must change after this commit is pushed, add a third commit — never amend, never
  `--no-verify`.
- Push **this branch only** — a plain push; 6b already force-pushed with lease:
  ```bash
  git push origin HEAD
  ```
- Verify the two-commit shape and the diff scope:
  ```bash
  git log --oneline "origin/<base>..HEAD"                        # exactly TWO commits: plan, contract
  gh pr view --json number,isDraft,baseRefName,changedFiles \
    --jq '"#\(.number) draft=\(.isDraft) base=\(.baseRefName) files=\(.changedFiles)"'
  ```
  Still `draft=true`, still based on its parent, and `changedFiles` covering only this node's docs,
  surface and tests. An inherited file or a parent's doc in that list means 6b did not finish cleanly
  — go back to it before moving up.
- **Say so in the report when this push unblocks a node** whose document records it as waiting. That
  node's worktree then runs `/pr-stack-rebase` to pick it up; without the message it waits on
  something that is no longer missing.

## Completion gate

### Step 7: Restore the original branch — MANDATORY

Every node now carries its plan and its contract. This worktree's job as planner is done; do not leave
it sitting on the top branch, which would pin it.

1. Confirm a clean tree and that the top branch is pushed:
   ```bash
   git status --porcelain | grep -v '^?? '                            # must be empty
   git rev-list --count "origin/$(git branch --show-current)..HEAD"   # must be 0
   ```
2. Capture the chain's shape for the Step 9 report:
   ```bash
   gh pr list --state open --json number,title,headRefName,baseRefName \
     --jq '.[] | select(.headRefName | startswith("feature/<slug>/")) |
           "#\(.number) \(.headRefName) → \(.baseRefName)  \(.title)"'
   ```
3. **Restore the starting branch — including `master`:**
   ```bash
   git checkout "$ORIGINAL_BRANCH"
   git branch --show-current   # must equal ORIGINAL_BRANCH
   ```
   Leaving this worktree on `origin/master` (detached), on a stack branch, or anywhere else is a
   defect — it pins that branch and is not a restore. The Step 4a detached `origin/$TRUNK` checkout is
   only for cutting the root node; it is not the restored state.

   **Do not delete this worktree.** **Never delete a stack branch** — each is a child's base, and
   deleting one **closes** that child's PR, which cannot then be reopened or re-based via the API.

### Step 8: Cascade `/pr-stack-rebase` over the whole stack — MANDATORY

With this worktree on `$ORIGINAL_BRANCH` and every stack branch free, run the cascade from here:

```
/pr-stack-rebase from #1 to #<n>     # stack positions, bottom-up
```

Say explicitly that `#1..#n` are **stack positions**, not PR numbers. Cascade mode borrows this
worktree for each layer (nothing pins the branches), rebases bottom-up using each layer's **recorded
pre-rebase tip** with `--onto`, and **restores `$ORIGINAL_BRANCH`** when it finishes or stops.

**Expected result: every layer reports *already current, leak-free, no rewrite*.** Wave 2 rebased each
layer immediately before its second commit, so the cascade is the proof, not the fix. Read its report
per layer:

- **all current** → the stack is coherent; continue to Step 9.
- **a layer was stale and got rebased** → something moved after its 6b. The cascade brought it and
  everything above it current; note in Step 9 which layer and why. Report its build/test verdict,
  never assume it.
- **the cascade stopped** (conflict, pinned worktree with unpushed work, unverifiable build) → the
  stack is **not** complete. Report which layers were done, which stopped, and which were not
  attempted, and do not hand off to `/green`.

Then confirm the restore held:

```bash
git branch --show-current   # must equal ORIGINAL_BRANCH — the cascade restores it, verify anyway
```

### Step 9: Present the Stack

Present a complete summary:

- **The final title of every PR**, as one list, showing the shared slug and ascending `K/N`. This is
  what Step 5's revision pass produced; re-read them now — if wave 2 shifted scope between nodes, fix
  the affected title with `gh pr edit` and report the corrected list.
- **The sequence** — each node's predecessor, its branch, and its PR base, plus the `gh stack link`
  output confirming the stack is registered. Note explicitly that GitHub enforces no merge order and
  will restack nothing after a merge, registered or not.
- **The Step 8 cascade verdict per layer** — already current or rebased, and each layer's build/test
  state. This is the line that says the stack is coherent.
- **This worktree is back on `$ORIGINAL_BRANCH`** (confirm `git branch --show-current`). `/green` on
  these PRs is unblocked now that the completion gate has run.
- Per node: number + URL, branch, base, the **two commits** (plan, contract), PRD, changeset,
  discovery companion, owned surface, failing acceptance + unit/integration test titles and paths, and
  confirmation the build passes on the published surface while tests fail for this node's own missing
  implementation.
- Which nodes can be `/green`-ed in parallel, and which have a sequencing fact recorded in
  `## Dependencies`.
- **Where the order was forced** — any pair that is only sequential because `gh stack` needs a line,
  so a reviewer knows those two could have gone in parallel.
- Anything you could not prove locally (server-side checks, org policy), stated as a risk.

### Step 10: Hand off to `/green`

**Do not create per-node worktrees from this command**, and do not check a stack branch out here again
after the restore — that would pin it. Each branch is already on the remote with an open draft PR;
this worktree stays on `$ORIGINAL_BRANCH`. Whoever picks up a node creates a **different** checkout:

```bash
git fetch origin "feature/<slug>/<node>"
git worktree add <path> "feature/<slug>/<node>"
cd <path> && ./dev bun install     # plus a cargo build to warm target/
```

Start only the nodes whose parents have shipped what they consume; the rest wait. Each changeset's
`## Dependencies` tells its implementer which symbols to leave alone, and `/green` stops and asks if a
capability this node consumes does not exist at `HEAD`.

**In the implementer's worktree, the loop is:**

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

Both `/green` and `/validate-changes` push, because a stack branch is a child's base — unpushed work
is invisible to everyone above you. **Push at every milestone**, not only at the end of `/green`: a
milestone is production code whose own tests already pass. A **signature change is the urgent one** —
push it on its own, immediately, ahead of the implementation behind it.

**Ready the stack bottom-up.** Wrapping is triggered by setting a PR ready for review, so the ready
order *is* the wrap order — and the forward-only doc-linking rule only survives if a parent's
documents leave `1-WIP` before its children's.

**To pick up a moved base, each worktree rebases itself with `/pr-stack-rebase`** (single mode) —
bottom-up, one branch at a time. Never `git rebase --update-refs` from a per-node worktree.

**Each `/green` worktree is temporary.** Remove it once that node's loop is finished — `/green` pushed,
`/validate-changes` clean, `/pr-wrap` done — unless the user asked to keep it. Verify nothing is
unpushed first, then `git worktree remove --force <path> && git worktree prune`. **Never delete the
branch.**

Landing: **`/merge-pr-stack`** — bottom-up, one PR at a time, with the whole-stack comment sweep first,
then the local-gated fix pass, then the CI gate and the `#automerge` squash gate, then the repoint that
nothing does for you once a parent lands. `/merge` and `/repoint` are the by-hand fallback.

> **Never arm `#automerge` on a non-root node.** It merges a PR into **its own base**, which for a
> stacked PR is its parent's branch — folding that node's work into its parent's PR. Arm it only once
> a node's base is `master`.

## Out-of-Scope Ideas

During planning and code analysis, if you identify enhancements outside the current stack's scope, add
them to `docs/dev/TODO.md` under **Future Enhancements**, with the source set to the stack slug.

## Rules

- **Register the stack.** `gh pr create --draft --base <predecessor>` opens each PR; the stack is
  not finished until `gh stack link --base "$TRUNK" <prs…>` has run (Step 5). Do not run
  `gh stack sync` / `gh stack rebase` — `/pr-stack-rebase` owns rewriting branches here.
- **Decompose linearly.** `gh stack` models a line, so plan one. Flatten work that branches, and say
  in the report what that cost — siblings become predecessor and successor, and an independent root
  loses its independence.
- **Refuse a detached start.** `ORIGINAL_BRANCH` must be a named branch.
- **The boundary contract governs the decomposition.** Every node is a vertical slice — schema, code,
  tests, in one PR. Splitting by layer is forbidden; a node that ships only surface is not a valid
  node. Split by capability instead. Only the two named exceptions apply, and do not invent a third.
- **Wave 2 is a draft-PR contract, not a stubs phase.** What it publishes is the first push of a PR
  that goes on to implement the same thing. **A node must never merge in that state.**
- **Two waves, two commits per node.** Wave 1 commits docs only and opens the draft; wave 2 adds
  exactly one commit (surface + failing tests + changeset check-offs). Never amend commit 1; a
  correction after commit 2 is pushed is a third commit.
- **Wave 1 writes no `src/`.** The whole chain must exist on GitHub before any branch is rewritten.
- **Wave 2 runs bottom-up and rebases before it writes.** Each node's 6b `/pr-stack-rebase` (single
  mode) comes before its build, surface and tests. Verify `origin/<base>..HEAD` is one commit after
  the rebase and two after the contract commit.
- **Nobody pins a stack branch until Step 8 has run.** If a branch is pinned, stop and ask; do not
  work around it with a second worktree.
- **Branch names are `feature/<stack-slug>/<node>`**, one namespace for the whole stack.
- **Plan in this worktree.** Do not create per-node worktrees during planning; they belong to `/green`.
- **Restore `$ORIGINAL_BRANCH` when wave 2 is done**, including when that is `master`, and **then run
  the cascade `/pr-stack-rebase from #1 to #n`** as the completion gate. Confirm
  `git branch --show-current` afterwards. Do not delete this worktree. **Never delete a stack branch**
  — it closes the dependent PR, irrecoverably.
- **The cascade is expected to be verify-and-return on every layer.** A rebased layer is a finding to
  report; a stopped cascade means the stack is not complete and `/green` is not unblocked.
- **No `docs/dev/1-WIP/STACK-*.md`.** Each node's context lives in its own changeset. Never record a
  sibling's status.
- **All four changeset headings, on every node** — `## Responsibility`, `## Boundaries`,
  `## Dependencies`, `## Draft PR contract`.
- **Never edit a parent's changeset or PRD from a child branch.** Doc links flow parent → child only.
- **Never implement a symbol another node owns**, however small it looks. Its owner's tests specify
  it; if the surface is wrong for you, say so upward.
- **Title every PR at 4d, then revise all of them in Step 5**, and re-read them in Step 9. A title is
  not planning output — it is the commit message that lands on `master`. Watch the one-commit squash
  trap. Use plain `gh pr edit`.
- **Renumber the whole stack whenever its shape changes** — adding a node changes `N` for every PR.
  Do not renumber a merged PR.
- **Each node's tests exercise only what it owns** — never a child's behaviour, never dependent on a
  child being green.
- **Give each node a distinct changelog/changeset slug** — several nodes of one stack land on the same
  date, and each adds its own file. Never edit a sibling's entry.
- **Ask before deleting** any file, including the whole-work discovery source (CLAUDE.md).
- Tests fully implemented, fluent-tests style, no conditional logic, no fallbacks, no try/catch to
  swallow errors.
- Never put "red phase" / "green phase" in code or test descriptions.
- Never use `--no-verify`, on a commit or a push.
- Verify per package and say which packages you verified — a full-workspace run carries pre-existing
  noise.

## Flow

```
/plan-pr-stack
  record ORIGINAL_BRANCH (stop if detached) and TRUNK
  interview → decompose into a LINEAR sequence of vertical slices → whole-work discovery
  → settle order, slug, owned surfaces, per-node draft-PR contracts
  → WAVE 1 (per node, in this worktree, in dependency order)
        git checkout -b feature/<slug>/<node>   (off its parent; root off origin/$TRUNK)
        PRD + changeset (4 headings) + discovery → commit 1 (docs only)
        git push -u origin HEAD → gh pr create --draft --base <parent> → title → draft PR live
  → checkpoint: verify every base, revise every title, delete whole-work discovery (ask), report
  → WAVE 2 (per node, in this worktree, in dependency order)
        git checkout feature/<slug>/<node> → /pr-stack-rebase (single; picks up the parent's surface)
        build + baseline → owned API surface → failing acceptance tests (USER REVIEW)
        → failing unit/integration tests → commit 2 → git push origin HEAD
  → git checkout ORIGINAL_BRANCH   # if that was master, this worktree is on master again
  → /pr-stack-rebase from #1 to #n   # completion gate: every layer already current, leak-free
  → (per node, in the implementer's own worktree, when its parents have shipped)
        /green            → ALWAYS /pr-stack-rebase first, then the dependency gate; commit + push
        /validate-changes → ALWAYS /pr-stack-rebase first → gaps? back to /green : /pr-wrap
        /pr-wrap          → ALWAYS /pr-stack-rebase first (again), then wrap + ready for review
  → /merge-pr-stack   # bottom-up: comment sweep, fix pass, CI gate, #automerge, repoint
```

## Related

**Commands**: `/plan-red` (its Steps 6–7 are what wave 2 runs per node), `/add-to-pr-stack` (a node on
top of an **existing** stack tail), `/split-pr-to-stack` (carve an **existing** PR into stacked slices;
this command plans from requirements), `/follow-up-branch`, `/green`, `/validate-changes`,
`/pr-stack-rebase` (single mode in wave 2, cascade mode as the completion gate), `/merge-pr-stack`,
`/merge`, `/repoint`, `/pr-wrap`, `/wrap-context-docs`
**Skill**: `pr-stack` (`.agents/skills/pr-stack/SKILL.md`)
**References**: `.agents/skills/planning/references/planning-phase.md`,
`.agents/skills/planning/references/initial-discovery.md`,
`.agents/skills/fluent-tests/references/generic-guidelines.md`
**Feature docs**: the `pr-stack` skill
