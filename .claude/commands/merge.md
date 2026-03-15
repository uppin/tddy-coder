# Merge Workflow

Merge incoming changes (from main/master or another branch) into the current working branch safely.

## Process

### 1. Backup

- Create a backup branch from the current HEAD: `git branch backup/<current-branch>-<date>`.
- Confirm the backup was created.

### 2. Analyze Changes

- Identify the current branch and the incoming branch (default: `master`).
- Run `git log --oneline HEAD..origin/master` to see incoming commits.
- Run `git diff HEAD...origin/master` to understand what will change.
- Identify potential conflict areas by comparing changed files on both sides.

### 3. Prepare Merge Plan

Create a merge plan document at `tmp/merge-plan-<date>.md` containing:
- List of incoming changes
- List of current branch changes
- Potential conflict files
- Resolution strategy for each conflict area

Present the plan to the user before proceeding.

### 4. Merge Strategy

- **Prefer branch changes over master** when both modify the same code -- the branch represents active work in progress.
- **Do not overwrite branch code without user consent** -- if a conflict would discard branch work, stop and ask the user.
- For new files from master that don't conflict, accept them.
- For deleted files, verify the deletion is intentional before accepting.

### 5. Execute Merge

- Run `git merge origin/master` (or the specified branch).
- Resolve conflicts according to the plan and the strategy above.
- After resolving, stage the resolved files.

### 6. Verify

- Run `cargo build` to confirm the project compiles.
- Run `cargo test` to confirm all tests pass.
- Run `cargo clippy -- -D warnings` for lint checks.
- If anything fails, diagnose and fix before completing the merge commit.

## Output

- Merge result summary (clean or conflicts resolved)
- List of conflict files and how each was resolved
- Test results after merge
- Backup branch name for reference
