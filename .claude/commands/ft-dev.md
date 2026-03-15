# Feature Development: TDD from A to Z

You are developing a feature end-to-end using TDD. A development plan (changeset) should already exist. If it does not, ask the user to provide one or describe the feature.

## Prerequisites

- A changeset or development plan describing the feature milestones
- A working branch aligned with the feature (if on main/master, create a feature branch first)

## Process

### 1. Review the Plan

- Read the changeset / plan documents (check `docs/dev/1-WIP/` for active changesets).
- Verify the current branch is appropriate for this feature.
- List the milestones and their status.

### 2. Create Failing Acceptance Tests

For the current milestone, write acceptance-level tests that define "done":

- Tests must be **fully implemented** -- real assertions, real setup, real expected values. No empty test bodies, no `todo!()`, no placeholders.
- Tests should cover the milestone's key behaviors end-to-end.
- Run `cargo test` to confirm they fail for the right reasons.

Present the test list:

| Test | File | Status |
|------|------|--------|
| `test_name` | `packages/.../tests/file.rs` | FAILING (expected) |

### 3. TDD Red-Green Cycle

For each milestone, iterate:

1. **Red**: Write or review failing unit/integration tests for the next piece of behavior. Follow the same rules as `/red` -- fully implemented tests, no skeletons, no conditional logic.
2. **Green**: Write minimal production code to make tests pass. Follow the same rules as `/green` -- real implementation, no fakes, no test-specific branches.
3. **Verify**: Run `cargo test` (package-scoped) after each green step.

Use the Agent tool to delegate implementation work when appropriate, providing clear context about the failing tests and expected behavior.

### 4. Update Progress

After completing a milestone:
- Update the changeset document if one exists.
- Run the full test suite: `cargo test`.
- Run `cargo clippy -- -D warnings`.

### 5. Repeat

Move to the next milestone and repeat from step 3.

## Output Format

### Completion Status

| Milestone | Status | Tests |
|-----------|--------|-------|
| Milestone 1 | DONE / IN PROGRESS / TODO | X passing |

### Test Results

```
<full cargo test output>
```

### Next Steps

- What remains to be done
- Any blockers or decisions needed from the user
- Suggest `/pr` when all milestones are complete
