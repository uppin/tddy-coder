# Systematic Test Fixing

Identify and fix failing tests methodically, **one at a time**, with logging. Authoritative Cursor command (toolchain, Cypress, `./verify`): **`.cursor/commands/fix-tests.md`**.

## Process

### 1. Discover Failures

**Rust** (repo root):

```bash
./test
```

If output is hard to capture: `./verify` then read `.verify-result.txt` (see AGENTS.md).

**Web (`packages/tddy-web`)**:

```bash
./dev bash -lc 'cd packages/tddy-web && bun test'
./dev bun run cypress:component --filter tddy-web
```

List failing **package**, **file**, and **test name**.

### 2. Prioritize

Fix foundational unit tests before integration tests that depend on them.

### 3. Fix Each Test

For each failure, in order:

**a. Isolate** — Run a **single** test or spec with verbose logging:

- Rust: `cargo test -p <package> <name> -- --exact --nocapture` with `RUST_LOG=debug` as needed.
- Bun: `bun test path/to/file.test.ts`
- Cypress: `bunx cypress run --component --spec cypress/component/Foo.cy.tsx` with `DEBUG=cypress:*` when needed.

**b. Diagnose** — Production bug vs incorrect/stale test vs infrastructure (fixtures, env, missing binary).

**c. Fix** — Correct production code or update the test to match **real** required behavior. Never weaken assertions just to pass.

**d. Standards** — [docs/dev/guides/testing.md](../../docs/dev/guides/testing.md): no conditional skips, no try/catch “pass anyway”, no test-only production behavior branches.

**e. Verify** — Re-run the same focused command, then widen.

### 4. Full Suite Verification

```bash
./test
```

Web changes: full `bun test` and relevant Cypress run. Then `cargo clippy -- -D warnings` for Rust.

## Output

Summarize each failure, root cause, fix, and final suite result. See `.cursor/commands/fix-tests.md` for a full report template.
