---
description: Split code into separate sibling branches - both off master, no stack. For dependent PRs use /split-pr-to-stack instead.
---
## Split Branch — Export Code to Separate Branches

This command handles splitting current work into multiple **sibling** branches for better
organization. Both branches end up independent of each other — typically both off `master` — and
neither depends on the other to build, test or merge.

## Which command do you want?

| You want | Use |
|---|---|
| Two independent branches, either can merge first, neither needs the other | **this command** |
| Slice B must merge **before** slice A, and A is built on top of B | `/split-pr-to-stack` |
| A new PR **on top of** an existing one, with nothing to carve out | `/add-to-pr-stack` |
| A branch that just needs to follow the current one, no split at all | `/follow-up-branch` |
| The work isn't written yet and you are planning several PRs from requirements | `/plan-pr-stack` |

**The test that decides it:** after the split, can each branch build and pass its own tests with the
other branch absent? If yes → sibling split, this command. If the second slice would not compile
without the first, the slices are **dependent** and belong in a stack — stop and run
`/split-pr-to-stack`, which keeps the correct bases and re-registers the stack.
Do **not** produce two siblings and paper the dependency over with a stub or a fallback; fallbacks
require explicit developer consent (`CLAUDE.md`).

Each resulting branch becomes its own PR, so each is judged on its own. The
the `pr-stack` skill § *The PR boundary contract*
is binding inside a stack, but its principle applies here too: split by **capability**, not
by layer. "Branch 1 has the API, branch 2 has the implementation" is not two branches — it is one
branch cut in half, and neither half is reviewable.

## Use Cases

1. **Move all code to new branch** (default if unspecified)
2. **Split functionality** between current and new branch

## Prerequisites

Understand what needs to be split:
- What goes to new branch?
- What stays in current branch?
- Is this a full move or partial split?
- Can each side build and test on its own? (If no → `/split-pr-to-stack`.)

## Preflight — check what the current branch is already committed to

Before touching anything, find out whether the branch you are about to rewrite is somebody's
published or tracked work. This matters because Step 4A resets the original branch **hard**.

```bash
git branch --show-current
git status --porcelain
git worktree list
gh pr list --state open --head "$(git branch --show-current)" --json number,url,baseRefName
gh pr list --state open --json number,headRefName,baseRefName   # is anything based on this branch?
```

Stop and ask the user if any of these hold:

- **The branch has an open PR.** A hard reset rewrites its whole diff. Either force-push knowingly
  (`--force-with-lease`, never plain `--force`) or pick a different branch to carry the leftover.
- **Another open PR is based on this branch.** Resetting it breaks that PR's base and its diff will
  swallow whatever is left. This is a stack, not siblings — use `/split-pr-to-stack`.
- **The branch is part of a stack.** The **branch is the durable link key**: an open PR points at
  it and successors are based on it, so rewriting it out from under them silently changes what that
  PR ships. Ask its owner first; reshaping a stack is `/split-pr-to-stack` plus a re-registration,
  not a reset here.
- **The branch is checked out in another worktree** (`<repo>/.worktrees/<name>` for session
  worktrees). Free it first, or work from a worktree that does not pin it — git will refuse
  otherwise, usually halfway through.

## Workflow

### 1. Commit Uncommitted Code

Ensure all changes are committed before proceeding:
```bash
git add .
git commit -m "Work in progress before split"
```

Never `--no-verify`. If a pre-commit hook blocks the commit, fix what it is complaining about.

### 2. Create Backup Branch

**🚨 SUPER CRITICAL: Keep backup frozen (never modify)**

```bash
git branch backup/<current-branch>-$(date +%Y-%m-%d-%H-%M)
```

This preserves original state for recovery if needed. Record the SHA it points at — you will quote
it in the output.

### 3. Create Split Document

Create `tmp/split-YYYY-MM-DD-HH-MM.md` (`tmp` is gitignored — don't add it to git):

```markdown
# Branch Split - YYYY-MM-DD HH:MM

## Branches
- **Original**: [current-branch-name]
- **New**: [new-branch-name]
- **Backup**: backup/[current-branch-name]-YYYY-MM-DD-HH-MM (FROZEN - do not modify)
- **Original SHA**: [sha]

## Split Type
[Full Move | Partial Split]

## Independence check
- New branch builds and tests with the original absent: [yes/no]
- Original branch builds and tests with the new one absent: [yes/no]
- (A "no" on either line means these slices are a stack — use /split-pr-to-stack)

## Functionality Distribution

### Goes to New Branch
- Feature/functionality 1
- Feature/functionality 2
- Files: [list key files — packages/<pkg>/src/<module>.rs, packages/<pkg>/tests/<name>.rs, ...]

### Stays in Original Branch
- Feature/functionality 3
- Feature/functionality 4
- Files: [list key files]

## Procedure Followed
1. Created backup branch ✅
2. Created new branch from current ✅
3. Reset new branch to origin/master ✅
4. Removed non-relevant functionality from new branch ✅
5. Built, linted and committed new branch ✅
6. Switched back to original branch ✅
7. Removed moved functionality from original ✅
8. Built, linted and tested original branch ✅

## Status
[In Progress | Complete]
```

### 4. Execute Split

#### A. Full Move (Default)

Move all current code to new branch:

```bash
# Create new branch from current (includes all changes)
git switch -c new-feature-branch

# new-feature-branch now carries all the work.
# The original branch still points at the same commits and needs resetting.

# Switch back to original
git switch original-branch

# Reset to master (removes all changes)
git reset --hard origin/master
```

**`git reset --hard` is destructive and unrecoverable except via the backup from Step 2.** Confirm
with the user first, and confirm again if the Preflight found an open PR, a dependent PR, a stack
node, or another worktree on this branch. If the original branch has already been pushed, the reset
also means the next push must be `--force-with-lease` — say so before doing it.

Fetch first so `origin/master` is not stale:

```bash
git fetch origin master
```

#### B. Partial Split

Split functionality between branches:

**Step 1: Create new branch**
```bash
git switch -c new-feature-branch
```

**Step 2: Reset to master (uncommits all changes, keeps them in the worktree)**
```bash
git fetch origin master
git reset origin/master
# All changes now uncommitted in new branch
```

Note this is a **mixed** reset (no `--hard`): the commits go away, the files stay. That is what makes
the next step possible.

**Step 3: Remove functionality staying in original**

Carefully revert files/changes that should NOT be in new branch:
```bash
git restore --source=origin/master --worktree -- packages/<pkg>/src/<module>.rs
# Or manually delete/modify files
```

For a file that exists only in the original work and belongs there, delete it outright. For a file
both slices touch, keep only the hunks this branch needs — `git add -p` is the tool for that.

**Step 4: Commit new branch**
```bash
git add .
git commit -m "Split: [new branch functionality]"
```

Verify it builds and passes its own tests **with the other slice absent**:
```bash
cargo build
cargo clippy -- -D warnings
cargo fmt
./test -p <touched-package>      # fast loop
./test                           # full workspace before pushing
```

`./test` writes its output to `.verify-result.txt` as well as the terminal. When terminal capture is
unreliable, **read that file** — do not claim tests pass on an exit code you could not see
(`CLAUDE.md` § Agent Verification).

**Step 5: Switch to original branch**
```bash
git switch original-branch
```

**Step 6: Remove functionality moved to new branch**

"Negative" changes — remove what went to the new branch:
```bash
# Identify what was added in the new branch
git diff original-branch new-feature-branch

# Remove those changes from original
# Manually delete/modify files
```

**Step 7: Commit original branch**
```bash
git add .
git commit -m "Split: removed [moved functionality]"
```

Verify the same way:
```bash
cargo build
cargo clippy -- -D warnings
./test
```

### 5. Verify Both Branches

**New branch:**
```bash
git switch new-feature-branch
cargo build
cargo clippy -- -D warnings
./test
```

**Original branch:**
```bash
git switch original-branch
cargo build
cargo clippy -- -D warnings
./test
```

Both must build, lint clean, and have working tests. Two further checks that a plain build will not
catch:

- **Tests went with their code.** A slice whose tests all landed on the other branch is not
  independently reviewable, whatever `cargo build` says. Each branch should own the tests that prove
  its own behaviour.
- **No leftover stubs.** If either branch needed an `unimplemented!()`, a `todo!()` or a `TODO(split)`
  to compile, the slices are dependent — restore from backup and use `/split-pr-to-stack`. Any
  temporary code that legitimately survives must be marked `TODO` / `FIXME` (`CLAUDE.md`).

Web packages (`packages/tddy-web`) are verified with the bun workspace instead:
```bash
./dev bun run build
./dev bun run cypress:component
```

### 6. Update Split Document

Mark procedure complete in `tmp/split-*.md`:
```markdown
## Status
✅ Complete

## Verification
- New branch builds (`cargo build`): ✅
- New branch clippy clean: ✅
- New branch tests pass (`./test`): ✅
- Original branch builds: ✅
- Original branch clippy clean: ✅
- Original branch tests pass: ✅

## Final State
- New branch: [commits, functionality summary]
- Original branch: [commits, functionality summary]
- Backup: [commit hash] (preserved)
```

### 7. Documentation and changesets

If the work being split has a changeset in `docs/dev/1-WIP/`, split it the same way the code was
split: each branch keeps the part describing what it now ships, and neither describes the other's
work. Record any new changeset as its own file in `docs/dev/changesets/` — `YYYY-MM-DD-<slug>.md`, one
per branch with a **distinct slug**, per `docs/dev/guides/changelog-merge-hygiene.md`.

**Never edit `packages/*/docs/` directly** — package docs change through the changeset workflow in
`docs/dev/1-WIP/`. Product requirements live in `docs/ft/<area>/`.

## Output Format

```markdown
## 🔀 Branch Split Complete

### Branches Created
- **New**: `new-feature-branch` - [functionality summary]
- **Original**: `original-branch` - [functionality summary]
- **Backup**: `backup/<branch>-YYYY-MM-DD-HH-MM` - 🔒 Frozen (recovery only)

### Split Type
[Full Move | Partial Split]

### Independence
- New branch builds/tests with the original absent: ✅
- Original branch builds/tests with the new one absent: ✅
- No stubs or fallbacks were added to make either side compile: ✅
- (If either is ❌ these slices are a stack — see `/split-pr-to-stack`)

### New Branch
**Functionality:**
- Feature A
- Feature B
- [X files changed]

**Status:**
- `cargo build`: ✅
- `cargo clippy -- -D warnings`: ✅
- `./test`: ✅ (evidence: `.verify-result.txt`)
- Commit: [commit hash]

### Original Branch
**Functionality:**
- Feature C
- Feature D
- [Y files changed]

**Status:**
- `cargo build`: ✅
- `cargo clippy -- -D warnings`: ✅
- `./test`: ✅ (evidence: `.verify-result.txt`)
- Commit: [commit hash]
- Force-push required (branch was reset): yes/no

### Backup Branch
**Commit**: [commit hash]
**Purpose**: Recovery if needed
**⚠️ NEVER MODIFY THIS BRANCH**

### Split Document
Created: `tmp/split-YYYY-MM-DD-HH-MM.md`
(Not in git - local reference only)

### Verification Results
- ✅ Both branches build and lint clean
- ✅ Both branches' tests pass
- ✅ Functionality correctly distributed, each branch owns its own tests
- ✅ Backup preserved

### Next Steps
1. Continue work on new branch: `git switch new-feature-branch`
2. Continue work on original: `git switch original-branch`
3. Open a PR per branch: `/pr` (it detects the base itself — do not assume `master`)
4. Backup available if needed: `git switch backup/<branch>-YYYY-MM-DD-HH-MM`
```

## Recovery Process

If split went wrong, recover from backup:

```bash
# Delete failed branches (ask first — CLAUDE.md: ask before deleting)
git branch -D new-feature-branch

# Restore from backup
git switch backup/<branch>-YYYY-MM-DD-HH-MM
git switch -c original-branch-recovered

# Start split process again
```

If the original branch was already force-pushed after a bad reset, restore it from the backup and
force-push again with `--force-with-lease`. **Never delete a branch that an open PR is based on** —
deleting a base branch **closes** its dependent PR on GitHub.

## Best Practices

✅ **Do:**
- Run the Preflight before rewriting anything
- Always create backup branch first
- Verify each branch builds, lints and tests **with the other absent**
- Keep each branch's own tests on that branch
- Document split in `tmp/` (gitignored)
- Keep backup branch frozen
- Use `--force-with-lease` if a pushed branch has to be rewritten

❌ **Don't:**
- Don't skip backup creation
- Don't modify backup branch
- Don't `git reset --hard` a branch with an open PR, a dependent PR, or a stack node without asking
- Don't add a stub or a fallback to make a slice compile — that means it isn't a sibling split
- Don't forget to build/lint/test both branches
- Don't lose track of what goes where
- Don't add the split document to git
- Don't use `--no-verify` to get a commit or a push through
- Don't proceed with broken code

## Common Pitfalls

**Incomplete functionality separation:**
- Files shared between features not properly split
- Dependencies not updated correctly
**Fix**: Carefully analyze file-by-file what each branch needs; `git add -p` for shared files

**Broken imports / unresolved paths after split:**
- Code references removed functionality — in Rust this shows as `unresolved import`,
  `cannot find function`, or a `mod` declaration pointing at a deleted file
**Fix**: Update `use` statements, `mod` declarations and `Cargo.toml` members on both branches. If
the reference cannot be removed because the code genuinely needs the other slice, these are stacked
PRs, not siblings — use `/split-pr-to-stack`

**Tests failing after split:**
- Test helpers, fixtures or a testkit crate not properly distributed
**Fix**: Ensure test infrastructure is complete on both branches. Shared helpers usually belong on
whichever branch merges first, or on `master` as a separate small PR

**Clippy fails on one branch only:**
- Dead code, unused imports or unused `mod` left behind by the removal
**Fix**: `cargo clippy -- -D warnings` on both branches is part of verification, not an afterthought

**One branch silently became the other's dependency:**
- A trait, type or module the second branch uses only exists on the first
**Fix**: This is a stack. Restore from backup and use `/split-pr-to-stack`

## Related

**Commands**: `/split-pr-to-stack` (when the slices must be a **stack** — `A→B1→B2`, not two
siblings off `master`), `/add-to-pr-stack` (a new PR on top of an existing one),
`/follow-up-branch` (a branch that just follows, nothing to carve out), `/pr` (open a PR per
branch — it detects the base), `/merge` (if branches need merging later), `/pr-wrap`
**Skill**: `pr-stack` (`.agents/skills/pr-stack/SKILL.md`) — only if the split should land as
stacked PRs; otherwise this command stays a sibling-branch split.
**Specs**: the `pr-stack` skill (the PR boundary contract and the stack data model),
`docs/dev/guides/testing.md`, `docs/dev/guides/changelog-merge-hygiene.md`
