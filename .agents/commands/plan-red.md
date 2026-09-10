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

Follow the planning phase from `.agents/skills/planning/references/planning-phase.md` — Steps 1 through 5 (interview → code analysis → **TODO cross-check** → product area → PRD → changeset).

**Step 2b is not optional.** After the code analysis, scan `docs/dev/todo/` for items sitting in
this change's path and classify each — blocking / during / answered / resolved-here / unrelated.
Anything blocking or constraining goes in the changeset's `## Prerequisites`, and a **blocking** item
also earns a line in `## Scope`, because it is work rather than a note. Discovering it during
`/green` is late: the choice by then is to work around it, silently make it worse, or stop.

**Record each item with a relative link to its file** —
`[2026-08-02-slug.md](../todo/2026-08-02-slug.md)`. An item this change **fixes** is marked
`✅ RESOLVED HERE`, and `/wrap-context-docs` **deletes that file** when the changeset wraps, so the
backlog does not keep describing a defect that is gone. The links are the wrapping session's only
memory of this scan.

If the fix is small and lives in this change's own files, do it here. If it is small but in shared
code, it is a prerequisite PR that lands first. If it is large, it gets its own PR — never absorb a
large refactor into a feature change, which buries a reviewable diff under a mechanical one.

Check off the first two TODO items in the changeset (`Create/update PRD documentation` and `Create changeset`).

### Step 6: Create Failing Acceptance Tests

**Before writing any tests**, read `.agents/skills/fluent-tests/references/generic-guidelines.md`
and the framework-specific reference for the test type (Cypress component, Rust, etc.).
Fluent-tests is the mandatory test style — all acceptance tests must comply.

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

`docs/dev/todo/` is read as well as written: Step 2b scans it for items this change runs into.
The two directions are complementary — what you defer today is what somebody's Step 2b finds
tomorrow, so write entries that state **why** the work was deferred, not only what is left. That
reason is what tells the next planner whether it blocks them.

## Rules

- Each step is discrete and actionable
- Cross-check `docs/dev/todo/` before writing the changeset; record every relevant item in
  `## Prerequisites` with a verdict **and a link to its file**, including the ones you decide not to
  fix — the ones marked ✅ RESOLVED HERE are what the wrap deletes from the backlog
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

**Commands**: `/red`, `/green`
**References**: `.agents/skills/planning/references/planning-phase.md`
