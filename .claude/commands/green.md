# TDD Green Phase: Make Tests Pass

You are executing the GREEN phase of TDD. Failing tests already exist from the Red phase. Your job is to write the minimal production code that makes them pass.

## Rules

1. **Code quality > test passage** -- never compromise production code design just to force tests green. If the tests demand a bad API, flag it and ask the user before proceeding.
2. **Real production implementation** -- no fake data, no hardcoded return values, no stub implementations that only work for the test inputs. The code must be genuinely correct.
3. **No test-specific branches** -- never write `if cfg!(test)` or any logic that behaves differently under test. Production code must not know it is being tested (see CLAUDE.md).
4. **Minimal implementation** -- write only what is needed to pass the current failing tests. Do not add features, optimizations, or abstractions beyond what the tests require.
5. **Respect existing architecture** -- follow patterns already established in the codebase. Check neighboring modules for conventions.

## Process

1. Run `cargo test` (scoped with `-p <package>`) to see the current failures.
2. Read the failing test code to understand the expected API and behavior.
3. Use the Agent tool to delegate implementation work with this prompt context:
   - The failing tests and their expected behavior
   - The package and module where production code should go
   - Relevant architectural patterns from the codebase
   - The constraint: minimal code to pass tests, no extras
4. After implementation, run `cargo test` again to verify all previously-failing tests now pass.
5. Run `cargo clippy -- -D warnings` to check for lint issues.
6. If any test still fails, diagnose whether it is a production code issue or a test issue. Fix production code; do not modify tests without user consent.

## Output Format

### Implementation Summary

| Module | File | What was added |
|--------|------|---------------|
| `module_name` | `path/to/file.rs` | Brief description |

### Test Results

```
<paste cargo test output here>
```

- Total: X tests
- Passing: X
- Failing: X (if any, explain why)

### Code Quality Notes

- [ ] No hardcoded or fake implementations
- [ ] No test-specific branches (`cfg!(test)`)
- [ ] Clippy passes with no warnings
- [ ] Implementation is minimal -- nothing beyond what tests require
- [ ] Follows existing codebase patterns

### Next Step

If all tests pass, suggest the user review the implementation and proceed to the next milestone or run `/red` for the next set of behaviors.
