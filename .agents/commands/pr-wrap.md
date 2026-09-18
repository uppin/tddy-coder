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

**Establish whether this PR is in a stack before acting.** Detect it the way `/pr` does:
`gh pr list --state open --json number,headRefName,baseRefName` plus `git merge-base --is-ancestor`;
any base that is not `master`/`main` means this PR has a predecessor. Its branch follows
`feature/<stack-slug>/<node>`.

If it is:

- **Enter only after `/validate-changes` reported no gaps** against this PR's plan.
- **Step 0 is a hard gate** — `/pr-stack-rebase` for **this branch only**, never a whole-stack cascade
  from a per-PR worktree.
- **Wrapping covers only what this PR owns** — this PR's own `docs/dev/1-WIP/` pair, and only the
  `docs/dev/todo/` entries its own changeset claims; never a predecessor's. See `/wrap-context-docs`
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
   other layers is a whole-stack operation (`/pr-stack-rebase` cascade, `/repoint` — see
   the `pr-stack` skill). From here,
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

### 3.5. File Length Gate → Restructure

Check every non-test source file this PR touched against the **500-production-line budget** —
`/analyze-clean-code`'s "File length" metric and the `oversized-file` category of the
[`deferred-work`](../skills/deferred-work/SKILL.md) records — **before** quality scoring. This gate
exists because per-PR growth is invisible in a diff review: a module reaches four thousand lines
through twenty individually-wrapped PRs, each of which added eighty and looked fine.

It diffs the **whole PR range**, never `HEAD~1`, and it measures the way this repo's records measure:
**production lines only, counted to the first `#[cfg(test)]`** for Rust (`changeset.rs` is 964
production lines of 1,627 total, and 964 is the number its record carries).

```bash
base=$(gh pr view --json baseRefName --jq .baseRefName 2>/dev/null || echo master)
git fetch origin "$base" --quiet \
  || { echo "GATE ERROR: cannot fetch origin/$base — fix before proceeding, do not skip" >&2; exit 1; }
mb=$(git merge-base "origin/$base" HEAD) \
  || { echo "GATE ERROR: no merge-base with origin/$base — fix before proceeding, do not skip" >&2; exit 1; }

# Rust: lines before the first #[cfg(test)] (also matches #[cfg(all(test, …))], and does not
# match #[cfg(feature = "testing")]). Everything else: the whole file.
count_prod() {
  case "$1" in
    *.rs) awk '/^[[:space:]]*#\[cfg\(.*test[),]/ {exit} {n++} END {print n+0}' ;;
    *)    awk 'END {print NR+0}' ;;
  esac
}

changed=$(git diff --name-status -M "$mb"..HEAD -- \
    '*.rs' '*.ts' '*.tsx' '*.js' '*.jsx' '*.mjs' '*.cjs' '*.mts' '*.cts' '*.py' \
    ':(exclude)*/src/gen/*' ':(exclude)*/tests/*' ':(exclude)*.test.ts' ':(exclude)*.test.tsx' \
    ':(exclude)*.cy.ts' ':(exclude)*.cy.tsx' ':(exclude)*/cypress/*' ':(exclude)*/test-utils/*') \
  || { echo "GATE ERROR: git diff against $mb failed — fix before proceeding, do not skip" >&2; exit 1; }

printf '%s\n' "$changed" | while IFS=$'\t' read -r status old new; do
    [ -n "${status:-}" ] || continue
    f=${new:-$old}                                               # R/C rows carry old<TAB>new
    [ -f "$f" ] || continue                                      # skip deletions
    now=$(count_prod "$f" < "$f")
    was=$(git show "$mb:$old" 2>/dev/null | count_prod "$f")      # 0 only for a true add
    if [ "$now" -ge 500 ]; then printf '%s → %s\t%s\n' "$was" "$now" "$f"; fi
  done
```

An empty or unresolvable base must **abort the gate loudly**, never let it pass vacuously — a missing
`origin/$base` with an unguarded `merge-base` yields an empty range and a silently green gate.
Renames are resolved (`--name-status -M`): an unchanged 600-line file renamed by this PR counts as
`600 → 600` (alert-only below), not as a fake `0 → 600` crossing.

**What the exclusions are for**, and why none of them is optional:

- **`*/src/gen/*`** — committed `buf generate` output under the CI drift gate
  (`scripts/generated-code.sh check`, `scripts/generated-code.manifest`). Four of those files are
  already over a thousand lines; splitting one is reverted by the next regeneration and fails the
  drift check.
- **Test paths** (`*/tests/*`, `*.test.*`, `*.cy.*`, `*/cypress/*`, `*/test-utils/*`) — the budget is
  a *non-test source* metric, and this repo writes acceptance suites one scenario per file. A test
  file over the limit is a `duplicate-tests` or `misplaced-tests` question, not a decomposition.

**Alert the user**: every file the loop prints appears in this step's output **and** in the final
summary (step 9) as a 🔴 line with its before → after production-line counts — a clean quality score
in step 4 never absorbs or silences this alert. Then act by case:

| Case | Action |
|------|--------|
| This PR pushed the file past 500 production lines | 🔴 **Decompose now** |
| File was already ≥ 500 and this PR grew it further | 🔴 **Decompose now** |
| File was already ≥ 500 but this PR did not grow it | ⚠️ Alert only — record it as `packages/<pkg>/docs/code-issues/oversized-file-<slug>.md` (existing category, budget 500) and flag it in the summary |

**Two repo-specific stops that override "decompose now".** Both are alert-plus-record, and neither is
a silent pass:

- **A stack branch where a parent or dependent PR also touches that file** — step 4's standing rule.
  The rename fallout cascades through their diffs and turns every one into a conflict. Defer to a
  follow-up branch after the stack lands, and say so in the summary (step 9).
- **The file's code-issue record carries a `Claimed by:` line** — another PR is already in flight to
  fix exactly this. Verify the claim is still open (`deferred-work` § claims), then follow
  [AGENTS.md](../../AGENTS.md)'s claim protocol: **stop and ask** whether to proceed and add to the
  debt, wait for that PR, or narrow scope. Do not decompose underneath it.

**Decompose** — by language:

- **Rust**: load the [`code-restructuring`](../skills/code-restructuring/SKILL.md) skill. You do not
  write the moved code: plan intents, prove the seams with `restructure check --deep`, then apply.
  `restructure check --budget LINES` takes the budget directly. Its execution discipline applies in
  full — green baseline before (`./test -p <pkg>`), engine-driven intents, same green after.
- **TypeScript (and anything else the skill rejects — v1 is Rust-only)**: split by hand under the same
  discipline — green baseline before, mechanical moves only (no behaviour change), imports updated,
  same green after.

Either way the split lands **in this PR**: record the module layout and before → after production-line
counts in a Restructuring section of this PR's existing changeset. When `code-restructuring` is
entered from this gate, that section **stands in for** the skill's own `Type: Refactor` changeset and
its initial-discovery companion — skip the skill's changeset step rather than opening a second
changeset for the same PR.

- Deferring a decomposition **this PR caused** requires **explicit developer consent**, recorded in
  the summary with the reason **and** as a `docs/dev/todo/` entry
  ([`deferred-work`](../skills/deferred-work/SKILL.md)). Silence is not deferral.
- A file this PR did **not** grow is a `packages/*/docs/code-issues/` record and the next planner's
  Step 2b problem — file it and move on. Step 7.5 re-measures it at wrap.

**Calibration, measured 2026-09-18**: 191 non-test, non-generated source files in `packages/` are
already at or over 500 production lines, 67 of them over 1,000, the largest at 7,028. So the
alert-only row is the *common* case here, and for the multi-thousand-line modules the consent
deferral above is the expected answer rather than an exception. What the gate buys is that the number
is on the table every time, with the growth this PR caused attributed to it.

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
  says a predecessor delivers** (this PR's `## Dependencies`).
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
- **Clears the `docs/dev/todo/` entries this change resolved.** Before handing off, reconcile the
  changeset's `## Prerequisites` with what the branch actually did: an entry recorded ⚠ DURING or
  ⛔ BLOCKING that this work **fixed** is promoted to ✅ RESOLVED HERE, with its file link, so the wrap
  deletes it. Steps 1–5 are the last chance to notice that — after step 7 the changeset is gone
- **Reconciles the `packages/*/docs/code-issues/` records this change affected** — see step 7.5,
  which runs **before** the hand-off because the changeset is the instruction and it is about to be
  deleted
- Also cleans up `docs/superpowers/specs/` and `docs/superpowers/plans/` working docs once implementation is complete
- **Stack branch**: it runs in **Stack Mode** (see `/wrap-context-docs` § Stack Mode) — it wraps only the
  documents this PR owns in `docs/dev/1-WIP/` and `docs/ft/*/1-WIP/`, never a parent's, and never the
  `docs/dev/1-WIP/` pair, and it deletes only the backlog entries **this PR's own** changeset claims.
  In a stack, wrap **bottom-up**

### 7.5. Reconcile the code issues — re-measure, then delete, narrow or reopen

**Do this before `/wrap-context-docs` hands off**, and do it by **measuring**, not by asking
yourself whether the PR fixed something. A code issue
([`deferred-work`](../skills/deferred-work/SKILL.md)) carries a reproducible measurement in its
`**Detected:**` line — re-run it and let the number decide.

```bash
PR=<this PR number>
# every open issue in every package this PR touched — not only the ones the changeset names
for PKG in $(git diff --name-only origin/master...HEAD | grep -oE '^packages/[^/]+' | sort -u); do
  grep -rL 'Status:\*\* Resolved' "$PKG/docs/code-issues/" 2>/dev/null
done
grep -rl "Claimed by:.*#$PR\b" packages/*/docs/code-issues/      # the ones this PR promised to fix
```

**Scanning every open issue in a touched package — not just the claimed ones — is the point of this
step.** A PR that splits a file may take it under budget without anybody planning that, and a
measured record can confirm it objectively. This is discovery the TODO path explicitly forbids,
and it is safe here **only** because the answer is a number rather than a judgement.

| What the measurement says | Action |
|---|---|
| Clean | Record the final numbers in the changeset/changelog entry, then **`git rm` the record** |
| Better, not clean | **File stays.** Add the row, set `Status: Open — partially fixed (<what remains>)`, **narrow `## What would close it`** |
| Unchanged, but the PR touched that code | Add a row saying so — "unchanged" and silence are different facts |
| Worse | `**Status:** Open — regressed <date>`, with what grew it |
| The code moved | **Not a deletion.** Rename the record and add `**Moved:**` — the finding is elsewhere, not gone |

Then: if this PR carried `**Claimed by:** #<this PR>` and finished the work, the claim goes with the
deleted file. If it did **not** finish it, **keep the file, keep the claim, narrow the remainder**,
and name the follow-up that owns it.

⚠ **Deleting a partial fix is the failure mode here.** A closed record's information survives in the
changelog entry; a partial one's *remainder* exists nowhere else. A record that went from four
defects to two **stays**, with two.

⚠ **Record the final measurement before the `git rm`.** A deletion with no number left behind is the
one way this loses something worth keeping — and it is what makes a future regression traceable at
all, since it will otherwise read as a first detection.

⚠ **Delete on a number, never on the changeset's claim.** The changeset says where to look; the
re-measurement is the evidence.

**Stack branches:** reconcile only what **this PR's own** changeset claims, bottom-up. A record
claimed by a node further up the stack is left alone — the claim is still true.

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
  the title is the one
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
| Backlog entries this PR resolved are gone from `docs/dev/todo/` | every ✅ RESOLVED HERE entry in this PR's own changeset deleted; every one kept has a stated reason. A parent's claimed entries are **not** this PR's |
| No unchecked boxes in this PR's changeset | Scope and acceptance criteria all `[x]`, or annotated as deferred |
| Own temporary markers resolved | `TODO` / `FIXME` **this PR added** are implemented or deferred with a reason in the changeset |
| `/pr-stack-rebase` ran and the leak check passed | `origin/<base>..HEAD` is this PR's commits only |
| No unplanned deletions | every deleted path maps to this PR's changeset; no parent-owned file removed |
| Build and tests green | step 6 passed on the rebased tree |

**Parent-owned code, files and `TODO`/`FIXME` markers are NOT this PR's WIP** — they belong to the PR
that owns them and stay exactly as they are. Only what this PR added counts against the gate.

#### Top node only (K = N): sweep the stack's backlog delta

The **top** node is the stack's last chance to fix anything cheaply. Below it, every node's own
`docs/dev/todo/` handling is covered by the gate row above; here the subject is different — the
entries **the stack as a whole added or edited down**, from planning's out-of-scope ideas, `/green`'s
deferrals, and every ⚠ DURING verdict that recorded an entry the work touched and left.

```bash
BASE=$(git merge-base origin/master HEAD)
git diff --name-status "$BASE"..HEAD -- docs/dev/todo/     # A = added by the stack, M = edited down
```

For each, ask: *could this be fixed inside the surface this stack already owns, as one more node?*
The three verdicts and their criteria are in the `pr-stack` skill § *The backlog delta a stack
leaves*. Look hardest for an entry an early node deferred **for want of something a later node then
built** — the entry still states the original reason, which stopped being true inside this stack.

- **An *Extra node* verdict is the user's decision**, since it grows the stack: present the entry, the
  fix you would make and its size, and let them choose. If they take it, `/add-to-pr-stack` on this
  branch, and this PR is no longer the top — renumber per that command.
- **This does not block marking the PR ready.** *Leave it* is a normal answer; leaving the question
  unasked is not.
- Whatever is left stays in the backlog **with the reason written into the entry** — that is what the
  next planner's Step 2b reads. `/merge-pr-stack` Wave 1 asks the same question again before anything
  merges, and `/eval-changeset` reports the delta as part of what the stack cost.

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
  landing step's job (`/merge-pr-stack`, or an `#automerge` comment per
  [ci.md § Automerge](../../docs/dev/guides/ci.md)). A child session readies its own PR and stops.
- **Never delete the branch** — a dependent PR bases on it.

### 9. Display Summary

Present comprehensive summary with recommendations. On a stack branch, state the readiness-gate result,
whether the title was corrected, whether the PR was marked ready, and any restructure deferred to a
follow-up branch (steps 3.5 and 4).

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
| `code-restructuring` (skill) | Decompose files breaching the 500-production-line budget (step 3.5) |
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
[ ] 3.5. File length gate (≥500 production lines) → alert + code-restructuring
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
| 3.5 | file length gate → code-restructuring | ✅ / 🔴 N files ≥ 500 |
| 4 | /analyze-clean-code | ✅ |
| 4 | refactor | ✅ |
| 5 | /validate-changes | ✅ |
| 7 | /wrap-context-docs | ✅ |
| 8 | stack: title corrected → readiness gate → `gh pr ready` | ✅ / n/a |

### Summary
- **File Length**: ✅ none ≥ 500 / 🔴 `<file>`: was → now production lines (decomposed / deferred: reason / claimed by #NNN)
- **Code Quality**: X/10 ⭐
- **Tests**: All passing ✅
- **Production Ready**: ✅ Yes
- **Documentation**: ✅ Wrapped
- **Backlog**: N `docs/dev/todo/` entries deleted, M kept (with reasons)
- **Backlog delta** (top node only): N entries this stack added/edited — X routed to an extra node, Y left with reasons
- **Stack** (stack branches only): rebase ✅ · readiness gate ✅ · title `<final title>` · `gh pr ready <N>` ✅ / n/a

### 🎯 Recommendation

[If fit to ship:]
✅ **Code is ready for PR!**
Next step: Use `/pr` command to create pull request

[If fit to ship, stack branch with a draft PR already open:]
✅ **PR #N marked ready for review** — its dependents are readied only after this one, bottom-up.
Merging and repointing belong to `/merge-pr-stack`.

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
- **Let step 3.5's alert reach the user** — a file at or over 500 production lines is reported in the
  summary even when it was decomposed, claimed by another PR, or legitimately deferred; growth this PR
  caused is never silently deferred

❌ **Don't:**
- Don't skip validation steps
- Don't wrap incomplete changesets
- Don't proceed with failing tests
- Don't ignore subagent recommendations
- Don't decompose a generated file (`*/src/gen/*`) or a test suite to satisfy the gate — both are
  excluded from it by design
- Don't use `--no-verify` when committing or pushing
- Don't leave a `docs/dev/todo/` entry standing for a defect this branch fixed — and don't delete one
  the changeset never marked ✅ RESOLVED HERE
- **Stack**: don't delete parent-owned files, code or markers to shrink a stacked diff — extra files mean
  leaked ancestor commits; rebase, do not `git rm`
- **Stack**: don't split or restructure a file a parent or dependent PR also touches — defer it to a
  follow-up branch after the stack lands
- **Stack**: don't cascade a rebase over the whole stack from a per-PR worktree
- **Stack**: don't mark a PR ready while any of its own WIP remains, and don't mark a whole stack ready
  at once

## Related

**Related**: Subagent `refactor`, Commands `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`, `/wrap-context-docs`
**Skill**: `code-restructuring` (`.agents/skills/code-restructuring/SKILL.md`) — engine-driven file decomposition for step 3.5
**Commands**: `/pr` (next step), `/update-context-docs`
**Stack**: Commands `/green`, `/validate-changes`, `/pr-stack-rebase` · Docs the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`), [ci.md](../../docs/dev/guides/ci.md)
