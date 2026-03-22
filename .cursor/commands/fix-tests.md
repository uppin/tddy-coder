---
description: Systematic test fixing with focused execution (one test at a time) and debug logging. Use when tests are failing and detailed investigation is needed.
---

## Fix Tests (Focused Mode)

Resolve test failures through **isolated runs** (one failing test or spec at a time) and **verbose logging**, then fix **root causes** (production code, test correctness, or test infrastructure)—not symptoms.

This repository uses **Rust (Cargo)** and **Bun** (web package `packages/tddy-web`). There is **no** `yarn test`.

## Key Ideas

1. **Run tests one at a time** — narrow output to a single test name, spec, or file when possible.
2. **Turn on debug logging** — `RUST_LOG`, `DEBUG` (Cypress), or tool-specific flags as below.
3. **Fix in-tree** — implement corrections here (same agent session). Use the **Task** tool with `generalPurpose` or `refactor` only when you need a parallel deep dive; there is no `bug-fixer` subagent in this workspace.

## When Invoked

Skim for context (and show the user what you used):

- **Testing standards**: [docs/dev/guides/testing.md](../../docs/dev/guides/testing.md)
- **Workspace commands**: [AGENTS.md](../../AGENTS.md) (Bun workspace, `./test`, `./verify`, Cypress)
- **Package dev docs** (if present): `packages/{package}/docs/*.md`
- **Active changeset**: `docs/dev/1-WIP/*.md`
- **Investigations** (if present): `docs/investigations/*.md`

## 1. Identify Failing Tests

**Rust workspace** (from repo root, nix shell via `./dev` if needed):

```bash
./test
```

If terminal capture is unreliable, use:

```bash
./verify
# then read .verify-result.txt
```

**Web (`tddy-web`)** (from repo root):

```bash
./dev bun install
./dev bun run build --filter tddy-web
./dev bun run cypress:component --filter tddy-web
# or unit tests:
./dev bash -lc 'cd packages/tddy-web && bun test'
```

Note failing **crate / package**, **file**, and **test name**.

## 2. Execute One Failure at a Time (with logging)

### Rust: single test

```bash
./dev cargo test -p tddy-core -- test_name --exact
# or filter:
./dev cargo test -p tddy-core some_module::
```

Enable tracing/log noise when useful:

```bash
RUST_LOG=debug,tddy_core=trace ./dev cargo test -p tddy-core -- test_name --exact -- --nocapture
```

**Note**: `acp_backend_acceptance` needs `target/debug/tddy-acp-stub`. **`./verify` and `./test` run `cargo build -p tddy-acp-stub` first.** If you use plain `cargo test -p tddy-core` and see “not built”, run `cargo build -p tddy-acp-stub` once. Use `--skip acp_` only when intentionally scoping away those tests.

### tddy-web: Bun unit test (single file)

```bash
./dev bash -lc 'cd packages/tddy-web && bun test src/components/example.test.ts'
```

### tddy-web: Cypress component (single spec)

```bash
./dev bash -lc 'cd packages/tddy-web && bunx cypress run --component --spec cypress/component/Some.cy.tsx'
```

Verbose Cypress logging:

```bash
./dev bash -lc 'cd packages/tddy-web && DEBUG=cypress:* bunx cypress run --component --spec cypress/component/Some.cy.tsx'
```

Interactive debugging: see `.cursor/skills/cypress-ct-debug/SKILL.md` and `bun run cypress:component:debug` in AGENTS.md.

## 3. Analyze Each Failure

For each failure:

1. **Production bug?** — Wrong behavior; test expectation matches the requirement.
2. **Test wrong or stale?** — Requirement or API changed; update the test deliberately (do not weaken assertions to “make green”).
3. **Infrastructure / env?** — Missing fixture, wrong cwd, LiveKit testkit URL, etc.

Check alignment with [docs/dev/guides/testing.md](../../docs/dev/guides/testing.md): no conditional skips, no try/catch “pass anyway”, no test-only production branches.

## 4. Implement Fixes

After root-cause analysis:

- **Apply the fix in this workspace** (production code, test, or harness).
- Re-run the **same single test** with logging until it passes.
- Remove temporary debug prints before finishing (no permanent `println!` in TUI paths per project rules).

Optional parallel investigation: Task subagent `generalPurpose` or `refactor` with a tight prompt (file, failure, hypothesis)—not a separate “bug-fixer” role.

## 5. Validate Standards

- Deterministic tests; one clear act/assert path.
- No fallbacks or environment hacks without explicit product consent (see coding practices).
- No `cfg(test)` branches in production code for behavior changes.

## 6. Full Suite Verification

```bash
./test
```

For web changes:

```bash
./dev bash -lc 'cd packages/tddy-web && bun test'
./dev bun run cypress:component --filter tddy-web
```

Rust lint from repo root:

```bash
./dev cargo clippy -- -D warnings
```

## Output Format (for the user)

Use this structure in the final message (no emoji required):

### Focused Test Fix Summary

**Approach**: One test/spec at a time; logging enabled where noted.

**Totals**: Analyzed X; initially failing Y; now failing Z.

### Per-failure

For each: **name**, **location**, **error summary**, **root cause** (production | test | infra), **fix** (file + short description), **verified** (command run).

### Production vs test changes

- List files touched and why.

### Standards check

- Confirm alignment with `docs/dev/guides/testing.md`.

### Final verification

- Commands run for full suite; attach or reference `./verify` / `.verify-result.txt` if used.

## Changeset “Validation Results” (optional)

If a changeset in `docs/dev/1-WIP/` is open:

```markdown
### Test Fixes (fix-tests command)
**Last Run**: YYYY-MM-DD
**Status**: All passing | Partial (explain)

**Summary**: Focused runs; root causes; suites re-run (./test, bun/cypress as applicable).
```

## Common Failure Patterns (this repo)

| Pattern | Symptom | What to try |
|--------|---------|-------------|
| Async / timing (Cypress) | Flaky UI | Follow cypress-ct-debug skill; avoid arbitrary `cy.wait` without cause |
| Order dependency | Passes alone, fails in suite | Isolate spec; check shared `before`/`after` and global state |
| Stub / mock | Wrong spy or no call | Trace enqueue/spy setup in component tests |
| Env (LiveKit, etc.) | CI vs local | `LIVEKIT_TESTKIT_WS_URL`, testcontainers docs in AGENTS.md |
| Missing binary | “not built” in test | `cargo build -p …` as test indicates |

## Related

- [docs/dev/guides/testing.md](../../docs/dev/guides/testing.md)
- [.cursor/rules/coding-practices.mdc](../rules/coding-practices.mdc)
- [AGENTS.md](../../AGENTS.md)
- `.cursor/skills/cypress-ct-debug/SKILL.md` (Cypress component debugging)
