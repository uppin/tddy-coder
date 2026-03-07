# Testing Practices

This document defines testing standards, anti-patterns, and guidelines for unit, integration, and production tests.

## Success Rate

There is no partial success rate. The only production-ready rate is 100% passing tests.

## General Guidelines

1. Tests should be as concise as possible.
2. They should be flat and easy to read.
3. There can be tests supporting code like drivers and testkits.
4. **There must not be any workarounds to just make the test pass**.
5. Tests should be reliable and add reliability to the production code.
6. Always assume that the environment is ready. Never ignore, return or workaround from the test.
7. Your goal is to see tests failing in order to produce better and more reliable production code.
8. A test producing a false positive is worse than no test.
9. A test should not have any code branches. It should test only one thing and one flow.
10. Do not add any alternative fallbacks to actors of the test setup.
11. Test givens and outcomes should be deterministic.
12. Tests will run on different environments and machines. No assumptions about completion time.
13. Performance testing should be strictly done by the User unless specifically asked.

## Anti-Patterns

### Conditional Test Skipping

```rust
// WRONG
if !some_function.is_available() {
  eprintln!("Skipping test - function not available");
  return;
}

// RIGHT
assert!(some_function.is_available());
```

### Try/Catch Workarounds

```rust
// WRONG
let result = some_function().unwrap_or_else(|_| {
  eprintln!("Function not implemented yet, passing anyway");
  default_value()
});
assert_eq!(result, expected);

// RIGHT
let result = some_function().expect("should succeed");
assert_eq!(result, expected);
```

### Conditional Logic in Tests

```rust
// WRONG
if !result.is_empty() {
  assert_eq!(result[0].data, expected_data);
} else {
  assert!(result.is_some());
}

// RIGHT
assert_eq!(result.len(), 1);
assert_eq!(result[0].data, expected_data);
```

### Fallback Assertions

```rust
// WRONG
assert_eq!(actual_value, expected_value);
assert!(actual_value.is_some()); // fallback

// RIGHT
assert_eq!(actual_value, expected_value);
```

### Environment Detection in Tests

```rust
// WRONG
if std::env::var("TEST").is_ok() {
  // Use mock implementation
}

// RIGHT - Use dependency injection or test setup instead
```

### "TODO" Test Placeholders

```rust
// WRONG
#[test]
fn should_work_with_feature_x() {
  assert!(true);
}

// RIGHT - Either test works completely or don't write the test yet
#[test]
fn should_work_with_feature_x() {
  let result = feature_x.do_something();
  assert_eq!(result, expected_output);
}
```

### Multiple Code Paths in One Test

```rust
// WRONG
#[test]
fn should_handle_various_inputs() {
  match input_type {
    InputType::A => assert_eq!(process_a(), result_a),
    InputType::B => assert_eq!(process_b(), result_b),
  }
}

// RIGHT
#[test]
fn should_handle_input_type_a() {
  assert_eq!(process_a(), result_a);
}

#[test]
fn should_handle_input_type_b() {
  assert_eq!(process_b(), result_b);
}
```

### Ignoring or Suppressing Errors

```rust
// WRONG
let result = risky_operation().unwrap_or_default();
assert!(result.is_some());

// RIGHT
let result = risky_operation().expect("should succeed");
assert!(result.is_some());
```

## Test Composition

1. Each test has a primary purpose or subject.
2. It may have secondary actors which aid the primary test.
3. Test suites should not grow too large. Big ones should be split.
4. Test cases are sorted from happy flows to secondary flows.
5. Error handling and edge cases come last in the test suite.
6. Test suites don't need to test secondary actors.

## Unit Tests

File pattern: `#[cfg(test)]` modules in `src/` or `tests/*.rs`

### Principles

1. Use stubs (preferred) or mocks to isolate from environment.
2. Hexagonal architecture is where unit tests work best.
3. Unit tests can influence the unit under test to make it more testable.
4. Unit tests should avoid loading from global environment in both test and production code.
5. Prefer modifying production code to have dependencies injected rather than directly imported.
6. Direct imports for cross-cutting, lightweight & functional dependencies are fully ok.
7. Collaborators with complex logic are preferred to be injected.

### Style & Tech

- Unit tests use `cargo test`.
- We use BDD-style `#[test]` functions to test behavior.

## Integration Tests

File pattern: `tests/*_integration.rs` or `tests/integration/*.rs`

### When to Use

Use integration tests for:
- Component interaction testing (multiple modules working together)
- API contract validation without external services
- Error propagation through multiple layers
- Fast feedback during development (< 3 seconds)

Do not use integration tests for:
- External service calls (use `#[ignore]` tests or separate binary)
- Single component logic (use unit tests in `#[cfg(test)]` modules)

### Performance Requirements

- Individual tests: < 5 seconds each
- Full suite: < 30 seconds total
- Setup/teardown: < 3 seconds combined
- No real external calls: all dependencies either on localhost or stubbed

### Stubbing Strategy

```rust
// Use #[cfg(test)] or test fixtures to create test-specific clients
fn create_test_client() -> McpClient {
    McpClient::new(TestConfig {
        stub_external_services: true,
        use_invalid_paths: true,
    })
}
```

### Configuration

```toml
# Cargo.toml - integration tests live in tests/ directory
# Run with: cargo test --test integration
```

## Production Tests

File pattern: `*.rs` with `#[ignore]` or separate test binary

### When to Use

Use production tests for:
- End-to-end validation with real external services
- Developer verification of complex integrations before releases
- Real environment testing that can't be adequately mocked

Do not use production tests for:
- CI/CD pipelines (too slow, unreliable)
- Unit testing individual components
- Rapid development feedback

### Performance Expectations

- Individual tests: 30 seconds to 4 minutes each
- Full suite: 3-10 minutes total
- Timeout settings: 10 minutes maximum per test
- Sequential execution to avoid parallel conflicts

### CI/CD Exclusion

Production tests use `#[ignore]` and can be run with `cargo test -- --ignored` when needed.

## Test Execution Workflow

```bash
# Regular development (fast feedback)
cargo test

# Run ignored/slow tests (production)
cargo test -- --ignored

# Full validation
cargo test && cargo test -- --ignored
```
