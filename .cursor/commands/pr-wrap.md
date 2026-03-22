---
description: Comprehensive PR preparation workflow using validation commands and targeted refactors
---

## PR Wrap — Prepare Changes for Pull Request

Orchestrate validation steps, address findings, run the real toolchain, then wrap docs when appropriate.

**Goal**: Code is reviewed for risk, tests meet standards, tooling is green, and documentation matches project rules.

## Prerequisites

- Changes ready for review (committed or clearly staged). Prefer **committing only after** fmt/clippy/tests pass (step 6), not the reverse.
- **Never use `git commit --no-verify`** — it is forbidden in this repo ([AGENTS.md](../../AGENTS.md)). Fix hooks or run validations before committing.
- Optional context: changeset `docs/dev/1-WIP/YYYY-MM-DD-*.md`, feature PRD `docs/ft/*/1-WIP/PRD-*.md` (see [changeset-doc.mdc](../rules/changeset-doc.mdc)).

## Workflow (order matters)

### 1. Validate changes → fix

**Cursor command**: `/validate-changes`

- Assess risks and unsafe patterns; update changeset **Validation Results** per that command’s template.

**Then**: Implement fixes here (same session) or use the **Task** subagent `refactor` with a narrow prompt (files, findings, acceptance criteria).

### 2. Validate tests → fix

**Cursor command**: `/validate-tests`

- Check test quality vs [docs/dev/guides/testing.md](../../docs/dev/guides/testing.md); update **Validation Results**.

**Then**: Same as step 1 — fix production/tests or delegate to Task `refactor`.

### 3. Production readiness → fix

**Cursor command**: `/validate-prod-ready`

- Blockers, TODOs, mocks, etc.; update **Validation Results**.

**Then**: Fix or Task `refactor`.

### 4. Clean code → fix

**Cursor command**: `/analyze-clean-code`

- Quality notes; update **Validation Results**.

**Then**: Fix or Task `refactor`.

### 5. Final validation

**Cursor command**: `/validate-changes` again

- Confirm refactors did not introduce new issues.

### 6. Lint and test (mandatory)

Run from repo root inside the nix toolchain (**`./dev`**):

```bash
./dev cargo fmt --all
./dev cargo clippy -- -D warnings
./test
```

- Use **`./verify`** instead of `./test` when terminal capture is unreliable; read **`.verify-result.txt`** for evidence ([AGENTS.md](../../AGENTS.md)).
- **`./test`** / **`./verify`** build **`tddy-acp-stub`** so `tddy-core` ACP integration tests can run.

**If `packages/tddy-web` (or Bun workspace) changed**, also:

```bash
./dev bun install
./dev bun run build --filter tddy-web
./dev bash -lc 'cd packages/tddy-web && bun test'
./dev bun run cypress:component --filter tddy-web
```

(Adjust scope: e.g. one spec file instead of full Cypress when iterating.)

### 7. Wrap documentation (when criteria met)

**Cursor command**: `/wrap-context-docs`

- Only when the changeset and validations satisfy [wrap-context-docs.md](./wrap-context-docs.md) and [changeset-doc.mdc](../rules/changeset-doc.mdc). Do not wrap incomplete work.

### 8. Summary

Report what ran, what failed/fixed, and whether the branch is ready for PR.

## Commands vs Task subagent

| Kind | Name | Role |
|------|------|------|
| Cursor command | `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`, `/wrap-context-docs` | Guided review; update changeset sections |
| Task subagent | `refactor` | Optional parallel implementation pass for mechanical refactors |
| Cursor command | `/pr` | Open/create PR after readiness |

Slash entries are **not** a separate runtime named “validate-changes”; they are repo instructions for the agent.

## Invocation pattern

For each validation step:

1. Run the **Cursor command** (read `.cursor/commands/<name>.md` for full rules).
2. Apply fixes or spawn **Task** `refactor` with a concrete checklist.
3. Re-run the same command until the changeset reflects **pass** or documented exceptions.

## Tracking progress

```
[ ] 1. /validate-changes → fixes
[ ] 2. /validate-tests → fixes
[ ] 3. /validate-prod-ready → fixes
[ ] 4. /analyze-clean-code → fixes
[ ] 5. /validate-changes (final)
[ ] 6. fmt, clippy, ./test (and web if touched)
[ ] 7. /wrap-context-docs (if ready)
[ ] 8. Summary
```

## Output format (for the user)

```markdown
## PR preparation summary

### Validations
| Step | Command | Result |
|------|---------|--------|
| 1 | /validate-changes | pass / notes |
| 2 | /validate-tests | pass / notes |
| 3 | /validate-prod-ready | pass / notes |
| 4 | /analyze-clean-code | pass / notes |
| 5 | /validate-changes (final) | pass / notes |

### Toolchain
- cargo fmt: pass / fail
- cargo clippy -D warnings: pass / fail
- ./test (or ./verify): pass / fail
- tddy-web (if applicable): build / bun test / cypress: pass / skip

### Documentation
- /wrap-context-docs: run / skipped (reason)

### Recommendation
- Ready for `/pr` OR list blockers and next actions.
```

## Best practices

**Do**: Run steps in order; update the changeset as each command specifies; use `./dev` for Rust/Bun; read `.verify-result.txt` when using `./verify`.

**Don’t**: Skip step 6; wrap incomplete changesets; merge with failing tests; use `--no-verify`.

## Related

- [validate-changes.md](./validate-changes.md), [validate-tests.md](./validate-tests.md), [validate-prod-ready.md](./validate-prod-ready.md), [analyze-clean-code.md](./analyze-clean-code.md), [wrap-context-docs.md](./wrap-context-docs.md), [pr.md](./pr.md)
- [AGENTS.md](../../AGENTS.md), [docs/dev/guides/testing.md](../../docs/dev/guides/testing.md)
