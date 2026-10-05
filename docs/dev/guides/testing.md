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

### Workflow Names in Test or File Names

A test is named for **what it tests**. The command, phase or changeset that happened to produce it
is not part of its subject, and none of it stays true: every test here was once red, so `_red`
distinguishes nothing — it records which command the author typed.

This applies to the **file name** exactly as it does to the test name. A `*_red.rs` file is the
same mistake as `#[test] fn red_phase_rejects_expired_cards`, with the phase moved one level out
where it is more visible and harder to grep away.

```
// WRONG — names the ritual, the paperwork, or the category
packages/tddy-discovery/tests/provider_queue_red.rs
packages/tddy-tools/tests/pr433_scheduling.rs
packages/tddy-core/tests/phase2_migration.rs
packages/tddy-tools/tests/relay_dispatch_acceptance.rs
#[test] fn green_phase_admits_the_waiting_caller()

// RIGHT — names the subject
packages/tddy-discovery/tests/provider_queue.rs
packages/tddy-tools/tests/request_scheduling.rs
packages/tddy-core/tests/schema_migration.rs
packages/tddy-tools/tests/relay_dispatch.rs
#[test] fn releasing_the_slot_admits_the_caller_that_was_waiting()
```

### `unit`, `integration` and `e2e` are legitimate markers

They say what a reader needs before opening the file: the scope and cost of the test — what it
touches, how slow it is, what a failure implicates. That is real information about the test, and
unlike a phase it stays true for the file's whole life. Keep them.

`session_store_unit_tests.rs`, `pr_stack_integration.rs`, `login_flow_e2e.rs` are all fine names.

### `acceptance` is not one of them

It names no level on that scale. In this repo it is attached to **348** files that are
overwhelmingly ordinary integration tests, so it partitions nothing — it only crowds out the words
that would have said what the file covers. If the file is an integration test, mark it
`integration` or leave it unmarked; if it earns a marker, use one that narrows something.

`_test` / `_tests` on a file already inside `tests/` is redundant rather than wrong. Drop it when
renaming anyway; it is not what this rule is about.

Suffixes a runner discovers by always stay: `.test.ts`, `.test.tsx`, `.cy.ts`, `.cy.tsx` — those
are mechanical, not descriptive.

**Legacy names are not a licence.** 374 test files in `packages/` carry a name this rule
disallows — 348 `*_acceptance.rs` and 26 `*_red.rs`. Do not add to them. Do not mass-rename them
as a drive-by either: a rename churns history and belongs in its own commit. See
[2026-09-27-test-files-are-named-after-the-workflow-that-made-them.md](../todo/2026-09-27-test-files-are-named-after-the-workflow-that-made-them.md).

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

### Test Rooms Come From `unique_room`

A test that talks to a LiveKit server names its room with `LiveKitTestkit::unique_room("<purpose>")`
(`<purpose>-<hex nanos>-<pid>-<counter>`), never a fixed literal. A fixed room is private only while
each test owns its own server; with a shared server (`LIVEKIT_TESTKIT_WS_URL`) or tests running side
by side, two tests on one name interfere. Keep the purpose as the prefix so a leaked room can be
attributed, and build the name once per test when its participants must meet in it. Identities are
room-scoped and stay as they are. The guard test `livekit_tests_use_unique_rooms` in
`tddy-livekit-testkit` fails and names the file and line of any fixed room it finds.

The rule covers every name that **becomes** a room, not only the ones a test passes as one. A
session id names its session room (`session-{id}`), so a fixture's session id comes from
`unique_room` too: the guard reads room constants and cannot see that derivation. Unique per
*process* is the bar — under nextest each test is its own process and the LiveKit binaries run side
by side, while `#[serial]` and a `OnceLock` room only order or share within one process.

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
