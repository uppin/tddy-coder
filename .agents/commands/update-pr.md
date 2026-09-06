---
description: Update an existing PR with the current changes - rebase first when the branch is stacked
---
User wants to update an existing PR with the current changes.

## If this branch is stacked, rebase FIRST

Before you look at the diff, work out whether this branch is part of a PR stack — its open PR has a
`baseRefName` that is not `master`/`main`, or the session's `changeset.yaml` carries an
its stack position:

```bash
gh pr list --state open --head "$(git branch --show-current)" --json number,baseRefName,isDraft
```

If it is stacked, **run `/pr-stack-rebase` (single mode) before composing anything**. The reason is
not tidiness: with a stale merge-base, ancestor commits sit in `HEAD` and read as this PR's work, so
"analyze the current changes" analyses the predecessor's diff too — and staging or reverting from that
reading is how parent-owned code gets destroyed. `/pr-stack-rebase` is a verify-and-return when the
branch is already current, so running it costs nothing.

You should:

1. Run code quality checks and fix any issues, scoped to what you touched:
   - `cargo fmt`
   - `cargo clippy -- -D warnings`
   - `./test -p <package>` for the packages this branch changes (`./test` also writes
     `.verify-result.txt` — read it for evidence rather than trusting an exit code you cannot see).
     Say which packages you scoped the run to.
2. Analyze the current changes and compose the commit.
3. The commit should only include relevant work. If there are irrelevant functional changes, ask the
   user whether they should be included.
4. Add relevant files that are not added to git yet.
5. Make a commit with a summary of the changes. Use markdown format.
6. Push the changes to the current remote branch.
7. **CRITICAL** only include files that are relevant to the PR.

## Working branch

- Commit and push to the **current branch** (no new branch creation).
- Ensure the branch is tracking the remote: `git push` or `git push -u origin <branch-name>` if needed.
- **If the branch was rebased**, a plain push is rejected. Use `git push --force-with-lease` — never
  plain `--force`, and never force-push a reviewed non-draft PR without asking first.

## Before commit

1. Always run `git status` to verify everything is staged as expected.
2. Do not skip lint, format or compile errors that prevent the commit — fix them. **Never** use
   `--no-verify` when committing or pushing.
3. After pushing, the git command may return a URL; you can open it with `open <URL>` if the user
   wants to view the PR.

## After pushing a stacked PR

Say which successor branches are now stale against this one and that each is cleared with
`/pr-stack-rebase` from the worktree that owns it. Do not rebase them from here.

## Related

**Commands**: `/pr-stack-rebase`, `/pr`, `/draft-pr`, `/fix-pr`, `/repoint`, `/pr-wrap`
