---
description:
globs:
alwaysApply: false
---
Users wants to open a PR from the changes in the current focus.

You should:
1. Analyze the current changes and compose the commit.
2. Commit should only include relevant work. If there are some irrelevant functionalty changes, you should ask the user if the changes should be included to the commit.
3. Add relevant files that are not added to git yet.
4. Make a commit with summary of the changes. Markdown format should be used.
5. Push the changes to a newly created remote branch.
   - Normal push: `git push -u origin <branch-name>` (first push) or `git push` (update).
   - **If the local branch has diverged from its remote tracking branch** (e.g., local was rebased; `git status` shows "have diverged"): a normal push is rejected. Ask the user before force-pushing, and use `git push --force-with-lease` (safe — aborts if the remote moved since your last fetch). **Never** `--force` to `master`/`main`, and never force-push without explicit user consent.
6. **CRITICAL** only include files that are relevant to the PR.
7. Create the PR with `gh pr create --base <detected-base> --head <branch-name>` (use the base detected above, not `master`). If a PR already exists for this branch, the push already updated it — skip `gh pr create` and present the existing PR URL.

## Working branch selection

1. **If the user is in main/master branch**:
   - **CRITICAL**: Create a new local branch from master first
   - Switch to the new branch: `git checkout -b feature-branch-name`
   - Commit changes to this new branch
   - Push to remote with tracking: `git push -u origin feature-branch-name`
   - **Why**: Never push master to a differently-named remote branch - it's error-prone and confusing

2. **If the user is in a different branch**, however not related to the current change:
   - Ask the user if to use this branch or create a new one

3. **If the user has manually created the relevant branch**:
   - Just use this branch as-is

## Determine the PR base (stack-aware)

Do NOT assume the base is `origin/master`. Detect it before creating the PR:

1. **Default branch**: `git symbolic-ref --short refs/remotes/origin/HEAD` (fallback: `master`, then `main`). This is the base when the branch is standalone.
2. **Already has an open PR?** `gh pr list --state open --head <current-branch> --json number,baseRefName`. If a PR exists for the current branch, you are **updating** it — keep its existing `baseRefName` and skip to the push step (do not re-detect).
3. **Detect a PR stack** (the current branch stacks on another open PR):
   - `gh pr list --state open --json number,headRefName,baseRefName`
   - For each open PR's `headRefName` (excluding the current branch), test ancestry: `git merge-base --is-ancestor origin/<headRefName> HEAD`. Fetch first if the ref is stale (`git fetch origin <headRefName>`).
   - Branches that pass are **stack ancestors**. The **stack parent** is the closest one — the ancestor whose tip is itself an ancestor of HEAD with no other open-PR branch in between (smallest `git rev-list --count origin/<headRefName>..HEAD`).
4. **Choose the base**:
   - If a stack parent was found → base = the stack parent's `headRefName` (note its PR number — this PR stacks on it).
   - Else → base = the default branch from step 1.
5. If multiple candidate stack parents, or the detected base is not `master`/`main`, **confirm with the user** before proceeding — basing a PR on the wrong branch misroutes the stack.

## Creation of the PR

1. Always check git status if everything is added to commit as expected.
2. Do not skip linting errors that are preventing commit. Fix them.
3. After the changes are pushed to remote branch, the git command returns an URL to finish the PR creation.
4. Open the user's browser window with the retrieved URL. In some systems `open <URL>` command is used.
