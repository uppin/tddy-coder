# Testing Practices

This document defines testing standards, anti-patterns, and guidelines for unit, integration, and production tests.

## Mandatory Test Style: `fluent-tests`

**`fluent-tests` is the mandatory test style for this repo.** Every test — new, refactored, or fixed — must comply with the `fluent-tests` skill at `.claude/skills/fluent-tests/`. Before writing or modifying any test, read:

- `.claude/skills/fluent-tests/references/generic-guidelines.md` (universal principles)
- The framework-specific reference for the test type (`rust/std-test.md`, `typescript/cypress-component.md`, etc.)

Required compliance:
- **Three-act structure** — every test has Given/When/Then, visually separated
- **Intent-revealing names** — test names form a sentence describing behavior
- **One behavior per test** — each test proves exactly one thing
- **Encapsulate access** — selectors, wire formats, and raw protocol calls live in drivers/page objects, never in the test body
- **Concrete values** — meaningful literals (`alice@example.com`), not `foo`/`bar`/`test`
- **Builders for data** — complex objects built fluently with sensible defaults
- **In-memory backends** for Cypress component tests (`mountWithRpc` + `anInMemoryRpcBackend`), not `cy.intercept`

Violations are treated as test bugs. The anti-patterns below are in addition to, not instead of, the fluent-tests standard.

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

## Determinism under load

A suite that passes on an idle laptop and fails on a busy one is not reporting whether the code
works. Two back-to-back `cargo test --workspace` runs once produced 12 and 15 failures with only 6 in
common; every one of them was a fixture defect, not a bug.

**`--test-threads=1` does not make a run serial.** It is *per test binary* — cargo still runs many
binaries at once, so a dozen tests can fork shell stubs simultaneously no matter what `./test` and
`./verify` pass.

Rules that follow from that:

- **Never sleep, and never read a file the subject is still writing.** Poll with
  `tddy_testing_commons::wait::{eventually, eventually_awaiting, eventually_blocking}` (25 ms
  cadence). The probe returns `Result<T, String>`, so the panic names the condition, the ceiling, the
  poll count and the **last observed state** — the difference between a flake and a diagnosis.
- **A timeout is a safety net, not a prediction.** Size it for the worst machine that will ever run
  it and let the polling decide when to stop. A budget tuned to an idle host reports load as breakage.
- **Assert on what happened, not on how long it took.** "Warm-up failed fast" is one probe received
  (`wiremock`'s `received_requests()`), not sub-second elapsed time — under load a single correct
  round trip can outlast any short budget.
- **Stubs must write records atomically.** `tddy_testing_commons::stub_scripts::a_stub_agent_script`
  writes to `"$f.tmp.$$"` then `mv -f`, and appends a pre-built line through a single `printf`, so a
  reader can never observe a half-written argv record. (Measured under injected preemption: 526,770
  torn observations for a naive stub, 0 for this one.) A longer timeout does not fix a torn read.
- **Wait for the thing, not for a proxy for the thing.** A Unix socket inode outlives the process that
  bound it, so "the socket file exists" is not "the server is up"; poll a real `connect` *and* the
  child's exit status. A spawned fixture with no readiness signal silently charges process start-up,
  dynamic linking and tokio boot to whatever budget the test thought it was measuring — give it a
  handshake, then split the budget (start-up vs. the call).
- **Never bind `:0` to find a "free" port for a later test.** The kernel hands ephemeral ports back
  out immediately. Probe *outside* the ephemeral range (49152+ on macOS, 32768+ on Linux).
- **Nothing outside the repo may need to be running.** Two tests required a local inference server and
  burned 242 s of an 863 s suite waiting out a readiness timeout before failing. They now point an
  agent def at `tddy_testing_commons::stub_http::a_stub_http_endpoint_answering_ok` — a loopback
  listener that drains the request headers **and** the declared `Content-Length` body before replying,
  because a stub that answers while the client is still writing gets that write reset.
- **Paths: compare like with like.** macOS `/tmp` is a symlink to `/private/tmp`; production
  canonicalizes, so a raw `TempDir` path is a different string for the same directory.

**When a test needs different behaviour from production, production grows a config knob whose default
*is* today's value, and the test supplies its own through that same knob** (`defaults ← daemon.yaml ←
`TDDY_*`). A test-only branch in production code is forbidden — see CLAUDE.md. The specialized-agent
warm-up budget and the spawn-startup grace period are both this pattern.

**Measure flakiness, don't assume it.** Run the full suite 3× back to back with the machine
deliberately loaded and compare the failure *sets*. `cargo test` fails fast by default and abandons
the remaining binaries after the first failing one, so the measurement needs `--no-fail-fast` or it
stops at the first flake and reports nothing.

## Rust workspace: `./verify` vs plain `cargo test`

From the repository root, prefer **`./dev ./verify`** (or **`./test`**, which follows the same pattern) when you need results that match CI and agent workflows: the dev shell provides `cargo` on `PATH`, and **`./verify`** builds prerequisite binaries such as **`tddy-acp-stub`** before running the full workspace test suite (output is also written to **`.verify-result.txt`**). Running **`cargo test`** or **`cargo test -q`** **without** that prerequisite build can fail integration tests that expect the stub. For a quick compile-only check, use **`./dev cargo check`**.

## LiveKit and gRPC terminal RPC E2E

End-to-end tests for `StreamTerminalIO`, `VirtualTui`, and Ghostty live in `packages/tddy-e2e`. For protocol behavior, assertion strategies, flaky-test notes, and source references, see [livekit-terminal-rpc-e2e.md](./livekit-terminal-rpc-e2e.md).
