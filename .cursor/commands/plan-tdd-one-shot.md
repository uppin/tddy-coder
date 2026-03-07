---
description: One-shot TDD planning command that creates comprehensive implementation plan in Plan mode following complete feature development lifecycle
globs:
alwaysApply: false
---

# Plan TDD One-Shot - Comprehensive Feature Development Planning

This command creates a complete, actionable implementation plan for feature development using Test-Driven Development methodology. It follows the `@plan-tdd-dev` skill and switches to Plan mode for collaborative planning.

**Prerequisites**:
- User has described the feature or change they want to implement
- Context about affected code/packages (if modifying existing features)

## Execution Flow

### Step 1: Create Initial PRD/Feature Documentation

**Before switching to Plan mode**, create the initial product requirements document:

**Actions**:
1. **MANDATORY**: Use AskQuestion tool to gather requirements:
   - **CRITICAL**: ALWAYS use the AskQuestion tool for requirements gathering
   - DO NOT skip this step or make assumptions about requirements
   - Ask about:
     - What is the user trying to achieve?
     - Is this new or modifying existing functionality?
     - Which parts of the codebase will be affected?
     - Any specific requirements or constraints?
     - Design preferences and user experience expectations
   - Present questions in organized multi-select widgets when appropriate
   - Wait for user responses before proceeding to PRD creation

2. Create PRD document:
   - Use `/plan-ft` command
   - Create in `docs/ft/1-WIP/PRD-YYYY-MM-DD-feature-name.md` if modifying existing features
   - OR create in `docs/ft/{product-area}/feature-name.md` if completely new
   - Include:
     - Summary of what's being built/changed
     - Background and motivation
     - Affected features (if PRD)
     - High-level requirements
     - Success criteria

**Outcome**: Initial PRD/Feature document created

### Step 2: Switch to Plan Mode

**After PRD is created**, switch to Plan mode for collaborative technical planning:

```
Use SwitchMode tool with target_mode_id: "plan"
Explanation: "PRD created, switching to Plan mode for collaborative technical planning and changeset creation"
```

**Why Plan Mode**:
- Collaborative discussion of technical approach
- Multiple implementation options can be explored
- Trade-offs can be discussed before coding
- User can guide technical planning decisions
- Design the changeset and milestones together

### Step 3: Create Development Plan & Changeset

**In Plan mode**, work collaboratively with the user to:

1. **Discuss technical approach**: Explore implementation options, trade-offs, and design decisions
2. **Create changeset document**: Use `/plan-ft-dev` to generate changeset in `docs/dev/1-WIP/YYYY-MM-DD-changeset.md`
3. **CRITICAL**: Include the entire Plan mode discussion/document as the first section of the changeset
   - This preserves the collaborative planning context
   - Documents why certain approaches were chosen over others
   - Captures user preferences and constraints discussed
4. **Generate detailed TODO list**: Covering all phases from planning through production readiness

Generate a detailed TODO list covering the entire development lifecycle from planning through production readiness.

## TODO List Structure

**CRITICAL**: The plan frontmatter must contain ALL workflow steps as individual TODOs (~24 items, including mandatory run of acceptance tests), not high-level phases. See the concrete example below.

### Frontmatter TODOs Array Example

When creating the plan, the `todos:` array in the frontmatter should include every step:

```yaml
todos:
  # Phase 1: Planning (Completed in Agent mode before Plan mode)
  - id: planning-prd
    content: "Create/update PRD documentation (/plan-ft)"
    status: completed
  - id: planning-changeset
    content: "Create dev doc + changeset (this plan document)"
    status: in_progress

  # Phase 2: Development (TDD Cycle)
  - id: dev-acceptance-tests
    content: "Create failing acceptance tests (/ft-dev)"
    status: pending
  - id: dev-run-acceptance-tests
    content: "Run the newly created acceptance tests (verify they fail) - MANDATORY"
    status: pending
  - id: dev-user-review-tests
    content: "USER REVIEW - Acceptance tests created (AskQuestion tool) - MANDATORY"
    status: pending
  - id: dev-tdd-red
    content: "TDD Red - Write failing unit/integration tests (/red)"
    status: pending
  - id: dev-tdd-green
    content: "TDD Green - Implement with quality code (/green)"
    status: pending
  - id: dev-update-docs
    content: "Update documentation with progress (/update-context-docs)"
    status: pending
  - id: dev-tdd-cycle-repeat
    content: "Repeat Red→Green→Update cycle until feature complete"
    status: pending
  - id: dev-run-all-tests
    content: "Run all tests (cargo test) - verify 100% pass"
    status: pending

  # Phase 3: Production Readiness (ALL steps MANDATORY)
  - id: prod-validate-changes
    content: "Validate changes (/validate-changes) - MANDATORY"
    status: pending
  - id: prod-refactor-changes
    content: "Refactor issues from change validation (refactor subagent)"
    status: pending
  - id: prod-user-review-dev
    content: "USER REVIEW - Development complete (AskQuestion tool) - MANDATORY"
    status: pending
  - id: prod-validate-tests
    content: "Validate tests (/validate-tests) - MANDATORY"
    status: pending
  - id: prod-refactor-tests
    content: "Refactor test issues (refactor subagent)"
    status: pending
  - id: prod-validate-ready
    content: "Validate production readiness (/validate-prod-ready) - MANDATORY"
    status: pending
  - id: prod-refactor-ready
    content: "Refactor production readiness issues (refactor subagent)"
    status: pending
  - id: prod-analyze-quality
    content: "Analyze code quality (/analyze-clean-code) - MANDATORY"
    status: pending
  - id: prod-refactor-quality
    content: "Refactor code quality issues (refactor subagent)"
    status: pending
  - id: prod-final-validation
    content: "Final validation (/validate-changes) - MANDATORY"
    status: pending
  - id: prod-lint-typecheck
    content: "Linting and type checking (cargo fmt, cargo check)"
    status: pending
  - id: prod-wrap-docs
    content: "Wrap documentation (/wrap-context-docs)"
    status: pending
  - id: prod-user-review-complete
    content: "USER REVIEW - Work complete, decide next steps (AskQuestion tool) - MANDATORY"
    status: pending
```

**Total**: ~24 TODOs covering complete workflow from planning through PR readiness (including mandatory run of acceptance tests).

**Key Points**:
- Each TODO is a discrete, actionable step
- User review checkpoints are separate TODOs (marked MANDATORY)
- All validation steps are separate TODOs (marked MANDATORY)
- Status tracks progress: `completed` → `in_progress` → `pending`

### Detailed Workflow Steps

The plan should include all phases from `@plan-tdd-dev` skill:

### Phase 1: Planning (Feature & Technical)

#### TODO: Create Feature Documentation
**Command**: `/plan-ft`
**Purpose**: Define product requirements
**Status**: Should be completed in Step 1 (before switching to Plan mode)

**Actions**:
- [x] Create feature document in `docs/ft/{product-area}/`
  - OR create PRD document in `docs/ft/{product-area}/1-WIP/` if modifying existing features
  - **CRITICAL**: If PRD, list ALL affected feature documents
- [x] Include aspects from `@prd-doc` rule:
  - Summary of changes
  - Background and context
  - Proposed changes (what's changing vs staying the same)
  - Impact analysis (technical + user impact)
  - Implementation plan overview
  - Acceptance criteria
  - References to affected features

**Outcome**: Feature Doc OR PRD documenting requirements (created before Plan mode)

#### TODO: Create Development Plan
**Command**: `/plan-ft-dev`
**Purpose**: Plan technical implementation
**Status**: Should be completed in Step 3 (in Plan mode)

**Actions**:
- [ ] Create dev doc in `packages/{package}/docs/`
- [ ] Create changeset doc in `docs/dev/1-WIP/YYYY-MM-DD-changeset.md`
- [ ] **CRITICAL**: Include the entire Plan mode MD document as the first section of the changeset
  - This preserves the collaborative planning discussion
  - Documents technical decisions, trade-offs, and user input
  - Provides context for why implementation approach was chosen
- [ ] Include in changeset (after Plan mode document):
  - Affected packages list
  - Implementation milestones with checkboxes
  - Acceptance tests outline
  - Testing plan
  - Technical decisions and constraints
  - TODO checklists for tracking progress

**Outcome**: Dev Doc + Changeset with complete planning context and technical roadmap

### Phase 2: Development (TDD Cycle)

#### TODO: Create Failing Acceptance Tests
**Command**: `/ft-dev`
**Purpose**: Start feature development with tests that define desired behavior

**Actions**:
- [ ] Review changeset document for acceptance tests list
- [ ] Verify working on correct branch (not master/main)
- [ ] For each acceptance test in changeset:
  - [ ] Write test that captures desired behavior
  - [ ] **CRITICAL**: The test should NOT be empty - implement it as if testing real implementation
  - [ ] Test is expected to fail due to missing functionality (GOOD!)
  - [ ] Verify test fails for the right reason
  - [ ] **CRITICAL**: If tests are passing, they should be removed (not verifying anything new)
  - [ ] Follow `@testing-practices` standards
- [ ] Create initial file structure/scaffolding
- [ ] **MANDATORY**: Run the newly created tests to confirm all acceptance tests fail
- [ ] **CRITICAL**: Present to user:
  - List of all test titles created
  - Clickable links to test file locations (format: `path/to/file.rs#LX`)
  - Brief summary of what each test validates
  - Confirmation that all tests are currently FAILING (as expected)

**Outcome**: Failing acceptance tests defining feature requirements + User review of test list

#### TODO: Run the newly created acceptance tests
**Purpose**: Verify acceptance tests exist and fail for the right reason before user review.

**MANDATORY - CANNOT SKIP.** Do not proceed to User Review without running the tests.

**Actions**:
- [ ] Run the newly created acceptance tests (e.g. `cargo test:it` or the specific test file)
- [ ] Confirm all new tests fail (expected: missing implementation)
- [ ] Confirm tests fail for the right reason (e.g. method not found, assertion on behavior), not for setup/lint errors
- [ ] If any new test passes: remove or change it (passing tests must not be empty or redundant)

**Outcome**: Evidence that acceptance tests are in place and failing as intended.

#### TODO: User Review - Acceptance Tests Created
**Purpose**: Review and approve acceptance tests before implementation
**Tool**: Use `AskQuestion` tool for structured review

**Actions**:
- [ ] Present test summary to user:
  - Total number of tests created
  - Test categories/groups
  - Clickable links to test locations
  - What each test validates
- [ ] **CRITICAL**: Use `AskQuestion` tool with options:
  - "Approve - tests cover all requirements, proceed with implementation"
  - "Request changes - tests need modifications or additions"
  - "Review manually - I want to review the test files first"
- [ ] Wait for user response via AskQuestion tool
- [ ] If changes requested: update tests accordingly and re-present for approval
- [ ] If manual review requested: wait for user to finish reviewing, then ask again
- [ ] If approved: confirm all tests are FAILING before implementation starts

**Quality Gate**: User approves test coverage via AskQuestion tool

#### TODO: Implement Feature Using TDD Red-Green Cycle
**Repeat for each milestone/component until feature is complete**

##### TODO: Red Phase - Write Failing Tests
**Command**: `/red`
**Subagent**: `test-writer`
**Uses**: `@tdd` skill

**Actions**:
- [ ] Write comprehensive failing unit/integration tests
- [ ] Test edge cases and error conditions
- [ ] Verify tests fail with clear error messages
- [ ] Ensure tests are deterministic (no conditional logic, fallbacks, or try/catch)
- [ ] Update changeset with test progress

**Quality Gate**: All new tests fail for the right reasons

##### TODO: Green Phase - Make Tests Pass
**Command**: `/green`
**Subagent**: `tdd-implementer`
**Uses**: `@tdd` skill

**Actions**:
- [ ] Implement minimal, production-quality code
- [ ] **CRITICAL**: Prioritize code quality over forcing tests to pass
- [ ] Never compromise implementation to make tests pass
- [ ] If tests don't pass with quality code, document the mismatch
- [ ] Focus on functionality and maintainability
- [ ] Update changeset with implementation progress

**Quality Gate**: Tests pass with quality implementation (or mismatch documented)

##### TODO: Update Documentation
**Command**: `/update-context-docs`

**Actions**:
- [ ] Update changeset docs with progress
- [ ] Check off completed milestones
- [ ] Document technical decisions made
- [ ] Track any technical debt discovered

**Repeat Red-Green cycle**: Continue `/red` → `/green` → `/update-context-docs` until all acceptance tests pass and feature is complete

#### TODO: Run All Tests
**Actions**:
- [ ] Run full test suite: `cargo test`
- [ ] Ensure all tests pass (unit + integration + e2e)
- [ ] Fix any broken tests in unrelated code
- [ ] Verify no test anti-patterns introduced

**Quality Gate**: All tests pass, 100% success rate

### Phase 3: Production Readiness (PR Wrap Steps)

**🚨 CRITICAL WARNING - VALIDATION STEPS ARE MANDATORY 🚨**

**ALL validation steps MUST be executed. NO EXCEPTIONS.**

**NEVER skip validation steps based on assumptions such as**:
- ❌ "Tests are passing, so validation will find nothing"
- ❌ "Linting passed, so code quality is good"
- ❌ "Type-check passed, so no issues exist"
- ❌ "Implementation looks clean, validation not needed"

**Why validation cannot be skipped**:
- Tests verify **behavior**, validation assesses **quality and safety**
- Linting checks **syntax**, validation checks **production readiness**
- Type-checking verifies **types**, validation finds **architectural issues**
- Visual inspection misses **subtle bugs and patterns** that tools catch

**Each validation tool serves a unique purpose**:
1. `/validate-changes` - Finds production threats, security issues, unsafe patterns
2. `/validate-tests` - Identifies test anti-patterns and quality issues
3. `/validate-prod-ready` - Detects mocks, workarounds, TODOs in production code
4. `/analyze-clean-code` - Analyzes complexity, duplication, maintainability

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

#### TODO: User Review - Development Complete
**Purpose**: Review implementation and initial validation results before continuing production readiness phase
**Tool**: Use `AskQuestion` tool for structured review

**🚨 CRITICAL - MANDATORY CHECKPOINT 🚨**
**This review CANNOT be skipped under any circumstances**

**Actions**:
- [ ] Present implementation summary to user:
  - All acceptance tests passing
  - Feature milestones completed
  - Test coverage overview
  - Key implementation decisions made
  - `/validate-changes` results and fixes applied
- [ ] Demo or walkthrough of implemented functionality (if applicable)
- [ ] **CRITICAL**: Use `AskQuestion` tool with options:
  - "Approve - implementation looks good, proceed with remaining validation"
  - "Request changes - implementation needs modifications"
  - "Test manually first - I want to test the feature before continuing"
  - "Review validation results - I need to examine the validation findings"
- [ ] Wait for user response via AskQuestion tool
- [ ] If changes requested: address them and re-present for approval
- [ ] If manual testing requested: wait for user to finish testing, then ask again
- [ ] If validation review requested: provide detailed validation results, then ask again
- [ ] If approved: proceed with remaining validation steps
- [ ] **NEVER proceed to remaining validation without explicit user approval via AskQuestion**

**Quality Gate**: User approves implementation and validation results via AskQuestion tool

**Why this cannot be skipped**:
- User may want to test the implementation manually
- User may have feedback that changes remaining validation priorities
- User can review `/validate-changes` results before investing in further validation
- Starting remaining validation without user sign-off wastes time if changes are needed

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
- [ ] Run `cargo fmt` to fix linting issues
- [ ] Run `cargo check` to verify types
- [ ] Run `cargo test` to confirm all tests still pass
- [ ] Fix any issues that arise

**Quality Gate**: No linting errors, no type errors, all tests pass

#### TODO: Update and Wrap Documentation
**Command**: `/wrap-context-docs`

**Actions**:
- [ ] Update changeset with final progress
- [ ] Check off all completed acceptance criteria
- [ ] If all criteria met: wrap changeset/PRD into feature docs
  - Produce clean State B documentation
  - Remove amendment metadata and change history
- [ ] If not all complete: keep changeset/PRD in 1-WIP/

**Quality Gate**: Documentation reflects current state accurately

#### TODO: User Review - Work Complete
**Purpose**: Review completed work and decide next steps
**Tool**: Use `AskQuestion` tool for next actions

**🎉 FEATURE COMPLETE - DECIDE NEXT STEPS 🎉**

**Actions**:
- [ ] Present completion summary to user:
  - All acceptance criteria met
  - All tests passing (unit + integration + e2e)
  - All validation steps completed successfully
  - Documentation updated and wrapped
  - Code is production-ready
- [ ] **CRITICAL**: Use `AskQuestion` tool with options:
  - "Create Pull Request - ready to submit for review"
  - "Manual testing - I want to test the feature end-to-end first"
  - "Continue development - I have additional changes to make"
  - "Review changes - I want to review all changes before proceeding"
  - "Done - no further action needed"
- [ ] Wait for user response via AskQuestion tool
- [ ] Handle user's choice:
  - **Create Pull Request**: Proceed with PR creation (if /pr command exists) or provide instructions
  - **Manual testing**: Wait for user to finish testing, then ask again
  - **Continue development**: Ask user what additional work is needed
  - **Review changes**: Present summary of all changes made, then ask again
  - **Done**: Confirm completion and end workflow

**Quality Gate**: User explicitly chooses next action via AskQuestion tool

**Why this checkpoint matters**:
- User may want to manually test before creating PR
- User may have discovered additional work during implementation
- User may want to review all changes holistically
- Prevents automatic PR creation without user consent
- Gives user control over when to submit for review

## Plan Template Format

**IMPORTANT**: This template shows the markdown **body structure** for readability and context. However, the frontmatter `todos:` array (shown in the example above) must contain ALL ~24 individual workflow steps (including mandatory run of acceptance tests) as separate TODO items, not the condensed groupings shown here.

The markdown body provides explanations and context, while the frontmatter TODOs enable granular progress tracking.

After gathering context, generate a plan with this markdown body structure:

```markdown
# Feature Implementation Plan: [Feature Name]

## Overview
[Brief description of what we're building and why]

## Affected Packages
- `@org/package-name` - [what changes]
- `@org/other-package` - [what changes]

## Implementation Strategy
[High-level approach and key technical decisions]

## Workflow Summary (Detailed in TODOs)

### Phase 1: Planning (COMPLETED)
- Create feature/PRD documentation (`/plan-ft`)
- Create dev doc + changeset (this plan)

### Phase 2: Development
- Create failing acceptance tests (`/ft-dev`)
- **Run the newly created acceptance tests (verify they fail) - MANDATORY**
- **User Review: Acceptance tests created** (AskQuestion tool)
- TDD Cycle (repeat as needed):
  - Red: Write failing tests (`/red`)
  - Green: Implement with quality code (`/green`)
  - Update docs (`/update-context-docs`)
- Run all tests (`cargo test`)

### Phase 3: Production Readiness (ALL MANDATORY)
- Validate changes (`/validate-changes`)
- Refactor issues (`refactor` subagent)
- **User Review: Development complete** (AskQuestion tool)
- Validate tests (`/validate-tests`)
- Refactor test issues (`refactor` subagent)
- Validate production readiness (`/validate-prod-ready`)
- Refactor production issues (`refactor` subagent)
- Analyze code quality (`/analyze-clean-code`)
- Refactor quality issues (`refactor` subagent)
- Final validation (`/validate-changes`)
- Linting and type checking
- Wrap documentation (`/wrap-context-docs`)
- **User Review: Work complete** (AskQuestion tool)

## Technical Decisions
[Document key architectural decisions made during planning]

## Risks and Mitigations
[Identify potential risks and how to address them]

## Acceptance Criteria
[List what "done" looks like for this feature]
```

**Remember**: The frontmatter `todos:` array expands the workflow summary into ~24 discrete, trackable steps.

## Best Practices

### During Initial PRD Creation (Agent Mode - Step 1)
1. **MANDATORY - Use AskQuestion tool**: Always use AskQuestion tool for requirements gathering
   - Present organized questions in multi-select widgets
   - Gather design preferences, UX expectations, and technical constraints
   - Wait for user responses before creating PRD
   - DO NOT make assumptions or skip this step
2. **Be concise**: Create initial PRD with core requirements based on user responses
3. **Focus on "what"**: Document what needs to be built, not how
4. **List affected features**: If modifying existing features, list all affected docs
5. **Keep it actionable**: Clear acceptance criteria

### During Technical Planning (Plan Mode - Steps 2-3)
1. **Ask questions**: Clarify technical ambiguities
2. **Explore options**: Discuss multiple implementation approaches
3. **Consider trade-offs**: Discuss pros/cons of each approach
4. **Be thorough**: Include all steps from planning to PR
5. **Document decisions**: Capture rationale for technical choices
6. **Create detailed changeset**: Milestones, testing plan, technical decisions

### During Execution (After Plan)
1. **Follow the plan**: Execute TODOs in order
2. **Update progress**: Check off completed items
3. **Seek user approval**: Pause at review checkpoints and **MUST use AskQuestion tool** with structured options
4. **Adapt as needed**: Update plan if new information emerges
5. **Document changes**: Track deviations and reasons
6. **Quality first**: Never compromise code quality to pass tests

### Documentation Management
1. **Keep current**: Update docs as implementation progresses
2. **Track progress**: Use checkboxes in changeset
3. **Document decisions**: Capture technical choices and rationale
4. **Link everything**: Connect feature docs ↔ dev docs ↔ PRs

## Related Commands and Skills

**Skills**:
- `@plan-tdd-dev` - Core workflow this command implements
- `@tdd` - TDD methodology for red-green-refactor cycle

**Planning Commands**:
- `/plan-ft` - Feature/PRD documentation
- `/plan-ft-dev` - Technical planning and changeset creation

**Development Commands**:
- `/ft-dev` - Initial acceptance test creation
- `/red` - Write failing tests (delegates to `test-writer`)
- `/green` - Implement with quality-first (delegates to `tdd-implementer`)
- `/update-context-docs` - Sync documentation with progress

**Production Readiness Commands**:
- `/validate-changes` - Critical change assessment
- `/validate-tests` - Test quality validation
- `/validate-prod-ready` - Production readiness check
- `/analyze-clean-code` - Code quality analysis
- `/wrap-context-docs` - Finalize documentation

**Subagents**:
- `test-writer` - Writes comprehensive failing tests
- `tdd-implementer` - Implements with quality-first approach
- `refactor` - Improves code quality and fixes issues

**Rules**:
- `@prd-doc` - PRD document structure
- `@feature-doc` - Feature documentation standards
- `@dev-doc` - Development documentation format
- `@changeset-doc` - Changeset structure and workflow
- `@testing-practices` - Test quality standards
- `@tdd` - TDD methodology
- `@coding-practices` - Code quality standards

## Success Criteria

A successful plan should:
1. ✅ Start with PRD creation in Agent mode (Step 1)
2. ✅ Switch to Plan mode for technical planning (Step 2)
3. ✅ Create comprehensive changeset in Plan mode (Step 3)
4. ✅ Include all phases: Planning → Development → Production
5. ✅ **Have ~24 detailed TODOs in frontmatter (not 5-6 high-level phases)**
6. ✅ **Include 3 user review checkpoints as separate TODOs (marked MANDATORY)**
7. ✅ **Include all 5 validation steps as separate TODOs (marked MANDATORY)**
8. ✅ **Use AskQuestion tool for all user review checkpoints**
9. ✅ Reference appropriate commands, subagents, and skills
10. ✅ Include all refactoring steps as separate TODOs
11. ✅ Emphasize quality-first TDD approach (code quality over passing tests)
12. ✅ Provide clear acceptance criteria
13. ✅ Be actionable and easy to follow

**Critical**: The frontmatter `todos:` array must match the example structure shown above (~24 items, including the mandatory run step after acceptance tests), not a condensed version.

## Review Checkpoints

**CRITICAL**: These three checkpoints MUST be included as separate TODOs in the frontmatter:

1. **After Acceptance Tests** - TODO id: `dev-user-review-tests`
   - Content: "USER REVIEW - Acceptance tests created (AskQuestion tool) - MANDATORY"

2. **After Initial Validation** - TODO id: `prod-user-review-dev`
   - Content: "USER REVIEW - Development complete (AskQuestion tool) - MANDATORY"

3. **After Work Complete** - TODO id: `prod-user-review-complete`
   - Content: "USER REVIEW - Work complete, decide next steps (AskQuestion tool) - MANDATORY"

The plan includes three user review checkpoints to ensure alignment before proceeding. **CRITICAL**: All checkpoints MUST use the `AskQuestion` tool for structured, explicit user approval.

### Checkpoint 1: After Acceptance Tests Created
**When**: After `/ft-dev` creates failing acceptance tests, before TDD implementation cycles
**Purpose**: Ensure user approves test coverage before implementation starts
**Tool**: `AskQuestion` with structured options

**What to review**:
- List of all test titles created
- Test file locations (with clickable links)
- What each test validates
- Test coverage completeness
- Confirmation tests are FAILING (as expected)

**Why this checkpoint matters**:
- Prevents implementing wrong tests
- Allows user to request additional test scenarios
- Ensures tests match user's mental model of feature
- Catches missing edge cases early
- Validates test quality before investing in implementation

**How to conduct review**:
- **MUST use `AskQuestion` tool** with options:
  - "Approve - tests cover all requirements, proceed with implementation"
  - "Request changes - tests need modifications or additions"
  - "Review manually - I want to review the test files first"
- Wait for explicit user selection via AskQuestion
- Handle each response appropriately (update tests, wait for manual review, or proceed)
- **NEVER assume approval without explicit AskQuestion response**

### Checkpoint 2: After Initial Validation
**When**: After `/validate-changes` runs and issues are fixed, before remaining production readiness validation
**Purpose**: Ensure user approves implementation and initial validation results before continuing
**Tool**: `AskQuestion` with structured options

**🚨 CRITICAL - MANDATORY CHECKPOINT 🚨**
**This checkpoint CANNOT be skipped - EVER**

**What to review**:
- Implemented functionality
- All acceptance tests passing
- Completed milestones
- Implementation decisions
- `/validate-changes` results and fixes applied

**Why this checkpoint matters**:
- Prevents wasted validation effort on wrong implementation
- Ensures alignment with user expectations
- Provides opportunity for course correction after seeing initial validation results
- Builds confidence before remaining production readiness steps
- **User may want to manually test before continuing validation**
- **User feedback may change remaining validation priorities**
- **User can review change validation results and decide if further validation is needed**

**How to conduct review**:
- **MUST use `AskQuestion` tool** with options:
  - "Approve - implementation looks good, proceed with remaining validation"
  - "Request changes - implementation needs modifications"
  - "Test manually first - I want to test the feature before continuing"
  - "Review validation results - I need to examine the validation findings"
- Wait for explicit user selection via AskQuestion
- Handle each response appropriately (make changes, wait for testing, provide details, or proceed)
- **NEVER assume approval without explicit AskQuestion response**

**Common mistakes to avoid**:
- ❌ Assuming user approval because tests pass
- ❌ Skipping review "to save time"
- ❌ Proceeding to remaining validation without explicit user confirmation
- ❌ Only asking for review after all validation is complete
- ❌ Running all validation steps before getting user feedback
- ❌ **Using plain text questions instead of AskQuestion tool**
- ❌ **Not providing structured options for user response**

### Checkpoint 3: After Work Complete
**When**: After `/wrap-context-docs` completes, at the end of the workflow
**Purpose**: Review all completed work and decide next steps
**Tool**: `AskQuestion` with structured options

**🎉 FEATURE COMPLETE - DECIDE NEXT STEPS 🎉**

**What to review**:
- All acceptance criteria met
- All tests passing (unit + integration + e2e)
- All validation steps completed successfully
- Documentation updated and wrapped
- Code is production-ready

**Why this checkpoint matters**:
- Gives user control over when to create PR
- User may want to manually test end-to-end before submitting
- User may have discovered additional work during implementation
- User may want to review all changes holistically before PR
- Prevents automatic PR creation without user consent
- Provides clear options for next steps

**How to conduct review**:
- **MUST use `AskQuestion` tool** with options:
  - "Create Pull Request - ready to submit for review"
  - "Manual testing - I want to test the feature end-to-end first"
  - "Continue development - I have additional changes to make"
  - "Review changes - I want to review all changes before proceeding"
  - "Done - no further action needed"
- Wait for explicit user selection via AskQuestion
- Handle each response appropriately:
  - **Create PR**: Proceed with PR creation or provide instructions
  - **Manual testing**: Wait for user to finish, then ask again
  - **Continue dev**: Ask what additional work is needed
  - **Review changes**: Present comprehensive summary, then ask again
  - **Done**: Confirm completion and end workflow
- **NEVER automatically create PR without user selection**

**Common mistakes to avoid**:
- ❌ Automatically creating PR after wrapping docs
- ❌ Assuming user wants PR just because work is complete
- ❌ Not offering manual testing option
- ❌ Proceeding without explicit user choice
- ❌ **Using plain text questions instead of AskQuestion tool**
- ❌ **Not providing structured options for user response**

---

**Last Revised:** 2026-02-17
