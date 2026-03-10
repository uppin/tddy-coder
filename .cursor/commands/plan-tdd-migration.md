---
description: Plan and execute code migrations (refactoring, architecture changes) without changing features, using collaborative planning in Plan mode to create comprehensive changeset
globs:
alwaysApply: false
---

# Plan TDD Migration - Code Migration Without Feature Changes

This command creates a complete, actionable implementation plan for code migrations, refactoring, and architecture changes that **do not modify features**. It focuses on technical improvements while maintaining existing behavior.

**When to Use This Command**:

- Refactoring code without changing features
- Migrating to new architecture patterns
- Updating dependencies or frameworks
- Improving code quality or performance
- Changing implementation without changing behavior

**Prerequisites**:

- User has described the migration/refactoring they want to perform
- Context about affected code/packages
- Clear understanding that **features must remain unchanged**

## Execution Flow

### Step 1: Gather Migration Context

**Before switching to Plan mode**, gather information about the migration:

**Actions**:

1. **MANDATORY**: Use AskQuestion tool to gather migration details:
   - **CRITICAL**: ALWAYS use the AskQuestion tool for requirements gathering
   - DO NOT skip this step or make assumptions about the migration
   - Ask about:
     - What code/architecture needs to change?
     - What is the motivation for this migration?
     - Which packages/components are affected?
     - What technical constraints exist?
     - What is the target architecture/pattern?
     - Are there any breaking changes to internal APIs?
   - Present questions in organized multi-select widgets when appropriate
   - Wait for user responses before proceeding

2. **MANDATORY**: Run tests for affected packages to establish pre-existing baseline:
   ```bash
   # Run tests ONLY for packages being migrated
   cd packages/package-being-migrated
   yarn test

   # Or if using workspace commands
   yarn workspace @wix/package-being-migrated test

   # DO NOT run full test suite - too broad and slow
   # Focus on packages you're planning to refactor
   ```

   **Document results PER AFFECTED PACKAGE**:
   - Package names tested
   - Total test count per package (passing/failing/skipped)
   - Any pre-existing test failures in packages being migrated
   - Test execution time per package
   - Any pre-existing warnings or issues in affected packages

   **Why this matters**:
   - Establishes clean baseline vs. new issues in migration scope
   - Prevents confusion ("did migration break this?")
   - Enables accurate validation later
   - User needs to know about existing issues in areas being modified
   - Critical for behavior preservation verification
   - Faster execution (minutes vs. hours)
   - Clear scope boundaries for migration

3. Document initial migration context:
   - Brief summary of what's changing (code/architecture, not features)
   - Motivation (performance, maintainability, modernization, etc.)
   - Affected packages and components
   - High-level technical approach

**Outcome**: Clear understanding of migration scope and approach + Pre-existing test baseline established

**Example of good baseline documentation for migration**:
```
Pre-existing test baseline (packages being migrated):
- @wix/old-parser: 123 passing, 3 failing
- @wix/new-parser: 89 passing, 0 failing ✅

Pre-existing failures in @wix/old-parser:
1. "should handle malformed input gracefully"
   - Error: TypeError: Cannot read property 'length' of undefined
   - Location: test/parser.test.ts:67
   - Cause: Known bug in legacy code
   - Tracking: Issue #234

2. "should parse large files efficiently"
   - Error: Timeout exceeded (10000ms)
   - Location: test/performance.test.ts:45
   - Cause: Performance issue in old implementation
   - Tracking: Issue #567

3. "should validate input schema correctly"
   - Error: AssertionError: expected true but got false
   - Location: test/validation.test.ts:123
   - Cause: Missing validation logic
   - Tracking: Issue #890

Note: Migration should preserve these test results (failing tests
should still fail for same reasons after migration).
```

### Step 2: Switch to Plan Mode

**After gathering context**, switch to Plan mode for collaborative technical planning:

```
Use SwitchMode tool with target_mode_id: "plan"
Explanation: "Migration context gathered, switching to Plan mode for collaborative technical planning and changeset creation"
```

**Why Plan Mode**:

- Collaborative discussion of migration strategy
- Multiple refactoring approaches can be explored
- Trade-offs can be discussed before implementation
- User can guide technical decisions
- Design the changeset and milestones together
- Ensure feature behavior preservation strategy is clear

### Step 3: Create Migration Changeset

**In Plan mode**, work collaboratively with the user to:

1. **Discuss migration strategy**: Explore approaches, trade-offs, and implementation steps
2. **Create changeset document**: Use `/plan-ft-dev` to generate changeset in `docs/dev/WIP-1/YYYY-MM-DD-migration-name.md`
3. **CRITICAL**: Include the entire Plan mode discussion/document as the first section of the changeset
   - This preserves the collaborative planning context
   - Documents why certain approaches were chosen over others
   - Captures technical decisions and constraints discussed
4. **Document pre-existing test baseline** in changeset:
   - Add "Pre-existing Baseline" section with test results captured in Step 1
   - Document results PER AFFECTED PACKAGE (not full suite)
   - List any pre-existing test failures with details (name, error, cause)
   - List any pre-existing warnings or issues
   - Note that only packages being migrated were tested (scope boundary)
   - Present this to user in chat so they're aware of existing issues
   - Clarify these issues existed BEFORE the migration
   - Critical for behavior preservation: we can verify these same tests pass after migration
5. **Generate detailed TODO list**: Covering all phases from planning through production readiness
6. **Define behavior preservation tests**: Ensure existing features remain unchanged

Generate a detailed TODO list covering the entire migration lifecycle.

## TODO List Structure

The plan should include all phases for migration:

### Phase 1: Planning (Technical Only - No Feature Changes)

#### TODO: Create Migration Changeset

**Command**: `/plan-ft-dev`
**Purpose**: Plan technical migration without changing features

**Actions**:

- [ ] Create changeset doc in `docs/dev/WIP-1/YYYY-MM-DD-migration-name.md`
- [ ] **CRITICAL**: Include the entire Plan mode MD document as the first section
  - This preserves the collaborative planning discussion
  - Documents technical decisions, trade-offs, and user input
  - Provides context for why migration approach was chosen
- [ ] Include in changeset:
  - **Affected Packages**: List ALL packages being migrated
  - **State A (Current)**: Describe current code/architecture
  - **State B (Target)**: Describe target code/architecture
  - **Migration Strategy**: Step-by-step technical approach
  - **Behavior Preservation**: How to ensure features don't change
  - **Implementation Milestones**: Checkboxes for tracking progress
  - **Testing Plan**: How to verify behavior is preserved
  - **Rollback Strategy**: How to revert if issues arise
  - **Technical Decisions**: Document key choices made
  - **TODO Checklists**: Track migration progress

**Outcome**: Comprehensive changeset with migration plan and context

**Note**: No feature documentation needed - this is internal refactoring only

### Phase 2: Development (TDD Cycle for Migration)

#### TODO: Create Behavior Preservation Tests

**Command**: `/ft-dev`
**Purpose**: Write tests that verify existing behavior before migration

**CRITICAL PRINCIPLE**: Write tests against **current implementation** to lock in behavior

**Actions**:

- [ ] Review changeset for list of behaviors to preserve
- [ ] Verify working on correct branch (not master/main)
- [ ] For each component being migrated:
  - [ ] Write tests against **current implementation** (State A)
  - [ ] Tests should capture all existing behavior
  - [ ] Tests should focus on **external behavior**, not implementation details
  - [ ] All tests should **PASS** with current code (verifying State A)
  - [ ] Follow `@testing-practices` standards
- [ ] Run tests to confirm all tests pass with current code
- [ ] **CRITICAL**: Present to user:
  - List of all test titles created
  - Clickable links to test file locations (format: `path/to/file.test.ts#LX`)
  - Brief summary of what behavior each test preserves
  - Confirmation that all tests are currently PASSING (locking in State A)

**Outcome**: Comprehensive tests that pass with current code, locking in behavior

#### TODO: User Review - Behavior Preservation Tests

**Purpose**: Review and approve tests before migration

**Actions**:

- [ ] Present test summary to user:
  - Total number of tests created
  - Test categories/groups
  - Clickable links to test locations
  - What behavior each test preserves
- [ ] Ask user: "Please review the behavior preservation tests. Do they adequately capture existing functionality? Should I add tests for any other scenarios?"
- [ ] Wait for user feedback
- [ ] If changes requested: update tests accordingly
- [ ] If approved: proceed to migration implementation
- [ ] Confirm all tests are PASSING before migration starts

**Quality Gate**: User approves test coverage and confirms tests adequately preserve behavior

#### TODO: Implement Migration Using TDD

**Migration TDD Pattern**:

1. Tests are already passing (preserve behavior)
2. Refactor code (change implementation)
3. Tests should still pass (behavior preserved)
4. If tests fail: implementation change broke behavior (fix it)

##### TODO: Migration Cycle - Implement Changes

**Command**: `/green`
**Subagent**: `tdd-implementer`
**Uses**: `@tdd` skill

**Actions**:

- [ ] Implement migration changes incrementally
- [ ] **CRITICAL**: Keep all tests passing throughout migration
- [ ] If tests fail, fix implementation (behavior must be preserved)
- [ ] Focus on code quality and maintainability
- [ ] Update changeset with migration progress
- [ ] Check off completed milestones

**Quality Gate**: All tests pass after each migration step (behavior preserved)

##### TODO: Add New Tests for Improved Implementation

**Command**: `/red` (optional)
**Purpose**: Add tests for new implementation details (if needed)

**Actions**:

- [ ] Write tests for new implementation patterns (if applicable)
- [ ] Test new internal APIs (if exposed)
- [ ] Verify tests fail before implementation
- [ ] Implement to make tests pass
- [ ] Update changeset with test additions

**Note**: This step is optional - only needed if migration introduces new testable behaviors

##### TODO: Update Documentation

**Command**: `/update-context-docs`

**Actions**:

- [ ] Update changeset docs with migration progress
- [ ] Check off completed milestones
- [ ] Document technical decisions made during implementation
- [ ] Track any technical debt discovered
- [ ] Update dev docs with new architecture patterns

**Repeat Migration cycle**: Continue implementing, testing, documenting until migration is complete

#### TODO: Run All Tests

**Actions**:

- [ ] Run full test suite: `yarn test`
- [ ] Ensure all tests pass (unit + integration + e2e)
- [ ] **CRITICAL**: Verify behavior preservation tests still pass
- [ ] Fix any broken tests in unrelated code
- [ ] Verify no test anti-patterns introduced

**Quality Gate**: All tests pass, 100% success rate, behavior preserved

#### TODO: User Review - Migration Complete

**Purpose**: Review implementation before production readiness phase

**🚨 CRITICAL - MANDATORY CHECKPOINT 🚨**
**This review CANNOT be skipped under any circumstances**

**Actions**:

- [ ] Present migration summary to user:
  - All behavior preservation tests passing
  - Migration milestones completed
  - Code quality improvements achieved
  - Key technical decisions made
- [ ] Demo or walkthrough of migrated code (if applicable)
- [ ] Ask user: "Migration phase is complete. Please review the changes before we proceed with production readiness validation."
- [ ] Wait for user approval or feedback
- [ ] If issues identified: address them before validation
- [ ] Get explicit confirmation to proceed with validation
- [ ] **NEVER proceed to Phase 3 without explicit user approval**

**Quality Gate**: User approves migration and authorizes production readiness phase

### Phase 3: Production Readiness (PR Wrap Steps)

**🚨 CRITICAL WARNING - VALIDATION STEPS ARE MANDATORY 🚨**

**ALL validation steps MUST be executed. NO EXCEPTIONS.**

Execute these steps in order to ensure code is production-ready:

#### TODO: Validate Changes

**Command**: `/validate-changes`

**🚨 MANDATORY - CANNOT SKIP 🚨**

**Actions**:

- [ ] Run `/validate-changes` to analyze code for:
  - Production threats
  - Testing infrastructure risks
  - Security vulnerabilities
  - Unsafe code patterns
- [ ] Review validation results in changeset
- [ ] Update changeset "Validation Results" section
- [ ] **NEVER skip this step regardless of test/lint status**

**Outcome**: List of issues to address

#### TODO: Refactor Issues from Change Validation

**Subagent**: `refactor`

**Actions**:

- [ ] Invoke `refactor` subagent to fix issues found in change validation
- [ ] Address production threats and unsafe patterns
- [ ] Verify fixes don't break tests
- [ ] Update changeset with fixes applied

**Quality Gate**: All critical risks resolved

#### TODO: Validate Tests

**Command**: `/validate-tests`

**🚨 MANDATORY - CANNOT SKIP 🚨**

**Actions**:

- [ ] Run `/validate-tests` to check:
  - Test anti-patterns (conditional logic, fallbacks, try/catch)
  - Deterministic behavior
  - Test-specific code branches in production code
  - Test quality standards
- [ ] Review test validation results
- [ ] Update changeset "Validation Results" section
- [ ] **NEVER skip this step - passing tests don't guarantee test quality**

**Outcome**: List of test quality issues

#### TODO: Refactor Test Issues

**Subagent**: `refactor`

**Actions**:

- [ ] Invoke `refactor` subagent to fix test issues
- [ ] Remove test anti-patterns
- [ ] Ensure deterministic test behavior
- [ ] Verify all tests still pass
- [ ] Update changeset with fixes applied

**Quality Gate**: All tests meet quality standards

#### TODO: Validate Production Readiness

**Command**: `/validate-prod-ready`

**🚨 MANDATORY - CANNOT SKIP 🚨**

**Actions**:

- [ ] Run `/validate-prod-ready` to check:
  - Mock code in production
  - TODO/FIXME markers
  - Unused code and imports
  - Test-specific branches in production code
  - Fallback logic without consent
- [ ] Review production readiness results
- [ ] Update changeset "Validation Results" section
- [ ] **NEVER skip this step - it catches production-specific issues**

**Outcome**: List of production readiness issues

#### TODO: Refactor Production Readiness Issues

**Subagent**: `refactor`

**Actions**:

- [ ] Invoke `refactor` subagent to fix production readiness issues
- [ ] Remove mocks and workarounds
- [ ] Address TODO/FIXME markers
- [ ] Clean up unused code
- [ ] Verify all tests still pass
- [ ] Update changeset with fixes applied

**Quality Gate**: Code is production-ready, no mocks or workarounds

#### TODO: Analyze Code Quality

**Command**: `/analyze-clean-code`

**🚨 MANDATORY - CANNOT SKIP 🚨**

**Actions**:

- [ ] Run `/analyze-clean-code` to analyze:
  - Function length and complexity
  - Nesting depth
  - Parameter count
  - Magic numbers
  - Code duplication
- [ ] Review code quality metrics
- [ ] Update changeset "Validation Results" section
- [ ] **NEVER skip this step - clean code principles matter**

**Outcome**: Code quality improvement recommendations

#### TODO: Refactor Code Quality Issues

**Subagent**: `refactor`

**Actions**:

- [ ] Invoke `refactor` subagent for code quality improvements
- [ ] Apply clean code principles
- [ ] Reduce complexity where needed
- [ ] Verify all tests still pass
- [ ] Update changeset with improvements applied

**Quality Gate**: Code meets clean code standards

#### TODO: Final Validation

**Command**: `/validate-changes`

**🚨 MANDATORY - CANNOT SKIP 🚨**

**Actions**:

- [ ] Re-run `/validate-changes` after all refactoring
- [ ] Ensure no new issues introduced during refactoring
- [ ] Confirm all previous issues resolved
- [ ] Update changeset with final validation status
- [ ] **NEVER skip this step - verify refactoring didn't introduce new issues**

**Quality Gate**: Clean final validation, ready for PR

#### TODO: Linting and Type Checking

**Actions**:

- [ ] Run `yarn lint:fix` to fix linting issues
- [ ] Run `yarn type-check` to verify TypeScript types
- [ ] Run `yarn test` to confirm all tests still pass
- [ ] Fix any issues that arise

**Quality Gate**: No linting errors, no type errors, all tests pass

#### TODO: Update and Wrap Documentation

**Command**: `/wrap-context-docs`

**Actions**:

- [ ] Update changeset with final migration status
- [ ] Check off all completed milestones
- [ ] If migration complete: wrap changeset into dev docs
  - Apply changeset to affected package READMEs and dev docs
  - Archive changeset to `docs/dev/changesets/1-ARCHIVE/`
  - Update package changesets.md history
- [ ] Update dev docs to reflect State B architecture

**Quality Gate**: Documentation reflects migrated state accurately

#### TODO: Create Pull Request

**Command**: `/pr`

**Actions**:

- [ ] Create PR with migration changes
- [ ] Include summary of what was migrated
- [ ] Link to archived changeset for context
- [ ] Highlight that features remain unchanged
- [ ] List validation results

**Outcome**: PR ready for review

---

After gathering context, generate a plan in this format:

```markdown
# Migration Implementation Plan: [Migration Name]

## Overview

[Brief description of what we're migrating and why]

## Affected Packages

- `@wix/package-name` - [what's changing technically]
- `@wix/other-package` - [what's changing technically]

## Migration Strategy

### State A (Current)

[Describe current code/architecture]

### State B (Target)

[Describe target code/architecture]

### Migration Approach

[High-level steps and key technical decisions]

### Behavior Preservation Strategy

[How to ensure features remain unchanged]

## TODO List

### Phase 1: Planning

- [ ] Create migration changeset (`/plan-ft-dev`)

### Phase 2: Migration Implementation

- [ ] Create behavior preservation tests (`/ft-dev`)
- [ ] **User Review: Tests created - confirm coverage**
- [ ] Migration Cycle:
  - [ ] Implement migration changes (`/green`)
  - [ ] Keep tests passing (behavior preserved)
  - [ ] Add new tests if needed (`/red`)
  - [ ] Update docs (`/update-context-docs`)
  - [ ] Repeat until migration complete
- [ ] Run all tests (`yarn test`)
- [ ] **User Review: Migration complete - get approval for validation**

### Phase 3: Production Readiness

- [ ] Validate changes (`/validate-changes`)
- [ ] Refactor issues from change validation (`refactor` subagent)
- [ ] Validate tests (`/validate-tests`)
- [ ] Refactor test issues (`refactor` subagent)
- [ ] Validate production readiness (`/validate-prod-ready`)
- [ ] Refactor production readiness issues (`refactor` subagent)
- [ ] Analyze code quality (`/analyze-clean-code`)
- [ ] Refactor code quality issues (`refactor` subagent)
- [ ] Final validation (`/validate-changes`)
- [ ] Linting and type checking (`yarn lint:fix`, `yarn type-check`, `yarn test`)
- [ ] Wrap documentation (`/wrap-context-docs`)
- [ ] Create PR (`/pr`)

## Technical Decisions

[Document key architectural decisions made during planning]

## Risks and Mitigations

[Identify potential risks and how to address them]

## Rollback Strategy

[How to revert if issues arise]

## Success Criteria

[What "done" looks like for this migration - all tests pass, behavior preserved, code quality improved]
```

## Best Practices

### During Context Gathering (Step 1)

1. **MANDATORY - Use AskQuestion tool**: Always use AskQuestion tool for gathering migration details
   - Present organized questions in multi-select widgets
   - Gather technical constraints and target architecture details
   - Wait for user responses before planning
   - DO NOT make assumptions or skip this step
2. **Understand current state**: Get clear picture of State A (current code/architecture)
3. **Define target state**: Understand State B (desired code/architecture)
4. **Identify affected packages**: List all packages that will change
5. **Clarify constraints**: Technical limitations, backward compatibility requirements

### During Technical Planning (Plan Mode - Steps 2-3)

1. **Ask questions**: Clarify technical ambiguities
2. **Explore migration paths**: Discuss multiple refactoring approaches
3. **Consider trade-offs**: Discuss pros/cons of each approach
4. **Plan incrementally**: Break migration into safe, testable steps
5. **Document decisions**: Capture rationale for technical choices
6. **Define rollback**: Plan how to revert if issues arise
7. **Behavior preservation**: Define strategy to ensure features don't change

### During Execution (After Plan)

1. **Test first**: Write behavior preservation tests before changing code
2. **Keep tests passing**: Tests should pass throughout migration
3. **Migrate incrementally**: Small steps, verify at each step
4. **Seek user approval**: Pause at review checkpoints for user feedback
5. **Document progress**: Update changeset as migration proceeds
6. **Quality first**: Use migration as opportunity to improve code quality

### Behavior Preservation Strategy

1. **Write tests against current code**: Lock in existing behavior
2. **Focus on external behavior**: Test what, not how
3. **Keep tests passing**: If tests fail, fix implementation
4. **Add integration tests**: Verify end-to-end behavior preserved
5. **Manual testing**: User should test critical paths after migration

## Related Commands and Skills

**Skills**:

- `@plan-tdd-dev` - Core workflow adapted for migrations
- `@tdd` - TDD methodology for migration implementation

**Planning Commands**:

- `/plan-ft-dev` - Technical planning and changeset creation (used for migration)

**Development Commands**:

- `/ft-dev` - Create behavior preservation tests
- `/red` - Write additional tests (optional)
- `/green` - Implement migration changes
- `/update-context-docs` - Sync documentation with progress

**Production Readiness Commands**:

- `/validate-changes` - Critical change assessment
- `/validate-tests` - Test quality validation
- `/validate-prod-ready` - Production readiness check
- `/analyze-clean-code` - Code quality analysis
- `/wrap-context-docs` - Finalize documentation
- `/pr` - Create pull request

**Subagents**:

- `tdd-implementer` - Implements migration with quality-first approach
- `refactor` - Improves code quality and fixes issues

**Rules**:

- `@dev-doc` - Development documentation format
- `@changeset-doc` - Changeset structure and workflow
- `@testing-practices` - Test quality standards
- `@tdd` - TDD methodology
- `@coding-practices` - Code quality standards

## Success Criteria

A successful migration plan should:

1. ✅ Gather migration context using AskQuestion tool (Step 1)
2. ✅ Switch to Plan mode for technical planning (Step 2)
3. ✅ Create comprehensive changeset in Plan mode (Step 3)
4. ✅ Include all phases: Planning → Migration → Production
5. ✅ Have detailed TODOs with clear actions and commands
6. ✅ Define behavior preservation strategy
7. ✅ Include user review checkpoints: after tests and before validation
8. ✅ Reference appropriate commands, subagents, and skills
9. ✅ Include all PR wrap steps as individual TODOs
10. ✅ Emphasize behavior preservation throughout
11. ✅ Be actionable and easy to follow

## Key Differences from Feature Development

### What's Different

- ❌ **No feature documentation** - features don't change
- ✅ **Behavior preservation tests** - write tests against current code first
- ✅ **Tests should pass** - unlike feature dev, tests pass before implementation
- ✅ **Focus on refactoring** - improve code without changing behavior
- ✅ **State A → State B** - describe code/architecture transition, not feature transition

### What's the Same

- ✅ **Plan mode collaboration** - still plan in Plan mode
- ✅ **Changeset tracking** - still use changeset for progress
- ✅ **TDD cycle** - still use `/green` for implementation
- ✅ **Validation steps** - all validation steps still mandatory
- ✅ **User review checkpoints** - still require user approval

## Review Checkpoints

### Checkpoint 1: After Behavior Preservation Tests Created

**When**: After `/ft-dev` creates tests, before migration implementation
**Purpose**: Ensure user approves test coverage before changing code

**What to review**:

- List of all test titles created
- Test file locations (with clickable links)
- What behavior each test preserves
- Test coverage completeness
- Confirmation tests are PASSING (locking in current behavior)

**Why this checkpoint matters**:

- Prevents migrating without adequate safety net
- Allows user to request additional test scenarios
- Ensures tests capture all critical behaviors
- Catches missing edge cases early

**User Actions**:

- Review test list and titles
- Confirm tests adequately preserve behavior
- Request changes/additions if needed
- Approve to proceed with migration

### Checkpoint 2: Before Validation Phase

**When**: After migration complete, before production readiness validation
**Purpose**: Ensure user approves migration before starting validation

**🚨 CRITICAL - MANDATORY CHECKPOINT 🚨**
**This checkpoint CANNOT be skipped - EVER**

**What to review**:

- Migrated code structure
- All behavior preservation tests passing
- Completed milestones
- Technical decisions made

**Why this checkpoint matters**:

- Prevents wasted validation effort on wrong migration
- Ensures alignment with user expectations
- Provides opportunity for course correction
- Builds confidence before production readiness

**User Actions**:

- Review migrated code
- Verify behavior is preserved
- Confirm migration meets expectations
- Approve to proceed with validation

---

**Last Revised:** 2026-02-14
