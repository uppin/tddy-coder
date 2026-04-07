---
name: plan-tdd-dev
description: Guides complete feature development workflow from planning through production using TDD methodology, documentation state management, and validation phases. Use when planning new features, driving development from conception to deployment, or when the user mentions feature development workflow, TDD feature cycle, or asks to implement a feature from start to finish.
---

# Feature Development with TDD Workflow

This skill guides you through the complete feature development lifecycle from conception to production deployment using Test-Driven Development and proper documentation management.

## Overview

The workflow has three main phases:
1. **Planning Phase** - Define requirements and technical approach
2. **Development Phase** - TDD implementation cycle
3. **Production Readiness Phase** - Validation and documentation finalization

## Phase 1: Planning

### Step 1: Feature Requirements

**Command**: `/plan-ft`

**Purpose**: Create feature documentation or amendment documents

**Actions**:
- Create feature document in `docs/ft/{product-area}/`
- OR create amendment in `docs/ft/{product-area}/amendments/` if modifying existing features
- **CRITICAL**: Amendments MUST reference ALL affected feature documents

**Outcome**: Feature Doc (State A) or Amendment Doc documenting State A → State B delta

### Step 2: Technical Planning

**Command**: `/plan-ft-dev`

**Purpose**: Plan technical implementation

**Actions**:
- Create dev doc in `packages/{package}/docs/`
- Create changeset doc with technical delta
- Define acceptance tests outline
- Create TODO checklists

**Outcome**: Dev Doc with technical plan and Changeset Doc with implementation strategy

## Phase 2: Development

### Step 1: Initial Implementation

**Command**: `/ft-dev`

**Purpose**: Start feature development with failing acceptance tests

**Actions**:
- Create initial file structure
- Write failing acceptance tests
- Set up basic scaffolding

**Outcome**: Failing acceptance tests that define feature requirements

### Step 2: TDD Cycle (Red-Green Loop)

**Repeat until all tests pass:**

#### Red Phase
**Command**: `/red`
**Subagent**: `test-writer`
**Skill**: Uses `@tdd` skill

**Actions**:
- Write comprehensive failing tests
- Test edge cases and error conditions
- Verify tests fail for the right reasons

**Quality Gate**: All new tests fail with clear error messages

#### Green Phase
**Command**: `/green`
**Subagent**: `tdd-implementer`
**Skill**: Uses `@tdd` skill (must-have)

**Actions**:
- Implement minimal code to make tests pass
- Focus on functionality over form
- Avoid over-engineering

**Quality Gate**: All tests pass

**Repeat**: Continue `/red` → `/green` until feature is complete

### Step 3: Update Documentation

**Command**: `/update-context-docs`

**Purpose**: Keep documentation synchronized with implementation progress

**Actions**:
- Update amendment documents with progress
- Update changeset docs with implementation details
- Check off completed TODOs

**Frequency**: During development as major milestones are completed

## Phase 3: Production Readiness

### Choose Your Approach

#### Option A: Quick Path

**Command**: `/pr-wrap`

**When to use**: Need faster turnaround, automated validation is sufficient

**What it does**:
- Runs all validation subagents
- Performs refactoring
- Wraps documentation
- Prepares PR

**Then**: `/pr` to create pull request

#### Option B: Thorough Path

**When to use**: Need fine-grained control, critical changes, complex features

**Validation and cleanup sequence**:

1. **`/validate-changes`** - Analyze production threats and unsafe code
2. **`/validate-tests`** - Validate test quality and deterministic behavior
3. **`/validate-prod-ready`** - Remove mocks, eliminate fallbacks, address TODOs
4. **`/analyze-clean-code`** - Analyze code quality and improvement priorities
5. **`refactor`** (subagent) - Clean up code, improve types, apply clean code principles
6. **`/wrap-context-docs`** (if amendments complete) - Apply amendments to feature docs
7. **`/pr`** - Create pull request

### Documentation Finalization

**When**: Before creating PR, after all implementation is complete

**Command**: `/wrap-context-docs`

**Prerequisites** (all must be met):
- All amendment acceptance criteria completed ✅
- All tests passing ✅
- Code is production-ready ✅

**What it does**:
- Applies amendment to original feature document
- Produces clean State B documentation
- Removes amendment metadata
- Results in final-state feature docs without delta/change language
- Prepends **one** changelog/changeset index line per [changelog-merge-hygiene.md](../../../docs/dev/guides/changelog-merge-hygiene.md) (product `##` sections; package/cross-package single-line bullets)

**Outcome**: Feature Doc transitions from State A to State B (clean, no amendment history)

## Documentation State Management

### State Transitions

```
Planning Phase:
  Feature Doc (State A) + Amendment (delta) + Dev Doc (plan)
                           ↓
Development Phase:
  Changeset Doc (technical delta) + Implementation
                           ↓
  /update-context-docs (keep docs synchronized)
                           ↓
Production Phase:
  Feature Doc (State A) + Amendment
  → /wrap-context-docs command (when criteria met)
  → Feature Doc (State B) - clean final state
```

### Key Concepts

- **State A**: Current feature state before changes
- **State B**: Desired feature state after implementation
- **Amendment**: Documents the delta (State A → State B)
- **Dev Doc**: Technical implementation plan
- **Changeset**: Technical delta with implementation details
- **wrap-context-docs**: Merges amendment into feature doc producing State B

## Workflow Decision Points

### When to Move to Next Phase

**Planning → Development**:
- ✅ Feature requirements are clear
- ✅ Technical approach is defined
- ✅ Acceptance tests are outlined
- ✅ Dev doc has TODO checklist

**Development → Production Readiness**:
- ✅ All acceptance tests pass
- ✅ `/red` → `/green` cycle is complete
- ✅ Feature functionality is complete
- ✅ Documentation is updated with progress

**Production Readiness → PR**:
- ✅ All validation checks pass
- ✅ Code quality is production-ready
- ✅ Tests are deterministic and reliable
- ✅ Documentation is finalized (if amendments exist)

### When to Use Quick vs Thorough Path

**Use Quick Path** (`/pr-wrap`) when:
- Feature is straightforward
- Changes are low-risk
- Time is critical
- Automated validation is sufficient

**Use Thorough Path** (individual subagents) when:
- Feature is complex or critical
- Need to review each validation step
- Changes touch sensitive areas
- Want fine-grained control over each phase

## Common Patterns

### New Feature from Scratch

```
1. /plan-ft → Create feature doc
2. /plan-ft-dev → Create dev doc + acceptance tests
3. /ft-dev → Initial implementation with failing tests
4. /red → Write detailed unit tests
5. /green → Implement to pass tests
6. Repeat /red → /green until complete
7. /update-context-docs → Update progress
8. /pr-wrap → Quick validation and wrap
9. /pr → Create pull request
```

### Amending Existing Feature

```
1. /plan-ft → Create amendment doc (reference all affected features)
2. /plan-ft-dev → Create dev doc with technical changes
3. /ft-dev → Start implementation with failing tests
4. /red → /green cycle for new behavior
5. /update-context-docs → Update amendment with progress
6. /validate-changes → Assess impact
7. /validate-tests → Check test quality
8. /validate-prod-ready → Ensure production standards
9. /analyze-clean-code → Check code quality
10. refactor (subagent) → Clean up implementation
11. /wrap-context-docs → Apply amendment to feature docs (produces clean State B)
12. /pr → Create pull request
```

## Quality Gates Checklist

### Planning Complete
- [ ] Feature requirements documented
- [ ] Technical approach defined
- [ ] Acceptance tests outlined
- [ ] Dev doc with TODO checklist created

### Development Complete
- [ ] All acceptance tests pass
- [ ] Unit tests cover edge cases
- [ ] `/red` → `/green` cycle finished
- [ ] Documentation updated with progress

### Production Ready
- [ ] All validation checks pass
- [ ] No test skips or workarounds
- [ ] No mocks or fallbacks in production code
- [ ] Rust types are proper (no `any`)
- [ ] Code quality meets standards
- [ ] Documentation finalized (if amendments)

## Tips for Success

1. **Don't skip planning** - Time spent in planning saves debugging time
2. **Write tests first** - Let failing tests guide implementation
3. **Update docs frequently** - Don't wait until the end
4. **Run validation early** - Catch issues before they compound
5. **Choose path based on complexity** - Quick for simple, thorough for complex
6. **Trust the process** - Each phase has a purpose

## When to Seek User Guidance

Ask the user if:
- Requirements are unclear during planning
- Tests reveal design issues during development
- Validation finds critical issues
- Unsure whether to use quick or thorough path
- Amendment criteria not met but need to proceed

## Related Components

**Commands**: `/plan-ft`, `/plan-ft-dev`, `/ft-dev`, `/red`, `/green`, `/update-context-docs`, `/pr-wrap`, `/pr`

**Subagents**: `test-writer`, `tdd-implementer`, `refactor`

**Commands**: `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`, `/wrap-context-docs`

**Skills**: `@tdd` (used by test-writer and tdd-implementer)

**Rules**: `@feature-doc`, `@dev-doc`, `@amendment-doc`, `@changeset-doc`, `@tdd`, `@testing-practices`
