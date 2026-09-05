---
name: fluent-tests
description: "Write fluent, human-readable tests that read like plain English while using the type system. Use when writing tests, improving test readability, designing matchers, or structuring Given-When-Then patterns in Rust or TypeScript."
---

# Fluent Tests

Write tests that a human or AI can read like a specification. Every test should answer three questions at a glance: what is set up, what happens, and what is expected.

## When to Use

- Writing new tests in Rust or TypeScript
- Refactoring existing tests for readability
- Designing custom matchers or assertion helpers
- Creating test data builders or factory functions
- Reviewing test code for clarity

## When NOT to Use

- For TDD workflow/process — use the project's `red`/`green`/`plan-red` skills instead
- For fixing flaky tests — use `fix-tests` instead

## Core Principles

1. **Three-act structure** — every test has Given (setup), When (execute), Then (verify), visually separated
2. **Flat and deterministic** — no nesting beyond one describe/module block, no conditionals or loops in tests
3. **Intent-revealing names** — test names form a sentence describing the behavior, not the implementation
4. **Builders for data** — complex objects built fluently with sensible defaults, override only what matters
5. **Custom matchers** — domain-specific assertions that read as English phrases
6. **One behavior per test** — each test proves exactly one thing
7. **Concrete values** — use meaningful literals ("alice@example.com"), not random or placeholder data
8. **No noise** — extract setup helpers, hide irrelevant defaults, show only what the test is about
9. **Encapsulate access** — selectors, wire formats, and raw protocol calls live in drivers/page objects, never in the test body. A test never contains `cy.get("[data-testid=…]")` or hand-rolled stream/message handling — only named, intent-revealing methods

## Workflow

### 1. Detect Context

Read existing tests in the project. Identify:
- Language and test framework (Rust built-in `#[test]`, `rstest`, `tokio::test`; TypeScript vitest, cypress, playwright, etc.)
- Existing patterns, matchers, builders, or drivers
- Naming conventions and file locations

### 2. Apply Language-Specific Patterns

Read the reference file matching the project's language:

- **Any language**: [references/generic-guidelines.md](references/generic-guidelines.md) — principles, anti-patterns, builders, matchers, async, mocking
- **Rust**: [references/rust/std-test.md](references/rust/std-test.md) — `#[test]` modules, builders, custom assertion helpers, `rstest` parameterized tests and fixtures, `tokio::test` for async, in-memory fakes
- **TypeScript**:
  - [references/typescript/vitest.md](references/typescript/vitest.md) — builders, custom matchers, describe structure, async
  - [references/typescript/cypress-e2e.md](references/typescript/cypress-e2e.md) — custom commands, page objects
  - [references/typescript/cypress-component.md](references/typescript/cypress-component.md) — fluent driver pattern, scenic() example
  - [references/typescript/playwright.md](references/typescript/playwright.md) — expect.extend, page object model

Always read `generic-guidelines.md` first for the universal anti-patterns, then the framework-specific file for idiomatic examples.

### 3. Write or Refactor Tests

Apply in order of impact:
1. Restructure into Given/When/Then blocks
2. Extract builders or factories for test data
3. Replace raw assertions with matchers that express intent
4. Rename tests to describe behavior
5. Remove dead setup, irrelevant fields, and commented-out code
6. For async code: replace sleeps with event synchronization or `eventually`, and suggest production code changes (return futures/promises, emit events) when the code isn't testable — see the "Testing Async Code" section in generic-guidelines.md

### 4. Verify Readability

For each test, ask: can someone who has never seen this code understand what it does by reading just the test name and the Then block? If no, the test needs more work.
