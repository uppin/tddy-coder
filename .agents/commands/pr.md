---
description: Create a pull request from the current branch's changes, with stack-aware base detection.
---

# Create Pull Request

Create a PR from the current branch's changes.

> **If this branch is part of a PR stack**, load the `pr-stack` skill
> (`.agents/skills/pr-stack/SKILL.md`) first — it owns the base-resolution rules, the title
> convention and the landing order that the steps below only summarise. Related commands:
> `/add-to-pr-stack`, `/pr-stack-rebase`, `/repoint`, `/merge-pr-stack`.

## Process

### 1. Branch Check

- **If on `main` or `master`**: ask the user for a branch name and create it from `master` before proceeding — `git checkout -b <feature-branch-name>` — then commit there and push with tracking (`git push -u origin <feature-branch-name>`).
  - **Why**: never push `master` to a differently-named remote branch; it is error-prone and confusing.
- **If on a different branch that is unrelated to the current change**: ask the user whether to reuse it or create a new one.
- **If the user has already created the relevant branch**: use it as-is.

### 2. Determine the PR base (stack-aware)

Do NOT assume the base is `origin/master`. Detect it:

1. **Default branch**: `git symbolic-ref --short refs/remotes/origin/HEAD` (fallback: `master`, then `main`). This is the base when the branch is standalone.
2. **Already has an open PR?** `gh pr list --state open --head <current-branch> --json number,baseRefName`. If a PR exists for the current branch, you are **updating** it — keep its existing `baseRefName` and skip to step 3 (do not re-detect).
3. **In a stack** — this branch is based on another open PR's branch. Two signals, either is enough:
   - per-PR documents are attached to this session: a `changeset.md` carrying `## Responsibility` / `## Boundaries` / `## Dependencies` / `## Draft PR contract`;
   - the branch is named `feature/<stack-slug>/<node>` and another branch under the same `feature/<stack-slug>/` namespace has an open PR.

   The base is then **the parent node's branch**, which the attached `changeset.md` names under `## Dependencies`. Do not guess it from ancestry — the stack is a DAG and a node may have several parents. See the `pr-stack` skill.
4. **Detecting it from the open PRs:**
   - `gh pr list --state open --json number,headRefName,baseRefName`
   - For each open PR's `headRefName` (excluding the current branch), test ancestry: `git merge-base --is-ancestor origin/<headRefName> HEAD`. Fetch first if the ref is stale (`git fetch origin <headRefName>`).
   - Branches that pass are **stack ancestors**. The **stack parent** is the closest one — the ancestor whose tip is itself an ancestor of HEAD with no other open-PR branch in between (smallest `git rev-list --count origin/<headRefName>..HEAD`).
5. **Choose the base**:
   - In a stack → base = the predecessor's branch (note its PR number — this PR stacks on it).
   - Ad-hoc stack parent found → base = the stack parent's `headRefName` (note its PR number).
   - Else → base = the default branch from step 1.
6. If multiple candidate stack parents, or the detected base is not `master`/`main`, **confirm with the user** before proceeding — basing a PR on the wrong branch misroutes the stack.

### 3. Analyze Changes

- Run `git status` and `git diff` to understand what has changed.
- Run `git log <base>..HEAD` to see all commits on this branch (use the detected base, not `master`).
- Identify which files are relevant to the feature and which are unrelated.

### 4. Compose Commit (if needed)

- If there are uncommitted changes, stage only the files relevant to the feature. Add relevant files that are not yet tracked by git.
- **CRITICAL**: only include files that are relevant to the PR. Do NOT include unrelated files, generated artifacts, or temporary files.
- If there are unrelated functional changes, ask the user whether to include them.
- Write a clear commit message summarizing the changes (markdown body).
- Re-check `git status` afterwards to confirm everything intended is staged and nothing else is.
- Do not skip linting or pre-commit errors that block the commit — fix them. **Never** use `--no-verify` when committing or pushing.

### 5. Verify Before Push

- Run `cargo test` to make sure tests pass.
- Run `cargo clippy -- -D warnings` to check for lint issues.
- If tests fail, stop and inform the user. Do not push broken code.

### 6. Push and Create PR

- Push the branch to the remote:
  - Normal push: `git push -u origin <branch-name>` (first push) or `git push` (update).
  - **If the local branch has diverged from its remote tracking branch** (e.g., local was rebased; `git status` shows "have diverged"): a normal push is rejected. Ask the user before force-pushing, and use `git push --force-with-lease` (safe — aborts if the remote moved since your last fetch). **Never** `--force` to `master`/`main`, and never force-push without explicit user consent.
- Create the PR using `gh pr create --base <detected-base> --head <branch-name>` with:
  - A concise title (under 70 characters). **On a stack branch**, close the title with the stack group — `<type>(<scope>): <what this PR delivers> (#<stack-slug> K/N)` — and write the subject as the capability *shipped*, never as a phase (`red`, `stubs`, `WIP`). The title becomes the squash-merge commit on `master` and cannot be fixed afterwards; see the `pr-stack` skill.
  - A body with a summary of changes and a test plan
  - If updating an existing PR, skip `gh pr create` (the push already updated it); just present the existing PR URL.
- **In a stack**: once the PR exists, **re-register the stack** so it joins — `gh stack link --base master <prs…>`, bottom to top, additive. A PR nobody registered is correctly based but invisible as part of the stack.
- Present the PR URL to the user. If the push output returned a URL to finish PR creation, you may open it in the browser (`open <URL>` on macOS).

## Output

- PR URL
- The detected base branch and whether this PR is part of a stack (with the parent PR number)
- Summary of what was included
- Any files that were intentionally excluded and why
