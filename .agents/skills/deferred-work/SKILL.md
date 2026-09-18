---
name: deferred-work
description: The two file-per-item records this repo keeps for work it did NOT do — code issues in the docs of the package they describe (`packages/<pkg>/docs/code-issues/`) and changeset TODOs in `docs/dev/todo/`. Defines the record formats, how planning cross-checks both before a changeset is written, the restructure-before-green contract, and — specific to this repo — how an issue already CLAIMED by an in-flight PR forces a wait-or-proceed decision the developer makes, never the planner. Use when planning a changeset or PR stack, when persisting `/analyze-code-issues` output, when something cannot be finished, or when landing a stack.
---

# Deferred Work — code issues and TODOs

Two records, one file per item, both read as often as they are written.

```
packages/<pkg>/docs/code-issues/      ← THE RECORD: analyzer findings, one file per issue
    <category>-<file-slug>[-<symbol>].md        (no date — see below)

docs/dev/todo/                        ← THE RECORD: deferred work, one file per item
    YYYY-MM-DD-<slug>.md

.agents/skills/deferred-work/         ← the rules, not the records
├── SKILL.md                          ← you are here
└── references/
    ├── code-issue-record.md          ← the file format, identity, reconciliation, claims
    └── planning-cross-check.md       ← Step 2b: verdicts, right-sizing, the wait-or-proceed fork
```

| Record | Path | One file is | Written by | Read by |
|---|---|---|---|---|
| **Code issue** | `packages/<pkg>/docs/code-issues/` | one analyzer finding against one symbol or file | `/analyze-code-issues`, `/pr-wrap` 7.5, `/wrap-context-docs` | planning Step 2b, `/code-restructuring` |
| **Changeset TODO** | `docs/dev/todo/` | one thing a changeset planned or discovered and **could not** finish | `/green`, `/plan-red`, `/plan-pr-stack`, `/pr-wrap` | planning Step 2b, `/pr-wrap` |

**Code issues are per-package; TODOs are repo-wide.** A code issue names a file, so it belongs
beside that file's package and travels with it — a reader of `packages/tddy-core/docs/` finds that
crate's open debt without knowing this skill exists. A deferral often constrains two packages at
once and has no single home, so it stays in the flat `docs/dev/todo/` this repo already uses. That
asymmetry is deliberate; do not "unify" it.

Neither directory has an index. **The listing is the index** — an index file is a shared
append-point, so every branch that recorded a finding would edit the same lines and conflict on
merge. Same reason `docs/dev/1-WIP/` and `docs/dev/changesets/` have none.

## Why the two naming conventions differ

| | Code issue | Changeset TODO |
|---|---|---|
| Filename | `<category>-<file-slug>[-<symbol>].md`, **no date** | `YYYY-MM-DD-<slug>.md` |
| Because | a finding is a **standing property of the code** — re-running the analyzer must update the file that already names that symbol | a deferral is an **event** — it happened on a day, and a second deferral in the same area is a second file |
| Identity | `(category, file, symbol)` | the date and the slug |

Both resolve **in place** — `**Status:** Resolved (YYYY-MM-DD, PR #NNNN)` — rather than being
deleted. The history is worth more than the tidiness, and a resolved file drops out of the
open-items queries, which are all `grep -L 'Status:\*\* Resolved'`.

## The rule that comes before every format

**A changeset's goal is the full scope that was planned.** A TODO is not a scope-reduction lever.
Before writing one for **planned** scope, the deferral must pass all four:

1. **It is impossible now, not merely expensive.** Name what makes it impossible. If the sentence is
   "this would take a while", it fails.
2. **Every route around it is wrong.** Name the two or three ways to proceed and say why each is
   *incorrect* rather than inconvenient.
3. **The developer consented**, in this session, and you recorded their words.
4. **The remaining scope still ships something coherent.**

> A deferral of something the changeset never planned is a **future enhancement**, not a deferral.
> Write those freely with `**Category:** Future enhancement`; rules 1–4 govern planned scope only.

## What is specific to this repo: an issue can be *claimed*

A code issue here may be **claimed by a PR that is already open and not yet merged** — typically a
node of a background refactoring stack. That is the normal state in this repo, not an edge case:
`#carve` alone claims a dozen issues across four packages and will take weeks to land.

A claimed issue changes what planning does. It is no longer "debt somebody should fix"; it is **debt
with a landing date and an owner**, and a change that touches the same code now has a real choice:

- **wait** for the claiming PR, and plan against the shape it will leave; or
- **proceed** on today's shape, accepting that the claiming PR must absorb the change — which grows
  the very diff the refactor exists to shrink.

**That choice is the developer's, every time.** A planner that picks one silently either stalls work
that could have shipped or quietly adds to the debt somebody is mid-way through paying down. The
verdict, the presentation format and the exact question are in
[`references/planning-cross-check.md`](references/planning-cross-check.md) § *A claimed issue — ask,
never assume*.

## The two records

- **[`references/code-issue-record.md`](references/code-issue-record.md)** — the code-issue file:
  identity-based naming, template, `Claimed by`, measurement history, reconciliation.
- **[`references/planning-cross-check.md`](references/planning-cross-check.md)** — Step 2b: the scan,
  the verdict table, right-sizing, the restructure-before-green contract, and the wait-or-proceed
  fork for a claimed issue.

## Rules

- **One file per item, and one item per file.** Never add a second *item* to an existing file. It is
  **not** a rule against editing a record: updating an item's own file is required, and a code
  issue's `Measurement history` and `Verified by hand` sections are append-only across runs.
- **No index**, in either record. The listing is the index.
- **State *why*, not only *what*.** The blocker paragraph is what the next planner's Step 2b acts on.
- **Verify a code issue by hand before treating it as a prerequisite.** Every analyzer has blind
  spots. An unverified finding is a lead, not an issue.
- **Re-analysis reconciles; it never duplicates.** One open code-issue file per symbol, ever.
- **Missing-tests is never a delete.** Those symbols are referenced from production.
- **Step 2b is mandatory in planning**, and every relevant item gets a verdict in the changeset's
  `## Prerequisites` — including the ones you decide not to fix.
- **A claimed issue is a question, not a verdict.** Present the fork and wait. Never default to
  proceeding, and never default to waiting.
- **A claim is a fact about a PR, so verify it.** `**Claimed by:**` names a PR number; check it is
  still open before you rely on it. A merged claim means the issue should already be `Resolved`, and
  a closed one means the issue is open and unowned again.
- **A restructure prerequisite runs before green, in its own commit, and implements nothing.**
- **Never absorb a large restructure into a feature change or a feature stack node.**
- **Resolve records, do not delete them.** `Resolved` is the token every closed state must carry,
  because the open-items queries are all `grep -L 'Status:\*\* Resolved'`. Put the reason after the
  token, never instead of it.
- **Record a partial fix as partial.** The record stays open, gains the improved numbers, and has
  its *What would close it* narrowed to the remainder. A partial fix written up as resolved hides
  the remainder from the next Step 2b — worse than not recording it.
- **Wrap reconciles code issues; it never deletes them.** `/pr-wrap` step 7.5 and
  `/wrap-context-docs` **re-measure** every open issue in every package the PR touched — not only
  the ones the changeset names — and then resolve, narrow or reopen. Re-measuring is what makes that
  discovery safe: for a TODO "did this fix it?" is a judgement and inference is forbidden, but for a
  code issue it is a number.
  **The single delete is an analyzer false positive**, which has no history worth keeping.
  Contrast `docs/dev/todo/`, whose resolved entries **are** deleted: a TODO is an event with no
  further meaning once done, while a code issue is a standing property whose history is what makes
  a later regression legible.
- **Code issues are the one thing `/analyze-code-issues` may write into `packages/*/docs/`
  directly.** Everything else under a package's `docs/` goes through the changeset workflow in
  `docs/dev/1-WIP/` (CLAUDE.md's rule). A record is a standing measurement with its own
  reconciliation contract; routing it through a changeset would make a permanent record into a
  document that is deleted at the next wrap.
- **Refer to a record by its path in backticks, not a Markdown link.** A code issue is *renamed*
  when the symbol it names moves, and a link would rot silently where a backticked path reads as the
  identifier it is.

## What these records are **not**

| Not this | That is |
|---|---|
| the code-issue record | `/analyze-clean-code` — the LLM heuristic pass. Complementary; a code-issue file is tool-measured and carries numbers |
| the code-issue record | `coverage/` — the gitignored scratch output of `tddy-tools analyze`. That is where the measurement is produced; a record is where the verified finding *lives* |
| the TODO record | `## TODO` in a changeset — work `/green` **must** do once the implementation exists. Those are held, not deferred |
| either record | a place to record something you have not told the developer about |

## Related

**Commands**: `/plan-red`, `/plan-ft-dev`, `/plan-pr-stack`, `/add-to-pr-stack`, `/green`,
`/pr-wrap`, `/wrap-context-docs`, `/merge-pr-stack`
**Skills**: [`analyze-code-issues`](../analyze-code-issues/SKILL.md),
[`code-restructuring`](../code-restructuring/SKILL.md), [`pr-stack`](../pr-stack/SKILL.md),
[`planning`](../planning/SKILL.md)
**References**: [`planning-phase.md`](../planning/references/planning-phase.md) Step 2b
