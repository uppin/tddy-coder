# Create Pull Request

Create a PR from the current branch's changes.

## Process

### 1. Branch Check

- If on `main` or `master`, ask the user for a branch name and create it before proceeding.
- If already on a feature branch, continue.

### 2. Analyze Changes

- Run `git status` and `git diff` to understand what has changed.
- Run `git log main..HEAD` (or `master..HEAD`) to see all commits on this branch.
- Identify which files are relevant to the feature and which are unrelated.

### 3. Compose Commit (if needed)

- If there are uncommitted changes, stage only the files relevant to the feature.
- Do NOT include unrelated files, generated artifacts, or temporary files.
- Write a clear commit message summarizing the changes.

### 4. Verify Before Push

- Run `cargo test` to make sure tests pass.
- Run `cargo clippy -- -D warnings` to check for lint issues.
- If tests fail, stop and inform the user. Do not push broken code.

### 5. Push and Create PR

- Push the branch to the remote: `git push -u origin <branch-name>`.
- Create the PR using `gh pr create` with:
  - A concise title (under 70 characters)
  - A body with a summary of changes and test plan
- Present the PR URL to the user.

## Output

- PR URL
- Summary of what was included
- Any files that were intentionally excluded and why
