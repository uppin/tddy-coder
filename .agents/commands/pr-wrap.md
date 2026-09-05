---
description: Comprehensive PR preparation workflow using subagents
---
## PR Wrap - Prepare Changes for Pull Request

This command orchestrates comprehensive PR preparation by invoking specialized subagents for each step.

**Goal**: Ensure code is clean, maintainable, tested, and production ready.

## Stack Branches — Read First

If this branch is one PR of a stack, `/pr-wrap` is the **last** step of the in-stack loop:

```
/green → /validate-changes → (gaps? back to /green) → /pr-wrap
```

Each of the three runs `/pr-stack-rebase` before it reads a code diff — including this one (step 0).

**Load the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`) before acting** — it owns the base
resolution, the PR title convention and the landing order this command depends on.

**Say which kind of stack this is before acting** (see [pr-stacking.md](../../docs/ft/coder/pr-stacking.md)):

- **Planned stack** — a `pr-stack` orchestrator session owns the DAG and spawned this child session. The
  orchestrator's per-PR documents are attached here as `artifacts/attachments/PRD.md` and
  `artifacts/attachments/changeset.md` (plus `pr-stack-plan.md` and `exploration.md`), and the branch
  follows `feature/<stack-slug>/<node>`. The `pr_*` tools belong to the **orchestrator** agent; this
  session does not have them.
- **Ad-hoc chain** — a PR opened on top of another open PR's branch, with no orchestrator. Detect it the
  way `/pr` does: `gh pr list --state open --json number,headRefName,baseRefName` plus
  `git merge-base --is-ancestor`; any base that is not `master`/`main` is a stack parent.

On either kind:

- **Enter only after `/validate-changes` reported no gaps** against this PR's plan.
- **Step 0 is a hard gate** — `/pr-stack-rebase` for **this branch only**, never a whole-stack cascade
  from a per-PR worktree.
- **Wrapping covers only what this PR owns** — the per-PR `PRD.md` / `changeset.md` attached from the
  orchestrator are session artifacts, not repo files, and are never wrapped. See `/wrap-context-docs`
  § Stack Mode.
- **Correct the PR title before marking ready** (step 8) — this is the last moment it can be fixed.
- **Mark ready per-PR and bottom-up** (`gh pr ready <N>`). Never flip a whole stack ready at once:
  dependents further up may still be being implemented.
- **A stacked PR goes up for review only with nothing of its own still in flight** — own docs wrapped,
  own boxes ticked, own temporary markers resolved. Parent-owned code and markers are not this PR's WIP;
  **do not delete them**.
- **Never delete the branch** after readying — a dependent PR bases on it.

## Prerequisites

- Branch contains the work you intend to PR (prefer committing **after** step 6 so fmt/clippy/test and hooks stay green — **do not** use `--no-verify` on commit or push)
- Changeset document in context (if applicable): `docs/dev/1-WIP/YYYY-MM-DD-*.md`
- PRD document in context (if applicable): `docs/ft/*/1-WIP/PRD-YYYY-MM-DD-*.md`

## Workflow Steps

Execute these steps in order, using the specified subagent for each:

### 0. Stack Rebase — HARD GATE (stack branches only)

Ordinary branch → skip to step 1. Stack branch (either kind, detected above) → **required, not optional**:

1. **Run Command**: `/pr-stack-rebase` — **this branch only**, before step 1 and before any validation
   that reads a code diff. The tree must be clean (see Prerequisites).
2. Confirm the leak check passed — `git log --oneline origin/<base>..HEAD` lists **only this PR's
   commits** (`<base>` is the parent's branch for a stacked node, not `master`). If it does not, **stop
   `/pr-wrap`**: do not run `git diff`, do not start `/validate-changes`, and never `git rm` files to
   shrink the diff. Extra files mean leaked ancestor commits — rebase, do not delete.
3. **Never cascade a rebase over the whole stack from a per-PR worktree.** Re-basing and repointing the
   other nodes is the orchestrator's job (`pr_repoint`, `pr_resolve_conflicts` — see
   [pr-stacking.md § PR-management tools](../../docs/ft/coder/pr-stacking.md)). From here,
   `/pr-stack-rebase` is the only rebase path.
4. The nested `/validate-changes` runs in steps 1 and 5 rebase again (cheap when already current) — do
   not skip either invocation.

Do not proceed to step 1 until this gate passes.

### 1. Validate Changes → Refactor

**Run Command**: `/validate-changes`
- Analyze code changes for risks
- Update changeset "Validation Results" section
- **Stack branch**: if this run reports a leak or unplanned deletions, **stop `/pr-wrap`** — rebase or
  restore the parent-owned code; do not delete extra files

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

**On a stack branch, two cleanups are off-limits:**

- **Never split or restructure a file that a parent or dependent PR in the stack also touches.** The
  rename fallout cascades through their diffs and turns every one of them into a conflict. Record the
  finding, **defer the split to a follow-up branch after the stack lands**, and note the deferral in the
  summary (step 9).
- **Never apply a cleanup that deletes parent-owned files, or code this PR's `## Dependencies` section
  says a parent delivers** (the attached `artifacts/attachments/changeset.md` for a planned stack node).
  That is not dead code — it belongs to another PR, and deleting it here surfaces as loss in the base.

### 5. Final Validation

**Run Command**: `/validate-changes`
- Re-validate after all refactoring
- Ensure no new issues introduced
- **Stack branch**: this nested run rebases again before it diffs (verify-and-return when already
  current). A leak, an unplanned deletion, or a new gap vs the plan **stops `/pr-wrap`**

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
- Also cleans up `docs/superpowers/specs/` and `docs/superpowers/plans/` working docs once implementation is complete
- **Stack branch**: it runs in **Stack Mode** (see `/wrap-context-docs` § Stack Mode) — it wraps only the
  documents this PR owns in `docs/dev/1-WIP/` and `docs/ft/*/1-WIP/`, never a parent's, and never the
  per-PR `PRD.md` / `changeset.md` attached from the orchestrator session. In a stack, wrap **bottom-up**

### 8. Stack Only: Correct the Title & Mark Ready for Review

**Stack branch — required, not optional.** Ordinary branch → skip to step 9 (`/pr` handles the title
when it opens the PR).

#### Re-read and correct the PR title — MANDATORY

**This is the last moment the title can be fixed.** The repo squash-merges, so the title becomes the
commit subject on `master` permanently — that is where the `… (#433)` subjects in `git log` come from
(see [ci.md § Automerge](../../docs/dev/guides/ci.md)). Retitling a merged PR changes the PR page and
leaves the commit untouched. The title was written at planning, before the implementation existed, and
**scope drifts during green** — so re-read it against what the branch actually delivers:

- **It states what the branch delivers, in its finished state.** The changeset's `## Summary` is usually
  the corrected wording.
- **No process artifact**: `red`, `green`, `stubs`, `failing tests`, `WIP`, `phase N`.
- **Not a branch slug** — `pr 4 production wiring` is what a tool leaves behind when nobody replaces it.
- **Keep the stack position group if the PR carries one.** Read it from the PR's **own current title**:
  the per-PR documents live on the orchestrator session, not on this branch, so the title is the one
  carrier that travels with the PR.

```bash
gh pr view <N> --json title --jq '.title' | grep -oE '\(#[a-z0-9-]+ [0-9]+/[0-9]+\)$'   # e.g. (#auth 2/4)
```

If that yields nothing, the title was never set in that format — leave the group out rather than
inventing a position, and say so in the summary. Then:

```bash
gh pr edit <N> --title "<type>(<package>): <what it delivers> (#<stack-slug> K/N)"
```

#### Readiness gate — no WIP of this PR's own may remain

| Check | Passes when |
|---|---|
| Own changeset wrapped out of `docs/dev/1-WIP/` | content transferred into package/feature docs, source deleted (step 7) |
| Own PRD wrapped out of `docs/ft/*/1-WIP/` | same, or deferred with a written reason |
| No unchecked boxes in this PR's changeset | Scope and acceptance criteria all `[x]`, or annotated as deferred |
| Own temporary markers resolved | `TODO` / `FIXME` **this PR added** are implemented or deferred with a reason in the changeset |
| `/pr-stack-rebase` ran and the leak check passed | `origin/<base>..HEAD` is this PR's commits only |
| No unplanned deletions | every deleted path maps to this PR's changeset; no parent-owned file removed |
| Build and tests green | step 6 passed on the rebased tree |

**Parent-owned code, files and `TODO`/`FIXME` markers are NOT this PR's WIP** — they belong to the PR
that owns them and stay exactly as they are. Only what this PR added counts against the gate.

#### Push, then mark this PR ready

```bash
git push origin $(git branch --show-current)     # never --no-verify
gh pr ready <N>                                   # this PR only
gh pr view <N> --json isDraft,baseRefName,title   # expect isDraft=false, base still the parent branch
```

- **Bottom-up**: a parent's PR is readied (and wrapped) before its dependents'. Do not mark a whole stack
  ready at once — dependents further up may still be being implemented, and publishing them puts
  unfinished work in front of reviewers.
- **Do not merge from here.** Merging the stack and repointing each base after a merge is the
  orchestrator's job (`pr_merge` / `pr_repoint`, or an `#automerge` comment per
  [ci.md § Automerge](../../docs/dev/guides/ci.md)). A child session readies its own PR and stops.
- **Never delete the branch** — a dependent PR bases on it.

### 9. Display Summary

Present comprehensive summary with recommendations. On a stack branch, state the readiness-gate result,
whether the title was corrected, whether the PR was marked ready, and any restructure deferred to a
follow-up branch (step 4).

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
| `/pr-stack-rebase` | Stack only: rebase this branch before any code diff (step 0) |

## Tracking Progress

Create TODO list and mark each step complete:

```
[ ] 0. Stack only: /pr-stack-rebase (always, before any code diff)
[ ] 1. /validate-changes → refactor
[ ] 2. /validate-tests → refactor
[ ] 3. /validate-prod-ready → refactor
[ ] 4. /analyze-clean-code → refactor
[ ] 5. Final validation
[ ] 6. Linting & type checking
[ ] 7. Documentation update/wrap
[ ] 8. Stack only: title corrected + readiness gate + `gh pr ready`
[ ] 9. Summary
```

## Output Format

```markdown
## 🎯 PR Preparation Complete

### Subagents Invoked
| Step | Subagent | Status |
|------|----------|--------|
| 0 | /pr-stack-rebase (stack: always, before any diff) | ✅ / n/a |
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
| 8 | stack: title corrected → readiness gate → `gh pr ready` | ✅ / n/a |

### Summary
- **Code Quality**: X/10 ⭐
- **Tests**: All passing ✅
- **Production Ready**: ✅ Yes
- **Documentation**: ✅ Wrapped
- **Stack** (stack branches only): rebase ✅ · readiness gate ✅ · title `<final title>` · `gh pr ready <N>` ✅ / n/a

### 🎯 Recommendation

[If fit to ship:]
✅ **Code is ready for PR!**
Next step: Use `/pr` command to create pull request

[If fit to ship, stack branch with a draft PR already open:]
✅ **PR #N marked ready for review** — its dependents are readied only after this one, bottom-up.
Merging and repointing are the orchestrator's to run.

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
- **Stack branches**: run `/pr-stack-rebase` before every code diff, correct the title before readying,
  and ready bottom-up

❌ **Don't:**
- Don't skip validation steps
- Don't wrap incomplete changesets
- Don't proceed with failing tests
- Don't ignore subagent recommendations
- Don't use `--no-verify` when committing or pushing
- **Stack**: don't delete parent-owned files, code or markers to shrink a stacked diff — extra files mean
  leaked ancestor commits; rebase, do not `git rm`
- **Stack**: don't split or restructure a file a parent or dependent PR also touches — defer it to a
  follow-up branch after the stack lands
- **Stack**: don't cascade a rebase over the whole stack from a per-PR worktree
- **Stack**: don't mark a PR ready while any of its own WIP remains, and don't mark a whole stack ready
  at once

## Related

**Related**: Subagent `refactor`, Commands `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`, `/wrap-context-docs`
**Commands**: `/pr` (next step), `/update-context-docs`
**Stack**: Commands `/green`, `/validate-changes`, `/pr-stack-rebase` · Docs [pr-stacking.md](../../docs/ft/coder/pr-stacking.md), [pr-stack-docs.md](../../docs/ft/coder/pr-stack-docs.md), [ci.md](../../docs/dev/guides/ci.md)
