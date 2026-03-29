---
description: Comprehensive PR preparation workflow using subagents
---
## PR Wrap - Prepare Changes for Pull Request

This command orchestrates comprehensive PR preparation by invoking specialized subagents for each step.

**Goal**: Ensure code is clean, maintainable, tested, and production ready.

## Prerequisites

- Branch contains the work you intend to PR (prefer committing **after** step 6 so fmt/clippy/test and hooks stay green — **do not** use `--no-verify` on commit or push)
- Changeset document in context (if applicable): `docs/dev/1-WIP/YYYY-MM-DD-*.md`
- PRD document in context (if applicable): `docs/ft/*/1-WIP/PRD-YYYY-MM-DD-*.md`

## Workflow Steps

Execute these steps in order, using the specified subagent for each:

### 1. Validate Changes → Refactor

**Run Command**: `/validate-changes`
- Analyze code changes for risks
- Update changeset "Validation Results" section

**Invoke Subagent**: `refactor`
- Fix issues found in validation

### 2. Validate Tests → Refactor

**Run Command**: `/validate-tests`
- Check test quality and anti-patterns
- Update changeset "Validation Results" section

**Invoke Subagent**: `refactor`
- Fix test issues found

### 3. Production Readiness → Refactor

**Run Command**: `/validate-prod-ready`
- Check for mock code, TODOs, unused code
- Update changeset "Validation Results" section

**Invoke Subagent**: `refactor`
- Fix production readiness issues

### 4. Code Quality → Refactor

**Run Command**: `/analyze-clean-code`
- Analyze code quality metrics
- Update changeset "Validation Results" section

**Invoke Subagent**: `refactor`
- Apply clean code improvements

### 5. Final Validation

**Run Command**: `/validate-changes`
- Re-validate after all refactoring
- Ensure no new issues introduced

### 6. Linting & Type Checking

Run directly (no subagent needed), from repo root (use `./dev` / `./test` if your toolchain is nix-wrapped):
```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
```

### 7. Update & Wrap Documentation

**Run Command**: `/wrap-context-docs`
- Update changeset progress
- Wrap documentation if all complete

### 8. Display Summary

Present comprehensive summary with recommendations.

## Subagent Invocation Pattern

For each step, explicitly delegate to the subagent:

```
Use the `/validate-changes` command to analyze the current changes.
[Wait for completion]

Use the refactor subagent to fix the issues found.
[Wait for completion]

Use the `/validate-tests` command to check test quality.
[Wait for completion]
...
```

## Available Subagents

| Subagent | Purpose |
|----------|---------|
| `/validate-changes` | Analyze code change risks |
| `/validate-tests` | Check test quality |
| `/validate-prod-ready` | Production readiness check |
| `/analyze-clean-code` | Code quality metrics |
| `refactor` (subagent) | Fix identified issues |
| `/wrap-context-docs` | Update/wrap documentation |

## Tracking Progress

Create TODO list and mark each step complete:

```
[ ] 1. /validate-changes → refactor
[ ] 2. /validate-tests → refactor
[ ] 3. /validate-prod-ready → refactor
[ ] 4. /analyze-clean-code → refactor
[ ] 5. Final validation
[ ] 6. Linting & type checking
[ ] 7. Documentation update/wrap
[ ] 8. Summary
```

## Output Format

```markdown
## 🎯 PR Preparation Complete

### Subagents Invoked
| Step | Subagent | Status |
|------|----------|--------|
| 1 | /validate-changes | ✅ |
| 1 | refactor | ✅ |
| 2 | /validate-tests | ✅ |
| 2 | refactor | ✅ |
| 3 | /validate-prod-ready | ✅ |
| 3 | refactor | ✅ |
| 4 | /analyze-clean-code | ✅ |
| 4 | refactor | ✅ |
| 5 | /validate-changes | ✅ |
| 7 | /wrap-context-docs | ✅ |

### Summary
- **Code Quality**: X/10 ⭐
- **Tests**: All passing ✅
- **Production Ready**: ✅ Yes
- **Documentation**: ✅ Wrapped

### 🎯 Recommendation

[If fit to ship:]
✅ **Code is ready for PR!**
Next step: Use `/pr` command to create pull request

[If needs refinement:]
⚠️ **Refinements needed:**
1. [Issue 1]
2. [Issue 2]
```

## Best Practices

✅ **Do:**
- Follow all steps in order
- Wait for each subagent to complete before proceeding
- Track progress with TODOs
- Provide changeset/PRD context to subagents

❌ **Don't:**
- Don't skip validation steps
- Don't wrap incomplete changesets
- Don't proceed with failing tests
- Don't ignore subagent recommendations
- Don't use `--no-verify` when committing or pushing

## Related

**Related**: Subagent `refactor`, Commands `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`, `/wrap-context-docs`
**Commands**: `/pr` (next step), `/update-context-docs`
