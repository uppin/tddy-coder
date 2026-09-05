---
description: TDD Green Phase - Delegate implementation to tdd-implementer subagent. Goal is quality implementation, not forcing tests to pass. On a PR-stack branch, rebase and check dependencies first, then commit and push each milestone.
---
## Green Phase - Delegate Implementation

This command delegates to the tdd-implementer subagent to implement production-quality code. The goal is correct, maintainable implementation - not making tests pass at any cost.

**CRITICAL**: Code quality > Test passage. Never compromise code to force tests to pass.

**For complete TDD workflow, see `.cursor/rules/tdd.mdc`.**
**For testing standards, see `.cursor/rules/testing-practices.mdc`.**
**On a PR-stack branch, load the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`)** — it owns the
rebase, dependency and boundary rules that step 0 below only enforces.

## Prerequisites

1. **Failing tests must exist** - Use `/red` first to write comprehensive failing tests
2. **Tests failing for right reasons** - Missing implementation, not test bugs
3. **Don't start** if failing test suite not ready
4. **If this branch is part of a PR stack**, complete **step 0** before any implementation — rebase onto the latest base, then confirm the parent work this PR consumes is actually on the branch. Do not skip that gate.

**Test style note**: `fluent-tests` is the mandatory test style for this repo. Tests are normally left unchanged in green phase. If a test adjustment is truly necessary, the edited test must remain compliant with `.agents/skills/fluent-tests/` (Given/When/Then, one behavior per test, named helpers, meaningful fixtures). Never weaken test structure to force passage.

## Workflow

### 0. If This PR Is Part of a Stack — Pre-run (HARD GATE)

Detect **before** reviewing tests or writing code, and bind every check to **this branch**. Another PR's document does not make this branch stacked.

A branch can be stacked in two ways, and both are in scope here:

- **Planned stack** — a `pr-stack` orchestrator session owns the DAG and spawned this session as a child node. The per-PR documents (`PRD.md`, `changeset.md`) are **attached to this session**; the `pr_*` tools belong to the orchestrator agent, not to you. See `docs/ft/coder/pr-stacking.md` and `docs/ft/coder/pr-stack-docs.md`.
- **Ad-hoc chain** — someone opened this PR on top of another open PR's branch, with no orchestrator. Detected the way `.agents/commands/pr.md` already does it.

```bash
BRANCH=$(git branch --show-current)

# 1. Planned stack: per-PR documents attached to this session
grep -l "## Draft PR contract" artifacts/attachments/changeset.md 2>/dev/null

# 2. This branch's open PR is based on something other than trunk
gh pr view --json baseRefName --jq .baseRefName 2>/dev/null

# 3. Planned branch convention `feature/<stack-slug>/<node>` with a sibling PR open
if [ -n "$BRANCH" ]; then
  case "$BRANCH" in
    feature/*/*)
      NS="feature/$(printf '%s' "$BRANCH" | cut -d/ -f2)/"
      gh pr list --state open --json headRefName \
        --jq ".[] | select(.headRefName | startswith(\"$NS\")) | .headRefName"
      ;;
  esac
fi
```

**Stack branch** if any of these hit:

1. `artifacts/attachments/changeset.md` exists and carries `## Responsibility`, `## Boundaries`, `## Dependencies` and `## Draft PR contract` — the four headings a child session spawned from a `pr-stack` orchestrator receives.
2. This branch's open PR has a `baseRefName` that is not `master` / `main`.
3. The branch matches `feature/<stack-slug>/<node>` and another branch under the same `feature/<stack-slug>/` namespace is open as a PR.

If none of those hit, skip this section — this is an ordinary branch, and `/green` behaves exactly as it always has.

An empty `git branch --show-current` (detached HEAD) is **not** a match — do not grep with it. Do **not** grep `docs/dev/1-WIP/` for stack wording without requiring this branch's name in the same file: another PR's changeset says nothing about this one.

**0a. Rebase first — `/pr-stack-rebase`**

Run **`/pr-stack-rebase`** (this branch only) before any implementation. If the branch is already on the latest base tip and `origin/<base>..HEAD` holds this PR's commits only, that command verifies and returns — that still counts as having run it.

Do **not** start implementing on a stale base. Do **not** kick off a whole-stack cascade from a per-PR worktree: syncing every node from here will fail on, or clobber, branches other worktrees have pinned. Whole-stack operations belong to the orchestrator session (`pr_resolve_conflicts`, `pr_repoint`), not to a child.

**0b. Dependency gate — MUST stop if unmet**

Read the attached `artifacts/attachments/changeset.md` before writing code:

- **`## Dependencies`** — per parent node, what *that* PR delivers that this one consumes.
- **`## Boundaries`** — what this PR explicitly must **not** do.
- **`## Draft PR contract`** — what a parent publishes early (API surface + failing tests) so dependents can branch off a real ref and compile against a real signature while that parent's implementation continues **in the same PR**.

After the rebase, check that everything this PR consumes from a parent is actually present on the branch. If it is not:

**STOP. Do not implement anything.** Ask the user how to proceed, and report:

- which parent-owned symbols are missing or unresolvable, with `file:line` where they are referenced;
- which parent node / PR owns each (node id, PR number, branch);
- why this PR needs them — which tests, which `## Dependencies` row;
- that you will **not** implement a parent-owned symbol from here.

**A missing dependency here means the parent has not merged or pushed yet — not that it deliberately left you a stub.** A parent is not permitted to ship a stubs-only PR: `docs/ft/coder/pr-stacking.md` § "PR boundary contract: every node is self-contained" requires the API change, the code implementing it and its tests to land in **one** node, and forbids splitting by layer. So the fix is almost always to wait for that parent's push and re-run `/pr-stack-rebase` — not to fill in its body. The `## Draft PR contract` is the sanctioned middle ground: a real signature to code against, with the owner still finishing the behaviour behind it.

Let the user choose — wait for the parent's push, code against the published draft signature with a test double at the seam, rescope the blocked tests, or something else they decide. Do not pick for them and do not silently continue.

### 1. Review Failing Tests

Understand what needs to be implemented:
- Which tests are failing?
- What functionality do they require?
- What API do tests expect?

Run tests to see current state:
```bash
cargo test -p package-name
# Or: cd packages/package-name && cargo test
```

### 2. Delegate to tdd-implementer Subagent

Delegate implementation directly:

**Invoke**: `tdd-implementer` subagent

**Provide to subagent**:
- The failing tests location and names
- Any context about the feature being implemented
- Expected API from tests (if known)

**The tdd-implementer will**:
- Analyze failing tests to understand requirements
- Implement production-quality code
- Focus on correct, quality implementation
- Verify each step with test runs
- Use minimal but proper implementation approach
- Keep tests unchanged (unless absolutely necessary)
- Report completion (tests may or may not pass - quality is priority)

**IMPORTANT**: If implementation is correct but tests still fail:
- That's acceptable - document why tests fail
- DO NOT compromise code quality to force passage
- May indicate test issues or incomplete understanding
- Better to have quality code with failing tests than bad code with passing tests

### 3. If This PR Is Part of a Stack — Commit, Push, Hand Off

If step 0 found this is **not** a stack branch, skip this section: on a standalone branch `/green` leaves committing to the user, as it always has. **The obligation below is stack-specific.**

Step 0 already rebased this branch and passed the dependency gate. If you did not run step 0, go back and run it before continuing.

**3a. Stay inside this PR's boundary**

- Implement only what `## Responsibility` in the attached `changeset.md` gives this PR.
- **Never implement a symbol listed under `## Dependencies`.** It belongs to a parent PR, whose own tests specify it. Two PRs implementing the same symbol is a guaranteed conflict, and the parent's version is the one that wins review.
- Never implement a dependent's behaviour, and never write a test here that asserts it.
- Never do anything `## Boundaries` rules out.

**3b. Commit and push — required on a stack branch**

Dependents are cut from **this** branch. Work sitting unpushed in this worktree does not exist for them: they keep building against whatever the last push left.

```bash
git add -A && git commit          # never --no-verify, never amend a pushed commit
git push origin "$(git branch --show-current)"
```

**Do not save it all for the end.** The end of green is the *latest* a stack branch may be pushed, not the cadence. Push **each milestone**: an owned symbol going from planned to real, a test group going green, and above all a **signature change to an owned symbol** — that one goes out on its own, immediately, because dependents compile against it.

**A milestone is production code that already passes its own tests** — a finished slice, not a progress checkpoint. Two hard preconditions before every push:

```bash
./test -p <package>    # the slice's own tests pass
cargo build            # the branch still builds
```

Other tests in this PR may still be red for parts not yet implemented; never the ones covering what you just pushed. **Never push half-finished implementation to "share progress"** — code that works for one case is worse for a dependent than nothing, because it surfaces as *their* bug. Not finished and green? Hold it and push when it is.

When a milestone unblocks a dependent, **say so** — that worktree needs to run `/pr-stack-rebase` to pick it up. Its own documents are the orchestrator's to update, not yours to edit from here.

If the push is rejected because the base moved, run **`/pr-stack-rebase`** (this branch only) and push again. Do not cascade a sync across the whole stack from here. Never `--force`; if a force-push is genuinely needed after a rebase, ask the user first and use `--force-with-lease`.

**3c. Hand off to `/validate-changes`**

Tell the user explicitly:

> This branch is part of a PR stack (`<branch>`, base `<base>`). Green is committed and pushed. **Next: run `/validate-changes`** — it re-checks the base, then validates this PR against its own `changeset.md` (responsibility delivered, boundaries respected, nothing borrowed from `## Dependencies`).

## Goals

1. ✅ Review and understand failing tests
2. ✅ Delegate to tdd-implementer subagent
3. ✅ Minimal production-quality implementation
4. ✅ Focus on functionality over form
5. ✅ Real code (not fake/hardcoded)
6. ✅ Quality implementation (tests passing is goal, not requirement)

## What NOT to Do

- ❌ Don't refactor existing code (unless necessary)
- ❌ Don't add extra features not covered by tests
- ❌ Don't focus on optimization or performance
- ❌ Don't make significant test structure changes
- ❌ **Don't add workarounds to make tests pass**
- ❌ **Don't use environment detection to force passage**
- ❌ **Don't ignore errors indicating real problems**
- ❌ **Don't hardcode values just to pass tests**

**CRITICAL**: If tests fail, fix implementation, not the test.

## Implementation Quality Principles

**CRITICAL**: Never compromise these for test passage:

### ✅ Acceptable - Quality Production Code

```rust
// ✅ Real production implementation
export function processData(input: Data): Result {
  if (!input || !input.value) {
    throw new Error('Invalid input');
  }

  return {
    processed: transformValue(input.value),
    timestamp: Date.now()
  };
}
```

**If tests fail with this quality code:**
- Document why they fail
- Investigate test vs. implementation mismatch
- DO NOT add workarounds

### ❌ NOT Acceptable - Compromised Code

```rust
// ❌ Hardcoded for tests
export function processData(input: Data): Result {
  if (input.value === 'test-value') {
    return { processed: 'expected-output', timestamp: 123 };
  }
  return { processed: '', timestamp: 0 };
}

// ❌ Test-specific branch
if (process.env.NODE_ENV === 'test') {
  return mockData;
}

// ❌ Workaround to force passage
try {
  return realImplementation();
} catch {
  return fakeResult; // Just to make test pass
}
```

**These are NEVER acceptable** - even if tests fail without them.

## Keep It Clean

**CRITICAL**: Never put "red phase" or "green phase" in:
- Code comments
- Test descriptions
- Production code

Remove any such comments if you see them.

## Output Format

```markdown
## 🟢 Green Phase Complete - Implementation Delegated

### Delegation to tdd-implementer ✅

**Delegated to**: `tdd-implementer` subagent

**Context provided to subagent:**
- Failing tests location and names
- Feature context and requirements
- Expected API from tests

**Result**: Quality implementation complete
- ✅ Production-quality code implemented
- 📊 Test status: [X passing, Y failing]
- 📝 If tests fail: [Reason and next steps]

### Implementation Created (by tdd-implementer)
- `src/feature.rs` - Main implementation
- `src/helpers.rs` - Supporting functions

### Implementation Summary
**Main functionality:**
- Implemented core feature logic (following plan)
- Added input validation (as planned)
- Implemented error handling (plan strategy)

**Code quality notes:**
- Minimal implementation (plan-guided)
- Production-quality code (not fake)
- Followed planned architecture
- TODO markers for future improvements

### Test Results
```bash
cargo test -p package-name
```

**Output:**
```
[✅ All tests passing | ⚠️ Some tests still failing]
- X tests passed
- Y tests failed
```

**If tests fail:**
- Analyze why: Implementation issue? Test issue? Misunderstanding?
- Document failure reasons
- DO NOT compromise code to force passage
- Consider if tests need adjustment or more work needed

### Test Modifications (by tdd-implementer)
[None | Minimal changes:]
- Adjusted import path in test setup
- [Explain why change was necessary]

### Code Quality ✅
- ✅ Real production code (not fake/hardcoded)
- ✅ Functional implementation
- ✅ No test-specific branches
- ✅ No workarounds
- ✅ Followed plan architecture
- ✅ Code quality NOT compromised for tests
- ⏭️ Refactoring may be needed (normal for green phase)

### FIXME/TODO Markers Added
- [ ] TODO: Add proper types (file:line)
- [ ] FIXME: Extract magic value to constant (file:line)
- [ ] If tests still failing: [Reason and investigation needed]

**Ready for Next Step**: ✅
- If all tests passing: Use `refactor` subagent to improve code
- If some tests failing: Investigate root cause, don't force passage
```

## Update Documentation

**If changeset/dev doc exists**: Update with implementation status.

## Success Criteria

Complete workflow successful when:

- ✅ Reviewed and understood failing tests
- ✅ Delegated to tdd-implementer subagent
- ✅ Quality implementation complete (production-quality, not fake)
- ✅ Minimal implementation (no over-engineering)
- ✅ Tests remain largely unchanged
- ✅ No workarounds added to tests
- ✅ Code quality NOT compromised
- 📊 Tests passing is goal, not hard requirement
- ✅ Ready for next step (refactor or investigation)
- ✅ **On a stack branch**: step 0 ran (rebase + dependency gate), nothing from `## Dependencies` was implemented here, and every milestone is committed and pushed

## Best Practices

✅ **Do:**
- Review failing tests before delegating
- Delegate to tdd-implementer subagent
- Provide test locations and names
- Provide any relevant context
- Wait for subagent completion
- Accept that tests may not all pass if code is correct
- Prioritize code quality over test passage
- Verify implementation is quality code

❌ **Don't:**
- Don't implement yourself (delegate it)
- Don't skip the delegation step
- Don't micromanage the subagent
- Don't demand tests pass at any cost
- Don't compromise code quality for test passage
- Don't add workarounds to force passage

## Related

**Rules**: `.cursor/rules/tdd.mdc`, `.cursor/rules/testing-practices.mdc`
**Subagents**: `tdd-implementer` (for implementation), `refactor` (next phase)
**Commands**: `/red` (previous phase), `/pr-stack-rebase` (step 0a, stack branches), `/validate-changes` (next, on a stack branch)
**Docs**: `docs/ft/coder/pr-stacking.md` (stack model, PR boundary contract), `docs/ft/coder/pr-stack-docs.md` (the per-PR `PRD.md` / `changeset.md`)

## Workflow Summary

```
/green command invoked
       ↓
0. Stack? → /pr-stack-rebase, then the dependency gate (HARD GATE)
       ↓
1. Review failing tests
       ↓
2. Delegate to tdd-implementer subagent
       ↓
   tdd-implementer analyzes tests and implements
       ↓
3. Stack? → commit + push each milestone, hand off to /validate-changes
       ↓
Quality implementation complete ✅
(Tests may or may not pass - quality is priority)
```

**Key Benefits**:
- Simple, direct delegation to implementation specialist
- Prioritizes code quality over forcing tests to pass
- Honest about test status - no workarounds to hide failures
- tdd-implementer determines approach from tests
