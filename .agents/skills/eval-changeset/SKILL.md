---
name: eval-changeset
description: Evaluate a landed or in-flight changeset — one PR, or a whole stack squashed into a single integration branch — for complexity, whether that complexity was justified by the expressed intent, how cleanly each stacked PR incremented on its predecessors, whether any PR was too large to review or to go green on its own, how the system's design helped or fought the change, and what redesign would have made it easier. Use when reviewing the cost of a change after the fact, deciding whether a stack was worth its size or split along the right seams, or looking for the design deficiency a painful change exposed.
---

# eval-changeset — judge a changeset as one unit

This is a **retrospective on a change**, not a code review. `/code-review` and `/validate-changes`
ask *is this correct and ready*. This skill asks five different questions:

1. **How complex was the changeset** — as one unit, whether it is one PR or a stack of eight.
2. **Was that size and complexity justified** by the intent the change claimed to serve.
3. **Did each PR of the stack cleanly increment** on its predecessors, or did it rewrite their work.
4. **How did the system's design serve the change** — where it carried it, where it fought it.
5. **If it fought the change, what redesign would have helped** — proposed, quantified, never
   implemented.

The unit of evaluation is the **changeset**: the whole delta from the trunk to the finished work. A
stack of PRs is one changeset, so it is squashed into a single integration branch in a temporary
worktree and measured there. Reviewing a stack node-by-node systematically under-reports its cost —
the plumbing that node 2 added for node 5 looks free in both diffs and is only visible in the union.
But the per-PR diffs are not thrown away either: comparing each PR against the integrated total is
what answers question 3.

## When to use

- After landing a stack, to ask what it cost and why.
- Before landing one, when the size feels out of proportion to the intent and you want the number.
- When a stack felt like it kept re-doing its own work, and you want to know whether the plan or the
  system caused it.
- When a change was painful and you want the design deficiency named, with evidence.
- When choosing what to refactor next: this skill's friction sites are ranked refactor candidates.

**Not** for: correctness (`/code-review`), production readiness (`/validate-prod-ready`), test
quality (`/validate-tests`), per-file metrics (`/analyze-clean-code`), or CRAP scores
(`analyze-code-issues`). Those are inputs you may pull in — see § 7 — not what this skill replaces.

## Ground rules

- **Never estimate a number you did not compute.** Every figure in the report comes from a command
  that ran. If a measurement was skipped, the report says skipped, not zero.
- **Propose, never implement.** No edits to production code, no PRs, no branch rewrites of anything
  but the throwaway eval branch. Per CLAUDE.md the developer owns the code; this skill hands them a
  judgement and options.
- **Challenge the change, and yourself.** If the size was justified, say so plainly and stop looking
  for a villain. A changeset that is large because the problem is large is a good changeset.
- **Never push the eval branch, never delete a stack branch.** A stack branch is its successor's
  base; deleting it closes that PR (`pr-stack` golden rule 2).

## Workflow

### 1. Resolve the target — say which shape you are evaluating

| Input | Shape |
|---|---|
| no argument | the current branch |
| a PR number, or several | those PRs |
| a branch name | that branch |
| `--stack` | the stack the current branch belongs to |

Then classify. Load [`pr-stack`](../pr-stack/SKILL.md) — **a stack in this repo is a registered
`gh stack` object and is therefore a single ordered line**, each PR based on the one below it. Detect
membership exactly as [`.agents/commands/pr.md`](../../commands/pr.md) § 2 does — the trunk, this
branch's PR, then the open PRs that are ancestors of it:

```bash
git symbolic-ref --short refs/remotes/origin/HEAD          # trunk; fallback master, then main
gh pr list --state open --head "$(git branch --show-current)" --json number,baseRefName
gh pr list --state open --json number,headRefName,baseRefName
git merge-base --is-ancestor "origin/<headRefName>" HEAD   # ancestor ⇒ same stack
```

**A base that is not the trunk means you are in a stack** whether or not anyone registered one. Three
more sources of membership, since `gh pr list --state open` cannot see a PR that already landed:

- the **branch namespace** — every branch of one stack shares `feature/<stack-slug>/<node>`;
- the **title group** — `(#<stack-slug> K/N)` names the stack and the position, so `N` tells you how
  many PRs to account for and whether any are missing from your list;
- **merged PRs** — `gh pr list --state merged --search '<stack-slug>'`, or the squash subjects on the
  trunk.

Say in the report if a node could not be recovered; a stack evaluated with a hole in it under-reports.

(`gh stack view` reads *local* tracking state and reports "not part of a stack" for a stack created
with `gh stack link`, so it is not a membership test here.)

**One ambiguity you must resolve with the user, not guess:** a single PR whose base is another open
PR can be evaluated two ways —

> Evaluating **#N** whose base is **#M**. Do you want **(A) this PR alone** — its diff against
> `<base-branch>` — or **(B) the changeset through this PR** — the stack from the trunk up to it?

(B) is the default reading of "evaluate the changeset"; (A) is the right one when judging a single
PR's boundary. Say in the report which you evaluated.

State the resolved shape before measuring: *single PR*, or *stack of N, positions K₁…Kₙ*.

### 2. Build the integration branch

Mechanics — the fork point, a branch that drifted out of line, unpushed branches, cleanup — are in
[`references/integration-branch.md`](references/integration-branch.md). The short form:

```bash
git fetch origin
BASE=$(git merge-base origin/master origin/<top-branch>)   # the fork point, not today's master
git worktree add --detach tmp/eval-changeset/<slug> "$BASE"
git -C tmp/eval-changeset/<slug> checkout -b eval/<slug>
git -C tmp/eval-changeset/<slug> merge --squash origin/<top-branch>
git -C tmp/eval-changeset/<slug> commit -m "eval: <slug> squashed changeset"
```

A linear stack squashes in one step, because the top branch already contains every PR below it.
**Verify that rather than assuming it** — a PR that was never restacked, or a shape that was
linearized late, leaves a branch that is not an ancestor of the top.

`tmp/` is gitignored, and this worktree needs no `target/` or `node_modules` unless you take the
optional deep pass in § 7 — so it is cheap, unlike the per-PR worktrees `pr-stack` warns about.

Two things that are findings in themselves, not setup noise:

- **The base is the fork point**, never today's `origin/master`. Diffing against a moved trunk
  attributes other people's work to this changeset.
- **A branch that is not an ancestor of the top** is a stack that drifted out of line — a missing
  restack, or a branching shape never linearized. Squash it in separately, record it for § 5, and say
  in the report that the integrated tree is a reconstruction rather than a tree that ever existed.

**Cleanup is part of the workflow, not an afterthought** — remove the worktree and delete
`eval/<slug>` when the report is written (§ 8).

### 3. Recover the expressed intent — before you look at the diff

Judging size against intent requires the intent to exist independently of the change. Read it first,
so the diff cannot talk you into a rationalisation. Sources, in order of authority:

1. **Each PR's own documents**, committed on its own branch under `docs/dev/1-WIP/`:
   `YYYY-MM-DD-<slug>.md` (the changeset — `## Responsibility`, `## Boundaries`, `## Dependencies`,
   `## Draft PR contract`) and `YYYY-MM-DD-<slug>-prd.md`. The four headings **are** the per-PR intent
   statement, and § 5 needs them. There is deliberately **no shared stack manifest** in this repo —
   do not look for one, and do not create one; the whole-stack view is assembled by reading each PR's
   own document, which is what the table below does.
2. A landed PR's documents have already been wrapped out of `1-WIP` — look in `docs/dev/changesets/`
   and the package docs, or read them from the branch before the wrap: `git show <sha>:<path>`.
3. `docs/ft/*/1-WIP/YYYY-MM-DD-*.md` PRD for the product-level intent.
4. PR titles and bodies: `gh pr view <N> --json title,body,url`, plus any linked issue.
5. Commit subjects: `git log --format='%s' "$BASE..eval/<slug>"`.

Write down **one paragraph** of intent and quote its source. If nothing above states an intent,
reconstruct it from the diff and **label it inferred** — a reconstructed intent always fits the
change perfectly, so a justification verdict against one is weak evidence and must say so.

Note explicitly whether the change **exceeds** its stated boundaries — a `## Boundaries` line saying
"does not touch the daemon" against a diff that touches the daemon is a finding regardless of size.

### 4. Measure the whole — size, then complexity

**Size is mechanical.** Run these against `"$BASE"..eval/<slug>` in the eval worktree:

```bash
git diff --stat "$BASE"..HEAD | tail -1
git diff --numstat "$BASE"..HEAD                     # per-file +/-; the basis for every split below
git diff --name-status --find-renames "$BASE"..HEAD  # A/M/D/R
git diff --dirstat=files,0 "$BASE"..HEAD             # where the change concentrated
git log --oneline "$BASE"..HEAD | wc -l              # pre-squash commit count, from the stack branches
```

Split the line counts by **role**, and report generated content separately — never inside a total
you then judge:

| Role | What counts |
|---|---|
| Production | shipped source |
| Test | tests, fixtures, mocks, testkits |
| Docs | `docs/**`, `*.md`, doc comments-only changes |
| Build & config | `Cargo.toml`, `flake.nix`, `./install`, `./release`, CI, systemd units, `*.yaml` |
| Generated (excluded from judgement) | `Cargo.lock`, `bun.lock`, snapshots, generated bindings |

**Complexity is judgement, and every rating needs evidence.** Rate 1–5, each with the file that
justifies it:

- **Conceptual load** — how many new concepts must a reader hold at once? Count the new public types,
  traits, states, protocol messages, invariants.
- **Control-flow depth** — new concurrency, async lifetimes, retries, state machines, error paths.
- **Blast radius** — to review *one* behaviour change, how many independent sites must be understood?
- **Reversibility** — is it behind a seam, or has it changed a wire format, an on-disk layout, a
  public API, an installed unit? Hard-to-reverse costs more than its line count suggests.
- **Review cost** — realistic reviewer-hours, and whether the node boundaries actually reduced them.

### 5. Per-PR increments — stacks only

**The ideal stack is one where every PR is a pure increment: it adds to what its predecessors built
and rewrites none of it.** A node that re-edits lines an earlier node in the same stack wrote is
doing work twice, and every such line was reviewed twice — once in the version that did not survive.
Measure it; do not eyeball it. Recipes are in
[`references/increment-analysis.md`](references/increment-analysis.md).

**Per PR, measure its own diff against its predecessor's tip** (not against the trunk):

```bash
git diff --numstat "$PRED_TIP".."$PR_TIP"
```

Then compute the three figures that matter:

| Figure | How | Reads as |
|---|---|---|
| **Inflation** | Σ per-PR changed lines ÷ integrated changed lines | 1.0 = every PR a pure increment. 1.3 = 30% of the work never reached the final tree. |
| **Rework, per PR** | of the lines this PR deletes or replaces, the share whose blame lands on a **predecessor in this same stack** | the direct measure of "changed what predecessors developed" |
| **File overlap** | files this PR touches that a predecessor also touched | the cheap pre-filter — blame only the overlapping files |

Report a row per PR — own size, **changed files**, overlap with predecessors, rework lines, rework
share — and put the integrated total on the last row so the comparison the table exists for is on one
screen. Flag every PR whose changed-file count is over 20; those get § 5a.

Distinguish two things a blame hit can mean, because only one is rework:

- **Rework** — the PR *replaced or deleted* a predecessor's line. Counts.
- **Extension** — the PR *added* lines to a file a predecessor created, leaving its lines intact.
  Does not count. Growing a module you built one PR earlier is exactly what a stack is for.

**Then answer the question the number raises: whose fault is the rework?** Three verdicts, and the
evidence separates them:

| Verdict | What the evidence looks like |
|---|---|
| **Planning** — the boundaries were drawn in the wrong place | Rework sits in code that squarely belongs to the *later* PR's responsibility: a predecessor wrote a stub, a placeholder, a default or a scaffold that its successor then replaced. Or the split follows **layers** (types → service → UI) rather than user-visible increments — which `pr-stack`'s boundary contract forbids outright, since a stubs-only PR is not a valid one. Or two adjacent PRs keep editing each other's files, meaning a different cut of the same work would have been orthogonal. **The fix is a better plan**, and § 8 says what the cut should have been. |
| **System** — the design forced every PR through the same place | Rework concentrates on a shared chokepoint that *any* decomposition would have hit: a central registry, a god module, a config fact declared in K homes, a signature that ripples through N layers. Test it by asking whether a different split would have avoided it — if no split would, the plan is exonerated and this is a § 7 friction site. **The fix is a redesign**, and it gets a § 8 proposal. |
| **Discovery** — the later PR learned something the earlier one could not have known | The rework follows a requirement, constraint or failure that surfaced *during* the stack: a review comment, a CI failure, an integration that behaved differently than assumed. Neither the plan nor the system is at fault, and saying so is a real finding. Worth reporting only as *what would have surfaced it earlier* — a spike, an acceptance test, a draft PR contract. |
| **Linearization** — two independent PRs were flattened into a line | `gh stack` models a line, so genuinely parallel work becomes predecessor and successor. Rework between two such PRs is the cost of that flattening, not a planning error — unless a different linear order would have avoided it, which is worth checking and saying. |

Do not force a single verdict on the whole stack. Attribute **per rework site**, then say which
cause dominates by lines.

### 5a. Oversized PRs — the 20-file line

**Report the changed-file count for every PR, and for the integrated changeset.** Count production +
test + config; report docs and generated files separately and keep them out of the figure you judge,
the same way § 4 does.

**A PR changing more than 20 such files gets its own analysis.** Twenty is a review-attention
heuristic, not a law — say so, and do not manufacture a finding for a PR at 21. But past it, a
reviewer stops holding the whole change in their head, and `pr-stack`'s contract is already at risk:
every PR must be independently reviewable and independently mergeable — **self-greenable**, meaning
its own tests pass, its CI is green, and it can land without its successors.

The question is the same shape as the rework verdict, with a different subject:

> Could this PR have been smaller and still gone green on its own?

**The atomic core test answers it.** The atomic core is the smallest set of the PR's changed files
that must land *together* for the tree to compile and its tests to pass. Everything outside the core
was separable — it could have been its own PR, below or above this one.

| Finding | What it looks like |
|---|---|
| **Planning** — the PR bundled separable work | The atomic core is small; the remainder is separable and often cohesive on its own (a rename, a docs sweep, a second responsibility, opportunistic cleanup from § 6). A different cut *was* available and nobody took it. **The fix is a better plan** — name the split, in linear order, in § 8. |
| **Design** — nothing smaller could have gone green | The atomic core is itself over the line. No decomposition would have been smaller, because the system forces these files to move together. **The fix is a redesign**, and it earns a § 8 proposal. |

When the verdict is *design*, name the force — the diagnosis is worthless unless it is specific:

| Force | Why nothing smaller compiles or goes green |
|---|---|
| **Signature atomicity** | a trait method, type or wire message changed, so every implementor and call site must move in the same commit. In Rust this is a hard compile boundary, not a preference — and the size of the blast is the size of the trait's implementor set |
| **Knowledge duplication** | one fact declared in K homes (a config key across `./install`, `daemon.yaml`, the unit file, a Rust default, a docs table) — all K must change together or the system is inconsistent at rest |
| **Circular dependency** | two packages that must change in lockstep because neither can compile against the other's old shape |
| **No seam to land behind** | no trait, registry or flag lets the old and new paths coexist for the length of one PR, so the swap has to be total |
| **Test-at-the-end** | the only test that proves the change is an acceptance test that passes only once the last layer lands. This is also a `## Draft PR contract` failure — that heading exists precisely so a predecessor can ship failing tests and unblock its successors |
| **Mechanical ripple** | a rename or move touching N files. Listed here because it *looks* like design, but it is **usually planning**: a pure-mechanical PR first, then the behavioural one, is almost always available. Call it design only if the rename cannot be separated from the behaviour change |

**Measuring the core.** Cheap version: read the diff and name the keystone — the one edit that
everything else exists to satisfy. Definitive version, in the eval worktree, when the verdict actually
turns on it:

```bash
git checkout "$BASE" -- <candidate separable paths>   # revert the subset you believe was separable
./dev cargo check -p <crate>                          # or ./test -p <crate> for the green claim
git checkout HEAD -- <candidate separable paths>      # restore the eval tree afterwards
```

If it still compiles and that package's tests still pass, the subset **was** separable and the
planning verdict is proven rather than asserted. If it does not, you have located a real atomicity
boundary and can say exactly which symbol enforces it. Skip this when the answer is already obvious
from the diff, and say in the report which way you decided it.

**A single PR over the line is the same question, asked once:** *should this have been a stack at
all?* Run the same atomic core test — if the core is small, the answer is yes, and § 8 names the PRs
it should have been.

**Also check the boundary contract itself** while the per-PR data is in hand: a PR whose diff is only
types or only stubs, a PR that implements a surface its `## Dependencies` says belongs to a
predecessor, a PR that cannot be reviewed without reading its successor, or a PR missing any of the
four required headings. Each is a planning finding independent of the rework count — as is a chain
that was never registered with `gh stack link`, which leaves reviewers no stack view at all.

For a **single PR** the increment figures do not apply — say so rather than omitting the section —
but **§ 5a still runs**: a 40-file standalone PR raises the same question a 40-file PR inside a stack does.

### 6. Classify every hunk — the number that carries the whole report

Put each changed file (or hunk group, when a file is mixed) into exactly one bucket:

| Bucket | Meaning |
|---|---|
| **Essential** | directly realises the intent — this is the change |
| **Enabling** | seam or refactor work that legitimately had to happen first |
| **Incidental** | tax the existing design imposed: mechanical propagation, signature churn, the same fact re-declared, mocks updated for a behaviour they do not exercise, re-exports, wiring |
| **Opportunistic** | unrelated cleanup that rode along — scope creep, however welcome |

The **essential : enabling : incidental : opportunistic** line split is the single most informative
figure this skill produces. Everything downstream leans on it: a high incidental share *is* the
design deficiency, located in the files it fell on.

Be strict about the essential bucket. A file is essential only if removing the change to it would
leave the intent undelivered.

Note that this classifies the **integrated** diff, so intra-stack rework has already cancelled out of
it — which is exactly why § 5 is measured separately. A stack can be 90% essential in its final tree
and still have cost 40% more work than it shows.

### 7. Diagnose — where the design carried the change, and where it fought

**Both lists are required.** Naming only friction produces a report that reads as a complaint and
tells the developer nothing about what to protect.

**Carried it.** For each: the seam, the file that proves it, and what it saved.
> *"A new backend needed `impl CodingBackend` plus one registry line — the trait absorbed it."*

**Fought it.** Name the pattern; do not describe it freehand. Each entry carries the pattern, the
evidence, and the incidental lines it caused:

| Pattern | Signature in the diff |
|---|---|
| **Shotgun surgery** | one behaviour change → edits in N unrelated files |
| **Parallel change** | the same edit repeated in K places, or successive PRs colliding on the same file |
| **Knowledge duplication** | one fact declared in K homes — a config key in `./install`, `daemon.yaml`, the unit file, a Rust default and a docs table |
| **Leaky abstraction** | a leaf change forces signature churn up through N layers |
| **Missing seam** | production code had to change to make the thing testable |
| **God module** | a file already over 500 lines had to grow again |
| **Test coupling** | N mocks or fixtures updated for one behaviour change |
| **Temporal coupling** | order-dependent init or setup the change had to thread through |
| **Cross-package cycle** | the change forced a dependency edge that should not exist |
| **Homeless code** | new code landed in a package because nothing owned the concern |
| **Unsplittable chokepoint** | every PR of the stack had to edit the same file — the § 5 *system* verdict, seen from the design side |
| **Compile-atomic surface** | a signature whose implementor set is so wide that no PR touching it can stay small — the § 5a *design* verdict, seen from the design side |

Also check what the change *avoided* saying: a fallback added without consent, a test-only branch in
production code, a `TODO`/`FIXME` standing in for the hard half. CLAUDE.md forbids the first two
outright — finding one flips the justification verdict from "small" to "under-scoped", because the
change is smaller than the problem.

**Optional deep pass**, when the friction is real but you want it quantified beyond line counts —
these are inputs, and if you skip them the report says so:

- `/analyze-clean-code` on the integrated tree — file length, nesting, duplication.
- `analyze-code-issues` (`tddy-tools analyze`) for CRAP on the touched crates. Needs a build in the
  eval worktree, which is the one thing that makes it expensive.
- Per-package tests only, never the workspace: `./test -p <crate>`. Full-workspace runs carry
  pre-existing noise that would be misattributed to this changeset.

### 8. Report, then clean up

Write the report to `tmp/eval-changeset/<slug>/report.md` using
[`references/report-template.md`](references/report-template.md), and give the user the verdicts, the
headline numbers — including the per-PR table if it is a stack — and the ranked proposals in the
reply, not the whole document.

Proposals come in **two kinds**, and mixing them wastes the distinction § 5 just established:

- **Planning proposals** — where the stack should have been cut instead: the rework § 5 attributed to
  planning, and the separable remainder § 5a found in an oversized PR. Concrete: name the PRs, their
  responsibilities, their order, and what each would have avoided. Every proposed PR must be
  **self-greenable** — its own tests pass, without its successors — or you have proposed the layer
  split the boundary contract forbids. Keep the cut **linear**; a branching proposal is not
  implementable here. These cost nothing to adopt and apply to the *next* stack.
- **Redesign proposals** — the payload for question 5. Raise one only for a friction site that cost
  real lines, and give each of them all six parts:

  1. **Deficiency** — which friction site, quantified from § 6, § 5 and § 5a.
  2. **Proposal** — the seam, in one sentence, naming the types or modules it would create.
  3. **Counterfactual** — what *this* changeset would have been under it: *"≈4 files / ≈300 lines
     instead of 19 / 2,400; PRs 3 and 4 would not have existed, and PR 5's rework of PR 2
     disappears."* Without a counterfactual a proposal is an opinion.
  4. **Cost** — migration size, call sites moved, what breaks, whether it can be done incrementally.
  5. **Risk of not doing it** — what the next change of this shape pays.
  6. **Verdict** — *do now* / *do before the next change of this shape* / *not worth it*, and why.

Rank redesign proposals by (lines saved on the next change of this shape × how often that shape
recurs) ÷ migration cost. A proposal that is not worth doing is still worth reporting **as** not
worth doing — that is a decision the developer no longer has to re-derive.

Then clean up, verifying before removing:

```bash
git -C tmp/eval-changeset/<slug> status --porcelain   # expect empty
git worktree remove --force tmp/eval-changeset/<slug>
git branch -D eval/<slug>
git worktree prune
```

Offer, do not assume: *"Keep this report? I can copy it to `docs/dev/evals/<date>-<slug>.md` so it is
tracked."* Nothing here is committed without the user asking.

## Related

**Commands**: `/code-review`, `/validate-changes`, `/analyze-clean-code`, `/pr-wrap`, `/squash-pr`,
`/split-pr-to-stack`
**Skills**: [`pr-stack`](../pr-stack/SKILL.md), [`analyze-code-issues`](../analyze-code-issues/SKILL.md),
[`code-restructuring`](../code-restructuring/SKILL.md)
**Guides**: [`docs/dev/guides/ci.md`](../../../docs/dev/guides/ci.md),
[`docs/dev/guides/changelog-merge-hygiene.md`](../../../docs/dev/guides/changelog-merge-hygiene.md)
