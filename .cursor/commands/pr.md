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
6. **CRITICAL** only include files that are relevant to the PR.

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

## Creation of the PR

1. Always check git status if everything is added to commit as expected.
2. Do not skip linting errors that are preventing commit. Fix them.
3. After the changes are pushed to remote branch, the git command returns an URL to finish the PR creation.
4. Open the user's browser window with the retrieved URL. In some systems `open <URL>` command is used.
