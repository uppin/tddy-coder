# Create Pull Request

Create a PR from the current branch's changes.

## Process

### 1. Branch Check

- If on `main` or `master`, ask the user for a branch name and create it from `master` before proceeding.
- If already on a feature branch, continue.

### 2. Determine the PR base (stack-aware)

Do NOT assume the base is `origin/master`. Detect it:

1. **Default branch**: `git symbolic-ref --short refs/remotes/origin/HEAD` (fallback: `master`, then `main`). This is the base when the branch is standalone.
2. **Already has an open PR?** `gh pr list --state open --head <current-branch> --json number,baseRefName`. If a PR exists for the current branch, you are **updating** it — keep its existing `baseRefName` and skip to step 3 (do not re-detect).
3. **Detect a PR stack** (the current branch stacks on another open PR):
   - `gh pr list --state open --json number,headRefName,baseRefName`
   - For each open PR's `headRefName` (excluding the current branch), test ancestry: `git merge-base --is-ancestor origin/<headRefName> HEAD`. Fetch first if the ref is stale (`git fetch origin <headRefName>`).
   - Branches that pass are **stack ancestors**. The **stack parent** is the closest one — the ancestor whose tip is itself an ancestor of HEAD with no other open-PR branch in between (smallest `git rev-list --count origin/<headRefName>..HEAD`).
4. **Choose the base**:
   - If a stack parent was found → base = the stack parent's `headRefName` (note its PR number — this PR stacks on it).
   - Else → base = the default branch from step 1.
5. If multiple candidate stack parents, or the detected base is not `master`/`main`, **confirm with the user** before proceeding — basing a PR on the wrong branch misroutes the stack.

### 3. Analyze Changes

- Run `git status` and `git diff` to understand what has changed.
- Run `git log <base>..HEAD` to see all commits on this branch (use the detected base, not `master`).
- Identify which files are relevant to the feature and which are unrelated.

### 4. Compose Commit (if needed)

- If there are uncommitted changes, stage only the files relevant to the feature.
- Do NOT include unrelated files, generated artifacts, or temporary files.
- If there are unrelated functional changes, ask the user whether to include them.
- Write a clear commit message summarizing the changes (markdown body).

### 5. Verify Before Push

- Run `cargo test` to make sure tests pass.
- Run `cargo clippy -- -D warnings` to check for lint issues.
- If tests fail, stop and inform the user. Do not push broken code.

### 6. Push and Create PR

- Push the branch to the remote:
  - Normal push: `git push -u origin <branch-name>` (first push) or `git push` (update).
  - **If the local branch has diverged from its remote tracking branch** (e.g., local was rebased; `git status` shows "have diverged"): a normal push is rejected. Ask the user before force-pushing, and use `git push --force-with-lease` (safe — aborts if the remote moved since your last fetch). **Never** `--force` to `master`/`main`, and never force-push without explicit user consent.
- Create the PR using `gh pr create --base <detected-base> --head <branch-name>` with:
  - A concise title (under 70 characters)
  - A body with a summary of changes and a test plan
  - If updating an existing PR, skip `gh pr create` (the push already updated it); just present the existing PR URL.
- Present the PR URL to the user.

## Output

- PR URL
- The detected base branch and whether this PR is part of a stack (with the parent PR number)
- Summary of what was included
- Any files that were intentionally excluded and why
