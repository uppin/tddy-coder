---
description: Safely merge incoming changes from master or another branch into the current working branch.
---

# Merge Workflow

Merge incoming changes (from main/master or another branch) into the current working branch safely. If no branch is specified, the incoming branch is **whatever this branch is really built on**, refreshed to latest — which is `master` only for a standalone branch.

> **On a stack branch, `/merge` is usually the wrong command.** A stack branch is built on its parent
> PR's branch, and merging `master` into it drags in commits the parent has not landed yet, inflating
> this PR's diff with work reviewers cannot attribute. Use **`/pr-stack-rebase`** instead — it rebases
> onto the latest tip of *this branch's own base* and verifies `origin/<base>..HEAD` contains only
> this PR's commits. Reach for `/merge` on a stack branch only when a rebase is genuinely unwanted
> (a shared branch others have already branched from), and say why in the merge document.
>
> See the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`).

## Process

### 0. Resolve what to merge *from*

Determine the incoming branch, in this order:

1. **An explicit argument** — the user named a branch. Use it.
2. **This branch's open PR base** — `gh pr view --json baseRefName --jq .baseRefName`. This is authoritative: it is what GitHub computes the PR's diff against.
3. **Planned stack** — the parent node's branch, named under `## Dependencies` in the per-PR `changeset.md` attached to this session. A node may have several parents; merge from each in turn, or use `/pr-stack-rebase`.
4. **Default branch** — `git symbolic-ref --short refs/remotes/origin/HEAD` (fallback `master`). Only when 1–3 found nothing.

`git fetch origin <incoming>` before comparing anything. State plainly which rule resolved the base, and stop and ask if 2 and 3 disagree.

### 1. Backup

- Create a backup branch from the current HEAD: `git branch backup/<current-branch>-<date>`.
- Confirm the backup was created.

### 2. Analyze Changes

- Identify the current branch and the incoming branch resolved in step 0.
- Run `git log --oneline HEAD..origin/<incoming>` to see incoming commits.
- Run `git diff HEAD...origin/<incoming>` to understand what will change.
- Identify potential conflict areas by comparing changed files on both sides.
- **On a stack branch**, check the incoming commits against this PR's `## Boundaries` and `## Dependencies`. Commits that belong to a parent PR are expected to arrive; anything else appearing in *this* PR's diff afterwards is a leak, and the fix is `/pr-stack-rebase`, never deleting the files.

### 3. Prepare Merge Plan

Create a merge plan document at `tmp/merge-plan-<date>.md` containing:
- List of incoming changes (incoming functionality)
- List of current branch changes (current functionality)
- Potential conflict files
- Resolution strategy for each conflict area

Present the plan to the user before proceeding.

**After the plan is created**, output this line so the document survives a long-running conversation:

```
**CRITICAL FOR CONTEXT & SUMMARY** The git merge document is: <path to the merge document>.md
```

Then proceed with the merge.

### 4. Merge Strategy

- **Prefer branch changes over the incoming branch** when both modify the same code -- the branch represents active work in progress and should be considered newer.
  - **Exception on a stack branch**: where the conflict is in code this PR's `## Dependencies` says a *parent* owns, prefer the parent's version. This PR does not own that surface, and "winning" the conflict here guarantees a worse one when the parent lands.
- **Do not overwrite branch code without user consent** -- if a conflict would discard branch work, stop and ask the user.
- For new files from master that don't conflict, accept them.
- For deleted files, verify the deletion is intentional before accepting.

### 5. Execute Merge

- Run `git merge origin/<incoming>` (the branch resolved in step 0).
- Resolve conflicts according to the plan and the strategy above.
- After resolving, stage the resolved files.

### 6. Verify

- Run `cargo build` to confirm the project compiles. Fix the code until it does.
- Run `cargo test` to confirm all tests pass -- tests guide the quality of the merge and reveal whether anything was lost. Check that no tests went missing and the pass/fail state has not regressed.
- Run `cargo clippy -- -D warnings` for lint checks.
- Verify that functionality from **both** branches was retained.
- If anything fails, diagnose and fix before completing the merge commit.
- Record the compile state and test state in the merge document as you go.

### 7. Commit and Record

- Commit the merge once build, tests, and lint pass. Never use `--no-verify`.
- Update the merge document with the final merge outcome.

## Output

- Merge result summary (clean or conflicts resolved)
- List of conflict files and how each was resolved
- Test results after merge
- Backup branch name for reference
- Path to the merge document
- Which branch was merged **from**, and which rule in step 0 resolved it

---

**Commands**: `/pr-stack-rebase`, `/repoint`, `/squash-pr`, `/merge-pr-stack`, `/pr`
**Skill**: `pr-stack` (`.agents/skills/pr-stack/SKILL.md`) — when this branch is part of a PR stack
