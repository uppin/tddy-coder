# Planning Step 2b — cross-checking both records

Planning reads the code-issue and TODO records **after** the code analysis, once you know which
files and modules the change touches, and **before** the changeset is written. Discovering an item
during `/green` is late: by then the only choices are to work around it, silently make it worse, or
stop.

Called by `/plan-red`, `/plan-ft-dev`, `/plan-pr-stack` and `/add-to-pr-stack`, via
[`planning-phase.md`](../../planning/references/planning-phase.md) Step 2b.

## Scan

```bash
PKG=packages/tddy-core            # one per package in scope

# 1. This package's code issues — the listing is the index
ls "$PKG/docs/code-issues/" 2>/dev/null
grep -rL 'Status:\*\* Resolved' "$PKG/docs/code-issues/" 2>/dev/null      # EVERY open issue, first
grep -rl 'Restructure:\*\* required' "$PKG/docs/code-issues/" 2>/dev/null | \
  xargs grep -L 'Status:\*\* Resolved'                                    # …of those, the restructures

# 2. CLAIMED issues — the ones with a PR already in flight. Read these even when
#    they are not in your change's path, because they tell you what is about to move.
grep -rl 'Claimed by:' "$PKG/docs/code-issues/" 2>/dev/null | \
  xargs grep -L 'Status:\*\* Resolved'

# 3. Issues against the exact files this change will edit, wherever they live
grep -rl 'src/presenter/presenter_impl.rs' packages/*/docs/code-issues/ 2>/dev/null

# 4. The repo-wide TODO backlog
ls docs/dev/todo/ | sort -r | head -30
grep -rL 'Status:\*\* Resolved' docs/dev/todo/ 2>/dev/null
grep -rl -iE '(presenter|changeset|<your-module>)' docs/dev/todo/ 2>/dev/null
```

**Step 2 is the one that gets skipped**, and it is the one this repo most needs. A claimed issue on a
file you were not planning to touch still matters: if the claiming PR is about to move that file
under you, your change is planned against a shape with a known expiry date.

**Enumerate every open issue before narrowing.** `Restructure: required` and a per-file grep are
*lenses* on the open set, not the set itself.

**A package with no `docs/code-issues/` directory has not been analyzed**, which is not the same as
clean. Name those packages in the changeset, and run `/analyze-code-issues` when the change lands in
code the analyzers would have something to say about.

Read the **body** of every candidate, not the heading.

## Classify every hit

| Verdict | Meaning | Where it goes |
|---|---|---|
| ⛔ **Blocking** | the change cannot be implemented *correctly* without it | `## Prerequisites`, **and** a line in `## Scope` — it is work, not a note |
| 🚧 **Claimed** | an open issue whose fix is **already in flight** in another PR | `## Prerequisites` — **and you stop and ask**, see below |
| ⚠ **During** | the change touches it, would make it worse, or must not re-introduce it | `## Prerequisites`, as a constraint on how the work is done |
| ℹ **Answered** | planning resolved an open question the item asks | `## Prerequisites` with the answer — and close the item at wrap |
| — **Unrelated** | same area, different concern | nothing. Do not pad the changeset |

**Blocking means correctness, not convenience.** Name the two or three ways to proceed without it
and check whether each is *wrong* rather than annoying. A 700-line module you would rather not read
is not blocking. A module whose only extension point is a function no test enters, where the change
must alter that function's behaviour, is.

Record the verdict even when it is "considered, not fixed here". The value is that a reviewer sees
the item was weighed rather than missed.

## A claimed issue — ask, never assume

This is the verdict that exists because this repo runs large refactors as **background stacks**. A
claimed issue is not debt somebody should fix one day; it is debt with an owner, a PR number and a
position in a queue. A change landing in the same code has a real choice, and **it is the
developer's, every time.**

Getting this wrong is expensive in both directions, which is exactly why it is not a default:

- **Proceeding silently** grows the diff the refactor exists to shrink. The claiming PR must now
  move your code too, and its behaviour-preserving claim gets harder to verify.
- **Waiting silently** stalls shippable work behind a stack that may be weeks out, and nobody asked
  for that trade.

### What to establish before you ask

1. **Is the claim live?** `gh pr view <N> --json state,isDraft,baseRefName`. Merged → the issue
   should be `Resolved`; close it and there is no fork. Closed-unmerged → the claim is stale; strip
   the field and treat it as an ordinary open issue.
2. **How far out is it?** Read `**Lands after:**`. A node is only as near as the chain beneath it, so
   report the whole chain and how much of it is merged.
3. **Does your change actually collide?** Name the files. An issue claimed against
   `presenter_impl.rs` does not constrain a change to `worktree.rs` just because both are
   `tddy-core`. Overlap is per-file, not per-package.
4. **What does the claiming PR do to your files?** Its changeset's `## Responsibility` says. "Moves
   it to another crate" is a different conversation from "splits it into six modules in place".

### How to present it

One block per claimed issue, in the planning report, **before** the changeset is written:

```
🚧 Claimed issue in this change's path — your call

  Issue          packages/tddy-core/docs/code-issues/god-object-presenter.md
  Claimed by     #495 (#carve 9/10, presenter-split) · draft · lands after #488,489,490,498,491,492,493,494
                 → 7 of 8 predecessors still open; realistically weeks out
  Overlap        this change adds a method to `Presenter` and reads `workflow_result`.
                 #495 partitions all 46 methods into six modules and #491 regroups all 37 fields.
                 Every line you add here moves in both.
  If you proceed  ~1 extra method for #495 to place, and its behaviour-preserving claim now covers
                 code #491's field grouping has not seen. Cheap, but not free.
  If you wait     #491 must land first (it owns the sub-structs). Your change then targets a
                 7-field struct and a named module instead of a 1,788-line file.
  Third option    scope this change to `presenter/workflow_runner.rs`, which neither node touches.

  Which: proceed and add to the debt, wait for #491, or narrow the scope?
```

Then **ask, and wait.** Do not fold a decision into `## Prerequisites` before the developer makes
it, and do not start the work.

### Record the answer, whichever it is

| Answer | What the changeset says | What the record says |
|---|---|---|
| **Proceed** | `## Prerequisites` verdict 🚧 with *"proceeding on today's shape with developer consent (quote them); #NNN absorbs it"*, and a `## Decisions & Trade-offs` line | append a `## Concurrent changes` line to the issue naming this changeset, so the claiming PR's `/green` knows what arrived after it was planned |
| **Wait** | the changeset is **not written yet**. Record the dependency and stop — this is not a TODO, it is a scheduling fact | nothing; the claim already says it |
| **Narrow** | `## Boundaries` gains the files this change now avoids and why | nothing |

**Proceeding is a legitimate answer** and usually the right one for a small change. The point of the
fork is that somebody chose it knowing the cost, and that the claiming PR finds out before it
rebases rather than during.

## Right-size the fix

| Size and location | Shape |
|---|---|
| Small, inside this change's own files | do it here; say so in `## Prerequisites` |
| Small, in shared code | **a separate change that lands first.** On a single PR that is a prerequisite PR off `master`. On a **stack** it is **its own stack node** — never a PR outside the stack being created |
| Large, or `Restructure: required` | its **own changeset** — `Type: Refactor`, executed before green. In a stack, **its own node in that stack** |

⚠ **Never absorb a large restructure into a feature change.** It buries a reviewable diff under a
mechanical one, and no reviewer can then tell the behaviour change from the move.

## The restructure prerequisite — planned in the changeset, executed before green

When the verdict is a restructure, the changeset carries it as an executable prerequisite and
`/green` runs it **before** the first line of implementation. Implementing on top of a structure you
are about to change means writing the code twice and reviewing a diff that is both at once.

```markdown
### Restructure Prerequisite — executed BEFORE green

- [ ] `/code-restructuring` against `packages/tddy-core`
  - **Code issues closed**: `packages/tddy-core/docs/code-issues/oversized-file-changeset.md`
  - **Restructure changeset**: `docs/dev/1-WIP/YYYY-MM-DD-<name>-restructure.md` (`Type: Refactor`)
  - **Why before green**: this changeset's implementation lands inside `read_changeset`; splitting
    it afterwards would re-touch every line the feature added
  - **Commit**: its own commit, before any implementation commit
```

Three contract points that are easy to get wrong:

1. **The baseline carve-out.** `/code-restructuring` demands a green baseline and calls a red one a
   stop. A changeset that has been through `/plan-red` *has* red tests by design. So the baseline is
   **"green except this changeset's own planned red tests"** — list those by name as known-red and
   require every other suite to match. Anything red that is not on that list is still a stop.
2. **The restructure rewrites this changeset's own red tests**, mechanically. Expected and correct —
   but they must still fail for the same reason afterwards. A red test that stops failing during a
   restructure has been broken, not fixed.
3. **It stays behaviour-preserving.** If a seam turns out to need the feature's behaviour to exist
   first, that is a planning error: re-sequence rather than sliding implementation into the
   restructure commit.

**Planning reserves the restructure changeset's path; it does not write the document.** Naming the
path is what lets `/green` find the work without re-deriving it.

## Softening the ground — a restructure you propose, not one a record asked for

The scan finds debt somebody already wrote down. This is the other source: **your own reading of the
code**, when the structure as it stands would make this change harder or riskier than it needs to be.

Suggest it. **Do not decide it** — a preparatory restructure spends the developer's review budget
before the feature has earned any.

The strongest signal in this repo:

> **The red phase cannot write a clean failing test against the current shape.** If expressing the
> acceptance test needs a private function reached through a test-only `pub`, a whole service
> constructed to exercise one branch, or a double of a double — the ground needs softening. A test
> you cannot write cleanly now is a test `/green` will rewrite, and a red phase built on a contorted
> seam specifies the contortion.

The bar, and the first test is the one that matters:

1. **Name the specific difficulty in this changeset that the restructure removes.** "The code would
   be nicer" is not a difficulty.
2. **It must be smaller than the change it enables.** A restructure that dwarfs its feature has
   become the work — replan it on its own merits.
3. **It must be behaviour-preserving and expressible in the vocabulary.** Check the operation table
   in `code-restructuring/references/plan-schema.md` first. Rust has no whole-symbol move, and
   `extract_module` refuses to lift one member out of an `impl` its siblings call. A seam the tools
   cannot reach is not a `/code-restructuring` job, and saying so is more useful than proposing one.
4. **The blast radius is measured, not guessed.** Give the caller count.

**If declined**, record the observation so it is not re-proposed as new next session: a code issue if
it is measurable, a `Future enhancement` TODO if it is not. Note in `## Decisions & Trade-offs` that
it was proposed and declined, so a reviewer reading a contorted seam sees a decision rather than an
oversight.

## A systemic restructure across a PR stack

When Step 2b finds an issue that is not one node's problem — the seam every node builds on, or a
file more than one node edits — **`/plan-pr-stack` has one shape: its own node in this stack**,
placed immediately before the first node that needs it. There is no "prerequisite PR off the trunk"
option: that would be a PR the planner created that is not in the stack.

**The restructure node does not block the planning of the nodes above it.** They are planned and
red-tested against the structure **as it is today**. That is deliberate — planning cannot wait for
an execution that happens in `/green`.

The consequence is accepted up front: **when the restructure node greens, the nodes above it
break.** What makes that safe rather than chaotic:

- every affected node's changeset carries an `## Inherited Restructure — expect rewrite` section
  naming the restructure node and what moves;
- the restructure node's `/green` reports, per successor, what it moved under them;
- each successor's next `/pr-stack-rebase` surfaces the conflict, and its **repair is a new commit**
  in that node — never an amend, never an edit to the restructure node from a successor branch;
- the successor's red tests must still be red **for their own missing implementation** after the
  repair. A test that went green in the repair was satisfied by the restructure, which means it was
  never testing this node's behaviour — raise it, do not pocket it.

## Carry the verdicts into the changeset

`## Prerequisites` has four columns and two authors. **Planning fills `Item`, `Verdict` and
`Disposition`**; `/green` fills **`Outcome`**. Write a `Disposition` a later phase can be held to:
"fixed here — see Scope" is checkable, "consider addressing" is not.

Reference each record **by path in backticks, not a Markdown link** — a code issue is renamed when
the symbol it names moves, and a link rots silently.
