# The code-issue record — `packages/<pkg>/docs/code-issues/`

One analyzer finding against one symbol or file, written by
[`analyze-code-issues`](../../analyze-code-issues/SKILL.md). One file per **verified** finding, in
the docs of the package whose code it is about.

## Layout and filename

```
packages/tddy-core/docs/code-issues/
├── cycle-dto-inside-behaviour-module.md
├── god-object-presenter.md
├── heavy-dependency-sqlx-session-catalog.md
└── oversized-file-changeset.md

packages/tddy-daemon/docs/code-issues/
└── misplaced-tests-integration-suites.md
```

Filename `<category>-<file-slug>[-<symbol>].md`. Categories mirror what the analyzers and this
repo's structural passes report:

| Category | Means |
|---|---|
| `crap` | complex **and** untested — `CRAP = complexity² × (1 − coverage)³ + complexity` |
| `missing-tests` | a symbol production references that no test enters. **Never a delete** |
| `oversized-file` | over the 500-production-line budget, measured before the first `#[cfg(test)]` |
| `god-object` | one type carrying unrelated state — field count and method count are the metrics |
| `cycle` | two modules or crates that name each other |
| `heavy-dependency` | a dependency a few files pull into every dependent's build |
| `misplaced-tests` | test binaries that exercise another package's code |
| `squatting` | a subsystem living in a crate whose purpose it does not share |
| `duplicate-tests` | identical or subset test signatures |
| `dead-code` | unreferenced after a find-references check |

**No date prefix, deliberately.** A code issue is a standing property of the code: re-running the
analyzer must find and update the file that already names that symbol rather than deposit a second
copy. Identity is `(category, file, symbol)`, and the filename encodes it.

**No index.** The listing is the index.

## Template

```markdown
# <category>: <symbol or file>

**Location:** `packages/tddy-core/src/presenter/presenter_impl.rs` — `Presenter`
**Category:** god-object
**Detected:** YYYY-MM-DD by <the command, or `structural audit` for a hand-measured one>
**Metrics:** 37 fields · 46 methods · 1,788 production lines · 35 dependent crates
**Restructure:** required — `extract_module --to_file` × 6, `/code-restructuring` territory
**Status:** Open | Open — claimed by #NNN, in flight | Open — partially fixed (<what remains>) | Open — regressed <date>
<!-- only when it applies -->
**Claimed by:** #NNN — `#<stack> K/N` `<node>` · draft · `feature/<stack>/<node>`
**Lands after:** #MMM (its base), #LLL (…)
**Moved:** was `<old path>` — relocated by <what moved it>

## Measurement history

| Run | <metric> | <metric> | Note |
|---|---|---|---|
| 2026-09-15 | 37 fields | 46 methods | first detection |

## What the tool found

<The measurement, and how. "Not measured" is not "clean" — say what did not run.>

## Why it matters here

<What breaks, or breaks silently, while this stands. A number is not a finding on its own.>

## What would close it

<Concretely, and whether it is a `/code-restructuring` job or ordinary work — that decides where it
lands in a changeset.>

## If you are about to change this code

<ONLY on a claimed issue. What a concurrent change is choosing between — see
`planning-cross-check.md` § A claimed issue.>

## Verified by hand

<MANDATORY before the file is trusted as a prerequisite. Say what you checked and what the tool got
wrong. APPEND on re-analysis, dated; never delete an earlier verification.>
```

## The four fields that carry the weight

**`Metrics`** must say how it was measured and what did not run. A CRAP number from one coverage
tier is not a finding, and "not measured" is not "clean".

**`Restructure: required`** is what turns a note into a **prerequisite**: the fix has to happen
before new code lands on top of it, and a different skill executes it. Set it only where the fix is
a `/code-restructuring` job — a split, an extraction, a move — rather than ordinary work.

**`Status`** distinguishes degrees of **open**, and nothing else — a closed issue is deleted, so
there is no closed status to encode. `Open — partially fixed (<what remains>)` is the value that
earns its keep: it is the difference between "nobody has touched this" and "half of it landed in
#NNN and this is the rest", and it is what stops a later wrap deleting a record whose problem is
still there.

**The listing is the open set.** No status filter is needed to enumerate open issues: `ls` does it.

**`Claimed by`** is what makes this repo's records different, and it is the field a concurrent
planner acts on. It names a **PR that is open and will fix this issue**. Three rules:

1. **It is a fact about a PR, so verify it.** Check the PR is still open before relying on it. A
   merged claim means the record should already be **gone** — re-measure, then delete it if the
   finding is closed or narrow it if the merge went only part of the way. A closed-unmerged claim
   means the issue is open and unowned again — remove the field.
2. **`Lands after` is what makes the claim actionable.** A claim on a stack node is only as near as
   the nodes beneath it, so a planner deciding whether to wait needs the whole chain, not just the
   one number.
3. **A claim is not a resolution.** The issue stays `Open`, appears in every open-items query, and
   still gets a verdict at Step 2b. It is the *disposition* that changes, not the status.

## Re-analysis reconciles; it never duplicates

One file per `(category, file, symbol)`, ever. On a re-run:

- **Same finding, new numbers** → append a `Measurement history` row. Do not create a file.
- **Finding gone** → **`git rm` the file**, having first recorded the final measurement in the
  changeset or changelog entry. That entry is the audit trail the file used to be.
- **Finding gone but the code merely moved** → **do not delete.** Rename the file to the new
  location and add `**Moved:**`. The finding is not gone; it is somewhere else.
- **Partially fixed** → the record **stays**, gains the improved numbers, and has its *What would
  close it* narrowed to the remainder. **Never delete a partial fix** — see below.
- **Category changed** (an oversized file split into six, one of which is still oversized) → delete
  the old record and open the new one with `**Supersedes:**` naming what it replaced, so the new
  file explains why it appeared.

### The deletion rule, and the one thing it must not swallow

Deleting a closed record is deliberate: a record that outlives its problem is read by the next
Step 2b and planned around, which costs more than the history was worth. The trade is that a later
regression reads as a **first detection** rather than a recurrence — accepted, and mitigated by the
changelog entry carrying the final numbers.

**A partially-fixed record must never be deleted, and this is the failure mode to guard.** A closed
record's information survives in the changelog; a partial one's *remainder* exists nowhere else.
Deleting it destroys the only description of what is left, silently, and the next analyzer run
rediscovers a smaller version of the problem with no idea it was once bigger or who narrowed it.

## Writing a record for a structural finding with no tool behind it

`/analyze-code-issues` covers CRAP, coverage and duplicate tests. Several categories above —
`cycle`, `god-object`, `squatting`, `misplaced-tests`, `heavy-dependency` — come from reading the
dependency graph instead, and they are legitimate records provided they carry the same things a tool
finding does:

- a **reproducible measurement**, not an impression: the command that counts it, so the next run can
  disagree with it;
- the **exact edge or symbol**, not a module summary — `stream/mod.rs:9` needs
  `ClarificationQuestion`, not "stream depends on backend";
- `**Detected:** YYYY-MM-DD by structural audit`, so nobody reads it as tool output.

A record whose measurement cannot be re-derived is prose, and prose belongs in the package's docs or
a TODO — not here, because reconciliation has nothing to key on.
