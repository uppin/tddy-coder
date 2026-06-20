# Fluent Tests

Write fluent, human-readable tests that read like plain English while using the type system. Use when writing tests, improving test readability, designing matchers, or structuring Given-When-Then patterns.

Scoped to **Rust** and **TypeScript**.

## Structure

```
fluent-tests/
  SKILL.md                              # Skill definition
  references/
    generic-guidelines.md               # Language-agnostic principles & anti-patterns
    rust/
      std-test.md                       # #[test], rstest, tokio::test, builders, custom asserts
    typescript/
      vitest.md                         # builders, custom matchers, describe structure, async
      cypress-e2e.md                    # custom commands, page objects
      cypress-component.md              # fluent driver pattern
      playwright.md                     # expect.extend, page object model
```

## Usage

Invoke the skill when writing or refactoring tests. It reads `generic-guidelines.md` for the
universal patterns first, then the reference matching the project's language and framework.
