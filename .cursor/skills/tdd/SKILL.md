---
name: tdd
description: Guides Test-Driven Development using the red-green-refactor cycle. Never skips or removes difficult tests; seeks user advice when tests are challenging. Use when writing tests first, implementing TDD workflow, or when the user mentions TDD, test-first development, red-green-refactor, or asks to write tests before implementation.
---

# Test-Driven Development (TDD)

This skill guides you through the Test-Driven Development workflow using the strict red-green-refactor cycle.

## Core TDD Principles

**CRITICAL**: Never skip, remove, or workaround tests that are hard to overcome. If a test is challenging:
1. Stop and analyze why the test is difficult
2. Explain the challenge to the user
3. Ask for user advice on how to proceed
4. Never make tests pass through false positives or negatives

## TDD Cycle

### 🔴 Red Phase - Write Failing Tests

**Before any implementation**, write comprehensive failing tests:

1. **Define the public API** through test usage
2. **Write descriptive test names** that explain behavior
3. **Set up clear assertions** for expected outcomes
4. **Verify tests fail** for the right reasons (missing implementation, not bugs)

**Run tests** to confirm they fail as expected.

**Purpose**: Capture requirements and desired behavior before coding begins.

### 🟢 Green Phase - Make Tests Pass

**Implement minimal code** to make all failing tests pass:

1. Focus on functionality over form
2. Write just enough code to satisfy tests
3. Keep test changes minimal - tests define requirements
4. Avoid over-engineering at this stage

**Run tests** to confirm they all pass.

**If tests are difficult to pass**:
- Do NOT skip the tests
- Do NOT remove assertions
- Do NOT add workarounds or false positives
- Explain the challenge and ask for user guidance

**Purpose**: Create working implementation with minimal code changes.

### 🔵 Refactor Phase - Clean Up Code

**Improve code quality** while keeping all tests green:

1. Add proper Rust typing and remove `any` types
2. Follow clean code principles - small methods, reduced nesting
3. Extract duplicated logic
4. Ensure production readiness

**Run tests** after each refactoring to ensure they remain green.

**Purpose**: Polish implementation while maintaining test coverage and functionality.

## Phase Transitions

Follow this strict flow:

- **Red → Green**: Only proceed when you have comprehensive failing tests
- **Green → Refactor**: Only proceed when all tests are passing
- **Refactor → Red**: Start next feature cycle with new failing tests

**Never skip phases.** Each phase has a specific purpose in ensuring code quality.

## Testing Standards

### Test Quality Requirements

- Tests must be **deterministic** - no random values, no timing dependencies
- Tests must be **isolated** - each test runs independently
- Tests must be **readable** - use BDD-style descriptions
- Tests must **validate real behavior** - no false positives

### What to Test

**Red Phase checklist**:
- [ ] Happy path scenarios
- [ ] Edge cases and boundary conditions
- [ ] Error conditions and validation
- [ ] Integration points if applicable

### Test File Naming

- Unit tests: `#[cfg(test)]` modules or `tests/*.rs`
- Integration tests: `tests/integration/*.rs`
- E2E tests: `tests/e2e/*.rs`

## Handling Difficult Tests

When you encounter a test that's challenging to make pass:

### Step 1: Stop and Analyze
- What makes this test difficult?
- Is the API design problematic?
- Are there missing dependencies?
- Is the test exposing a design flaw?

### Step 2: Communicate to User
Explain clearly:
- Which test is challenging
- Why it's difficult
- What the core issue is
- Potential approaches you've considered

### Step 3: Request Guidance
Ask specific questions:
- "Should we redesign the API to make this testable?"
- "Do we need to add a dependency injection point here?"
- "Should we split this into smaller units?"

### Step 4: Never Compromise
**Do NOT**:
- Skip the test "temporarily"
- Remove assertions to make it pass
- Add `any` types to bypass Rust
- Use ranges/tolerances unless testing uncontrollable factors
- Add test-only code branches in production code
- Mock away the actual behavior being tested

## Running Tests

From repository root:
- `cargo test` - Run all tests (unit, integration, e2e)
- `cargo test` - Run unit tests (in src/ with #[cfg(test)])
- `cargo test --test integration` - Run integration tests
- `cargo test --test e2e` - Run e2e tests

**Run tests frequently** during each phase to verify state.

## Quality Gates

Before moving to the next phase:

**Red Phase**:
- ✓ All new tests fail initially
- ✓ Tests fail for the right reasons
- ✓ Test names clearly describe behavior
- ✓ Edge cases are covered

**Green Phase**:
- ✓ All tests pass
- ✓ No tests were skipped or removed
- ✓ No workarounds or false positives
- ✓ Implementation is minimal and focused

**Refactor Phase**:
- ✓ All tests remain green
- ✓ Code quality improved
- ✓ Rust types are proper
- ✓ Production ready

## Example Workflow

**Starting a new feature:**

1. **Red Phase**:
```rust
describe('UserService', () => {
  it('should create user with valid email', () => {
    const service = new UserService();
    const user = service.create('test@example.com');
    expect(user.email).toBe('test@example.com');
  });

  it('should throw error for invalid email', () => {
    const service = new UserService();
    expect(() => service.create('invalid')).toThrow('Invalid email');
  });
});
```
Run tests → they fail (UserService doesn't exist)

2. **Green Phase**:
```rust
class UserService {
  create(email: string) {
    if (!email.includes('@')) throw new Error('Invalid email');
    return { email };
  }
}
```
Run tests → they pass

3. **Refactor Phase**:
```rust
class UserService {
  create(email: string): User {
    this.validateEmail(email);
    return { email };
  }

  private validateEmail(email: string): void {
    if (!this.isValidEmail(email)) {
      throw new Error('Invalid email');
    }
  }

  private isValidEmail(email: string): boolean {
    return email.includes('@');
  }
}
```
Run tests → they still pass, code is cleaner

## Benefits

- **Comprehensive coverage** by design
- **Clear requirements** through failing tests
- **Regression prevention** with continuous validation
- **Maintainable codebase** through structured refactoring
- **Living documentation** via executable test specifications
- **Design feedback** - difficult tests reveal design issues early

## Summary

When using TDD:
1. Always write tests first (Red)
2. Implement minimal code to pass (Green)
3. Refactor while keeping tests green (Refactor)
4. Never skip or compromise on difficult tests
5. Seek user advice when facing challenges
6. Run tests frequently to verify phase completion
