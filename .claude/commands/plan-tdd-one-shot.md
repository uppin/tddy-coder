# Plan TDD One-Shot

Complete feature development from planning through production readiness. This is the most comprehensive development command.

## Step 1: Gather Requirements

Ask the user what feature they want to build. Collect enough context to write a PRD. Then create the PRD document following the `/plan-ft` process.

## Step 2: Collaborative Planning

Use the EnterPlanMode tool to switch to plan mode for collaborative planning with the user. Discuss:
- Architecture decisions
- Implementation approach
- Testing strategy
- Risk areas and trade-offs

Stay in plan mode until the user approves the plan.

## Step 3: Create Changeset

Create a development changeset following the `/plan-ft-dev` process. Include the plan discussion summary as the first section of the changeset.

## Step 4: Generate TODO List

Generate approximately 24 detailed TODOs organized into three phases:

### Phase 1: Planning (2 TODOs)

- [ ] **TODO-01**: Create PRD document
- [ ] **TODO-02**: Create changeset with development plan

### Phase 2: Development / TDD Cycle (8 TODOs)

For each milestone in the changeset:
- [ ] **TODO-03**: Write acceptance tests (tests should FAIL initially)
- [ ] **TODO-04**: Implement minimum code to pass first test
- [ ] **TODO-05**: Continue TDD cycle for milestone 1
- [ ] **TODO-06**: **CHECKPOINT: Ask the user to review acceptance tests**
- [ ] **TODO-07**: TDD cycle for milestone 2
- [ ] **TODO-08**: TDD cycle for milestone 3
- [ ] **TODO-09**: **CHECKPOINT: Ask the user to review initial validation**
- [ ] **TODO-10**: Integration and refinement

### Phase 3: Production Readiness (14 MANDATORY Validation Steps)

Every single one of these steps is MANDATORY. Do not skip any.

- [ ] **TODO-11**: `validate-changes` - Review all changed files for correctness
- [ ] **TODO-12**: Use the Agent tool to refactor issues found in TODO-11
- [ ] **TODO-13**: `validate-tests` - Run full test suite, verify all tests pass
- [ ] **TODO-14**: Use the Agent tool to fix any failing tests from TODO-13
- [ ] **TODO-15**: `validate-tests` - Re-run tests after fixes
- [ ] **TODO-16**: `validate-prod-ready` - Check for TODO/FIXME annotations, debug code, hardcoded values
- [ ] **TODO-17**: Use the Agent tool to address issues found in TODO-16
- [ ] **TODO-18**: `analyze-clean-code` - Check code style, naming, structure
- [ ] **TODO-19**: Use the Agent tool to apply clean code improvements from TODO-18
- [ ] **TODO-20**: Run `cargo clippy -- -D warnings` and fix all warnings
- [ ] **TODO-21**: Run `cargo fmt` to ensure consistent formatting
- [ ] **TODO-22**: Run full test suite one final time
- [ ] **TODO-23**: Update documentation (see `/update-context-docs`)
- [ ] **TODO-24**: **CHECKPOINT: Ask the user to review completed work**

## Mandatory Checkpoints

There are 3 mandatory user review checkpoints. At each checkpoint:

1. Present a summary of work completed
2. Ask the user for feedback
3. Do not proceed until the user approves

**Checkpoint 1** (after TODO-06): Review acceptance tests before continuing implementation.
**Checkpoint 2** (after TODO-09): Review initial implementation before production readiness.
**Checkpoint 3** (after TODO-24): Final review of all completed work.

## Plan Template

```markdown
# Development Plan: {Feature Name}

**PRD:** {path}
**Changeset:** {path}

## Phase 1: Planning
- [ ] TODO-01: Create PRD
- [ ] TODO-02: Create changeset

## Phase 2: Development (TDD)
- [ ] TODO-03 through TODO-10
- [ ] CHECKPOINT after TODO-06
- [ ] CHECKPOINT after TODO-09

## Phase 3: Production Readiness
- [ ] TODO-11 through TODO-24 (ALL MANDATORY)
- [ ] CHECKPOINT after TODO-24
```

## Related Commands

- `/plan-ft` - Create PRD only
- `/plan-ft-dev` - Create changeset only
- `/update-context-docs` - Update documentation
- `/wrap-context-docs` - Wrap completed work

See CLAUDE.md for project structure, build commands, and testing guidelines.
