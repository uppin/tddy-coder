---
description: Plan feature and write failing tests - PRD, changeset, acceptance tests, and red phase
---
## Plan Red - From Requirements to Failing Tests

Combines planning and test-first development into a single flow: gather requirements, create documentation, write acceptance tests, and write failing unit/integration tests.

**Prerequisites**:
- User has described the feature or change they want to implement
- Context about affected code areas (if modifying existing features)

## Execution Flow

### Planning Phase (Steps 1–5)

Follow the planning phase from `.agents/skills/planning/references/planning-phase.md` — Steps 1
through 5 (interview → code analysis → **deferred-work cross-check** → product area → PRD →
changeset).

**Step 2b is not optional, and it scans two records.** Follow
[`deferred-work/references/planning-cross-check.md`](../skills/deferred-work/references/planning-cross-check.md)
exactly — it owns the scan commands, the verdict table and the right-sizing table.

| Record | Path | One file is |
|---|---|---|
| **Code issue** | `packages/<pkg>/docs/code-issues/` | one analyzer or structural finding against one symbol or file |
| **Changeset TODO** | `docs/dev/todo/` | one thing a changeset planned and could not finish |

Classify each hit — ⛔ blocking / 🚧 claimed / ⚠ during / ℹ answered / — unrelated. Anything
blocking or constraining goes in the changeset's `## Prerequisites`, and a **blocking** item also
earns a line in `## Scope`, because it is work rather than a note. Discovering it during `/green` is
late: the choice by then is to work around it, silently make it worse, or stop.

**Reference a code issue by its path in backticks**, never a Markdown link — it is renamed when the
symbol it names moves, and a link would rot silently. A **TODO** still takes a relative link
(`[2026-08-02-slug.md](../todo/2026-08-02-slug.md)`), because `/wrap-context-docs` deletes the file
of anything marked `✅ RESOLVED HERE` and the link is the wrapping session's only memory of this
scan. A code issue marked resolved is **closed in place**, not deleted.

#### 🚧 A claimed issue — stop and ask

A code issue carrying `**Claimed by:** #NNN` has a PR **already in flight** to fix that exact code.
This repo runs its large refactors as background stacks, so that is the normal state, not an edge
case — and it gives this change a real choice that **is the developer's, never yours**:

- **proceed** on today's shape, accepting that the claiming PR must absorb your change; or
- **wait** for the named PR and plan against the shape it will leave; or
- **narrow** scope to files the claiming PR does not touch.

Verify the claim is live first (`gh pr view <N>` — a merged claim means the issue should already be
resolved), read its `**Lands after:**` to say how far out it really is, and name the **per-file**
overlap rather than the package. Present the fork in the format the reference gives and **wait**.

A single PR has one advantage over a stack here: waiting is cheap, because there is nothing above
you to strand. Say so when it is true.

#### Right-sizing the fix — single-PR shapes

If the fix is small and lives in this change's own files, do it here. If it is small but in shared
code, it is a **prerequisite PR off `master`** that lands first. If it is large, or the record says
`**Restructure:** required`, it gets its **own `Type: Refactor` changeset executed before green** —
never absorb a large refactor into a feature change, which buries a reviewable diff under a
mechanical one.

A restructure prerequisite has one contract point specific to this command: `/code-restructuring`
demands a green baseline and calls a red one a stop, but a changeset that has been through
`/plan-red` **has red tests by design**. The baseline is therefore *"green except this changeset's
own planned red tests"* — list those by name in the restructure changeset's `## Baseline` as
known-red. Anything red that is not on that list is still a stop, and a red test that **stops
failing** during the restructure has been broken, not fixed.

Check off the first two TODO items in the changeset (`Create/update PRD documentation` and `Create changeset`).

### Step 6: Create Failing Acceptance Tests

**Before writing any tests**, read `.agents/skills/fluent-tests/references/generic-guidelines.md`
and the framework-specific reference for the test type (Cypress component, Rust, etc.).
Fluent-tests is the mandatory test style — all acceptance tests must comply.

> **If you cannot write a clean failing test against the current shape, stop and say so.** That is
> the strongest signal in this repo that the ground needs softening before the red phase, and this
> is the step where it fires. If expressing the test needs a private function reached through a
> test-only `pub`, a whole service constructed to exercise one branch, or a double of a double —
> **suggest a preparatory restructure and let the developer decide**
> ([`planning-cross-check.md`](../skills/deferred-work/references/planning-cross-check.md) §
> *Softening the ground*). A test you cannot write cleanly now is a test `/green` will rewrite, and
> a red phase built on a contorted seam specifies the contortion. If the developer declines, record
> the observation — a code issue if it is measurable, a `Future enhancement` TODO if it is not — so
> it is not re-proposed as new next session.

For each acceptance test defined in changeset:
1. Write test in fluent-tests style: Given/When/Then, page-object helpers, one behavior per test
2. **CRITICAL**: Fully implement test — not a placeholder
3. Use `mountWithRpc` + `anInMemoryRpcBackend` for Cypress component tests (not `cy.intercept`)
4. Test should fail due to missing functionality
5. Verify test fails for the right reason

**MANDATORY — Present to user**:
- List of all test titles created
- File paths with line numbers
- What each test validates
- Confirmation all tests are FAILING

**USER REVIEW — Acceptance tests created — MANDATORY**
Wait for user approval before proceeding.

### Step 7: Red Phase — Write Failing Unit/Integration Tests

Use `/red` approach for smaller-scope tests (fluent-tests style is mandatory here too):
1. Write comprehensive failing tests covering:
   - Main functionality
   - Edge cases
   - Error scenarios
   - API boundaries
2. Fully implement tests (not skeletons)
3. Define public API through test usage
4. Verify all tests fail for right reasons (missing implementation, not bugs)

### Step 8: Present Results

Present complete summary:
- PRD location
- Changeset location
- All acceptance test titles + file paths
- All unit/integration test titles + file paths
- Confirmation all tests are failing correctly
- Ready for `/green` phase

## Out-of-Scope Ideas

During planning and code analysis, if you identify enhancements or improvements that are relevant but outside the current changeset scope, **add a new file** to `docs/dev/todo/` — `YYYY-MM-DD-<slug>.md`, `**Category:** Future enhancement`, `**Source:**` the current changeset name. Never append to an existing file; that is the shared append-point this layout removes.

**A measurable finding belongs in a code issue instead** — an oversized file, an untested complex
function, a cycle. Run `/analyze-code-issues` and file it under
`packages/<pkg>/docs/code-issues/`, where the numbers and the reconciliation contract live. A TODO
is for a deferral; a code issue is for a standing property of the code.

Both records are read as well as written: Step 2b scans them for items this change runs into.
The two directions are complementary — what you defer today is what somebody's Step 2b finds
tomorrow, so write entries that state **why** the work was deferred, not only what is left. That
reason is what tells the next planner whether it blocks them.

## Rules

- Each step is discrete and actionable
- Cross-check **both** records — `packages/*/docs/code-issues/` and `docs/dev/todo/` — before
  writing the changeset; record every relevant item in `## Prerequisites` with a verdict, including
  the ones you decide not to fix. TODOs take a relative link and the ✅ RESOLVED HERE ones are what
  the wrap deletes; code issues take a **backticked path** and are closed in place
- **A 🚧 claimed issue is a question, not a verdict.** Present the proceed / wait / narrow fork and
  wait for the developer. Never default to either, and record their answer in the changeset
- **A restructure prerequisite runs before green, in its own commit, and implements nothing** — with
  the baseline carved out to "green except this changeset's own planned red tests"
- Never skip user review after acceptance tests
- Never assume user approval without explicit confirmation
- Take extra time on testing strategy — don't rush
- Tests must be fully implemented, not placeholders
- Tests must follow fluent-tests style (mandatory for this repo)
- No conditional logic in tests
- No try/catch blocks or fallbacks
- Never put "red phase" or "green phase" in code comments or test descriptions
- Quality first — never compromise code quality

## Flow

```
/plan-red → /green → /pr-wrap
```

**Next**: Use `/green` to implement production-quality code making tests pass.

## Related

**Commands**: `/red`, `/green`, `/analyze-code-issues`, `/code-restructuring`
**Skills**: [`deferred-work`](../skills/deferred-work/SKILL.md)
**References**: `.agents/skills/planning/references/planning-phase.md`,
[`planning-cross-check.md`](../skills/deferred-work/references/planning-cross-check.md)
