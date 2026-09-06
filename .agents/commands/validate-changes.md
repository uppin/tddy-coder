---
description: Analyze code changes in the current branch for risks in test infrastructure, production code, security, and code quality; rebase and gate the diff first on a PR-stack branch, sync the changeset with actual code state, and validate documentation when no context documents exist.
---

Analyze all code changes in the current branch for risks and correctness.

## Context Documents

**Expect in context:**
- Changeset (`docs/dev/1-WIP/YYYY-MM-DD-*.md`) — tracks implementation progress
- PRD (`docs/ft/*/1-WIP/YYYY-MM-DD-*.md`) — tracks requirement changes
- Dev docs (`packages/{package}/docs/*.md`) — stable technical reference
- Feature docs (`docs/ft/{product-area}/*.md`) — product requirements
- **On a PR-stack child session**, the attached per-PR documents
  (`artifacts/attachments/PRD.md`, `artifacts/attachments/changeset.md`) — this PR's responsibility,
  boundaries and dependencies. They are the source of truth for step 6a. **Load the `pr-stack` skill**
  (`.agents/skills/pr-stack/SKILL.md`) — it owns the boundary contract step 6a checks against.

Use them to scope expected changes, confirm the work matches what was planned, and update the
changeset's "Validation Results" section.

## Steps

### 0. Stack Detection and Rebase — HARD GATE Before Any Code Diff

Detect whether this branch is part of a PR stack **before** any `git diff`, any `git log` of the
delta, or any file-by-file analysis. A stacked `HEAD` contains its parents' commits; diffing it
against the wrong base reports their work as this PR's — and tempts an agent to "clean" the diff by
deleting parent-owned files.

This PR is in a stack when its base is another open PR's branch — detected the way
`.agents/commands/pr.md` already does it. Its plan lives in this branch's own `docs/dev/1-WIP/`
changeset.

```bash
BRANCH=$(git branch --show-current)

# 1. This branch's own changeset, carrying the four stack headings
grep -l "## Draft PR contract" docs/dev/1-WIP/*.md 2>/dev/null

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

1. This branch's `docs/dev/1-WIP/` changeset carries `## Responsibility`, `## Boundaries`,
   `## Dependencies` and `## Draft PR contract` — the four headings every PR in a stack has.
2. This branch's open PR has a `baseRefName` that is not `master` / `main`.
3. The branch matches `feature/<stack-slug>/<node>` and another branch under the same
   `feature/<stack-slug>/` namespace is open as a PR.

No hit (and the base is trunk, or there is no PR) → ordinary branch; skip to step 1 and diff against
`master` as usual. An empty `git branch --show-current` (detached HEAD) is **not** a match — do not
grep with it. Do **not** grep `docs/dev/1-WIP/` for stack wording without requiring this branch's
name in the same file: another PR's changeset says nothing about this one.

**On a stack branch, all of the following is required before any code diff in this run:**

1. Run **`/pr-stack-rebase`** (this branch only). If the branch is already on the latest base tip and
   the commit range is this PR's only, that command verifies and returns without rewriting — that
   still counts as having run it.
2. **Do not execute a code diff** until `/pr-stack-rebase` has finished **and** the leak check has
   passed: `git log --oneline origin/<base>..HEAD` contains **only this PR's own commits**. Any
   parent commit in that range is leaked work from another PR.
3. If the leak check fails, or `/pr-stack-rebase` cannot produce a leak-free range: **STOP.** Report
   it as a CRITICAL issue. Do **not** run `git diff`, do **not** treat the extra files as this PR's
   scope, and do **not** delete them to shrink the diff.

**Never "fix" a too-large stacked diff by deleting the extra files.** Extra files mean the base is
wrong or parent commits leaked in — go back to `/pr-stack-rebase`. Never `git rm`, never `git
checkout` over parent-owned files. Deleting a parent's work to make this PR's diff look right
destroys that work and is itself a CRITICAL finding.

The base `/pr-stack-rebase` resolved is the **only** base later steps may diff against.

### 1. Identify Changes

On a stack branch, `base` is the one step 0's `/pr-stack-rebase` resolved — **never assume `main`**.
On an ordinary branch, `base` is the default branch (`main`/`master`). Run every diff below against
that same `base`, and only once the step 0 gate has passed.

Run `git diff $base...HEAD --name-only` to get all changed files. Group them by package:

```
git diff $base...HEAD --name-only | grep "^packages/" | cut -d'/' -f1-2 | sort -u
```

Also run `git diff $base...HEAD --stat` and the full `git diff $base...HEAD` to read the actual change.

On a stack branch the file list must contain **only this PR's own files**. Anything else means the
base is wrong or a parent's work leaked in — CRITICAL, stop the analysis, and do not delete the
extra files.

### 2. Check for Changeset Document

Look in `docs/dev/1-WIP/` for an active changeset document related to this work:

```
find docs/dev/1-WIP -name "*.md" -exec grep -l "🚧 In Progress" {} \;
```

- **If a changeset exists:** Read it and use it as the source of truth for what this change intends
  to do. Extract its "Implementation Progress" / "Technical Changes" section, note the current item
  statuses (they may be outdated), and identify affected packages.
- **If no changeset exists and the changes are code-focused:** Ask the user whether to create one
  (following `.cursor/rules/changeset-doc.mdc`, named `docs/dev/1-WIP/YYYY-MM-DD-{descriptive-name}.md`,
  status `🚧 In Progress`, with affected packages, summary, technical changes, acceptance criteria)
  or skip changeset tracking.
- **If the changes are documentation-only:** Skip changeset creation — changesets track code-driven
  changes — and go to step 5.

### 3. Build Validation

For each affected Rust package, run:

```
cargo build -p <package-name>
```

Classify each: ✅ builds clean, ⚠️ builds with warnings in changed code, ❌ fails.

Report any build failures immediately — these block all further validation. Mark a failure as a
critical issue and block the PR until it is resolved. **Exception:** crates that require CI-only
environment variables (e.g. `ARTIFACT_VERSION`) — verify those via type-check plus tests instead.

Report build warnings only for changed files, with `file:line`.

### 4. Documentation Validation

If the changeset references context documents (PRD, design docs), verify the code changes align
with documented requirements. If no context documents exist, note this as a gap.

### 5. Documentation Quality Validation (no context documents)

Run this when there is no active changeset in `docs/dev/1-WIP/`, no active PRD in `docs/ft/*/1-WIP/`,
or when the changes are documentation-only.

**Feature documentation** (against `.cursor/rules/feature-doc.mdc`):
- Product area structure (`1-OVERVIEW.md`, correct titles)
- Feature document standards (correct template, acceptance criteria, status indicators)
- Asset management (all `appendices/` files referenced, no orphans)
- Changelog format (no broken PRD links, release-note style, says "PRDs" not "amendments"; see
  `docs/dev/guides/changelog-merge-hygiene.md` for indexes)

**Development documentation** (against `.cursor/rules/dev-doc.mdc`):
- Package README standards (single README, < 150 lines, no implementation details)
- Detailed docs structure (`packages/{package}/docs/` exists, comprehensive)
- Changesets history format (no broken changeset links, release-note style; **one file per entry**
  in `changesets/` named `YYYY-MM-DD-<slug>.md`, no index, no edits to existing entries — per
  `docs/dev/guides/changelog-merge-hygiene.md`)

### 6. Analyze Each Changed File

For every changed file, check for:

**Test infrastructure risks:**
- Tests that always pass (no real assertions)
- Tests that depend on external state or ordering
- Missing error case coverage
- Test helpers that silently swallow errors
- Mock implementations or test-only code paths in production code

**Production code risks:**
- Unwrap/expect calls without justification
- Missing error propagation
- Race conditions or shared mutable state
- Breaking API changes
- Hardcoded values that should be configurable
- Unsafe type assertions / missing null checks

**Security:**
- Hardcoded secrets, tokens or API keys
- Unsafe blocks without safety comments
- Unvalidated user input
- Injection and XSS vulnerabilities

**Code quality (see CLAUDE.md):**
- Direct stdout/stderr usage in TUI code paths (corrupts ratatui display)
- Fallbacks added without developer consent
- Code branches that only work in test environment
- Missing FIXME/TODO annotations on temporary code
- Long functions (>40 lines), deep nesting (>3 levels), magic values, duplicated code

**Stack boundary risks (stack branches only):**
- Parent-owned symbols implemented here, a dependent's behaviour anticipated, parent-owned files deleted
- See step 6a for the full check list

### 6a. Stack Boundary Checks (stack branches only)

Skip this on an ordinary branch. On a stack branch, validate the diff against **this PR's own plan** —
the attached `artifacts/attachments/changeset.md` and `artifacts/attachments/PRD.md`:

| Check | Source of truth | A gap looks like |
|-------|-----------------|------------------|
| Every changeset item implemented or explicitly deferred | this PR's `changeset.md` | an item with no code behind it and no recorded deferral reason |
| `## Responsibility` fully delivered | this PR's `changeset.md` → `## Responsibility` | a `TODO(...)` stub body, an `unimplemented!()`, or a function that only compiles |
| Nothing from `## Dependencies` implemented here | this PR's `changeset.md` → `## Dependencies` | a parent-owned symbol implemented **in this diff** |
| Nothing from `## Boundaries` crept in | this PR's `changeset.md` → `## Boundaries` | code or a test covering something this PR says it does not do |
| No dependent's behaviour | the dependent node's own plan | code or a test asserting a later PR's surface |
| Diff contains only this PR's files | the resolved base from step 0 | a file in the diff that no changeset item claims |
| No unplanned deletions of parent-owned files | the tree at `origin/<base>` | a path present at the base and gone at `HEAD` with no changeset item calling for its removal |

**Implementing a symbol listed under `## Dependencies` is a defect, not a bonus.** It guarantees a
conflict with the PR that owns it, and the owning PR's tests are the ones that specify it. Report it
as CRITICAL and hand it back — do not "keep" the extra work because it looks finished.

Leftover and trespassing stubs surface quickly:

```
grep -rn "TODO(\|unimplemented!(\|todo!(" packages/<package>/src
```

A stub inside `## Responsibility` is this PR's own gap. A body filled in for a symbol listed under
`## Dependencies` is a boundary trespass. Both are findings; they are not the same finding.

### 7. Sync and Update Changeset

If a changeset document exists, sync every item with what the code actually does, then write the
updates back. **The changeset must reflect actual code state after validation** — it becomes the
source of truth for implementation state.

- ✅ Complete — fully implemented in code
- ⚠️ In Progress — partially implemented
- 🔲 Not Started — no code changes found
- 🆕 Added — code change found that was not in the original changeset

Record in the changeset:
- Validation results per file
- Changeset sync results (items updated, items added, statuses corrected)
- Any refactoring needed before merge
- Risk assessment summary

### 8. Output Format

Present findings as:

```
## Risk Summary
- Critical: <count>
- Warning: <count>
- Info: <count>

## Stack Gate
(omit this section entirely on an ordinary branch)

| Check | Result |
|-------|--------|
| Stack branch | Yes — planned / ad-hoc (base `<base>`) |
| `/pr-stack-rebase` | ✅ Already current \| ✅ Rebased \| ❌ Could not resolve |
| Leak check (`origin/<base>..HEAD` is this PR only) | ✅ Clean \| ❌ Leaked: <commits> — analysis stopped |

## Stack Boundary
(omit this section entirely on an ordinary branch)

| Check | Result |
|-------|--------|
| Changeset items implemented or deferred | ✅ All \| ⚠️ <n> open \| ❌ <n> silently dropped |
| `## Responsibility` delivered | ✅ Complete \| ❌ Stubs remain (file:line) |
| `## Dependencies` not implemented here | ✅ Clean \| ❌ <symbol> implemented here, owned by <node/PR> |
| `## Boundaries` respected | ✅ Clean \| ❌ <what crept in> |
| No dependent's behaviour | ✅ Clean \| ❌ <what> |
| Diff contains only this PR's files | ✅ Clean \| ❌ <extra files> |
| Parent-owned files intact | ✅ Clean \| ❌ <deleted paths> |

## Changeset Sync

| Changeset Item | Old Status | Actual Code Status | Updated To |
|----------------|------------|--------------------|------------|
| Feature X | 🔲 Not Started | ✅ Implemented (a.rs, b.rs) | ✅ Complete |
| Test Y | ✅ Complete | ⚠️ Partial (3/5 tests exist) | ⚠️ In Progress |
| - | - | 🆕 Found: Optimization Z (c.rs) | 🆕 Added |

## Build Validation

| Package | Status | Notes |
|---------|--------|-------|
| tddy-core | ✅ Pass | Built successfully |
| tddy-daemon | ❌ Failed | Missing type definitions |

## Issues

### [CRITICAL] <file path>
<description and recommendation>

### [WARNING] <file path>
<description and recommendation>

### [INFO] <file path>
<description and recommendation>
```

If issues are found, ask the user whether to proceed with fixes or just report.

## Reference

- **Rules**: `.cursor/rules/coding-practices.mdc`, `.cursor/rules/changeset-doc.mdc`,
  `.cursor/rules/feature-doc.mdc`, `.cursor/rules/dev-doc.mdc`
- **Commands**: `/pr-stack-rebase` (step 0, stack branches), `/green` (its step 0 runs the same gate)
- **Docs**: the `pr-stack` skill (stack model, `pr_*` tools, PR boundary contract),
  the `pr-stack` skill § *Per-PR documents* (the per-PR `PRD.md` / `changeset.md` and their four headings)
