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

Follow the planning phase from `.agents/skills/planning/references/planning-phase.md` — Steps 1 through 5 (interview → code analysis → product area → PRD → changeset).

Check off the first two TODO items in the changeset (`Create/update PRD documentation` and `Create changeset`).

### Step 6: Create Failing Acceptance Tests

For each acceptance test defined in changeset:
1. Write test that captures desired behavior
2. **CRITICAL**: Fully implement test — not a placeholder
3. Test should fail due to missing functionality
4. Verify test fails for the right reason

**MANDATORY — Present to user**:
- List of all test titles created
- File paths with line numbers
- What each test validates
- Confirmation all tests are FAILING

**USER REVIEW — Acceptance tests created — MANDATORY**
Wait for user approval before proceeding.

### Step 7: Red Phase — Write Failing Unit/Integration Tests

Use `/red` approach for smaller-scope tests:
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

During planning and code analysis, if you identify enhancements or improvements that are relevant but outside the current changeset scope, add them to `docs/dev/TODO.md` under **Future Enhancements** with source set to the current changeset name.

## Rules

- Each step is discrete and actionable
- Never skip user review after acceptance tests
- Never assume user approval without explicit confirmation
- Take extra time on testing strategy — don't rush
- Tests must be fully implemented, not placeholders
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
