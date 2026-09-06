---
description: Repoint a stacked branch onto origin/master (default) after its predecessor merged - change the PR base, restack the history to match, and re-register the stack
---
## Repoint Branch

After a stacked PR's **predecessor has merged** — or after its base branch became unusable for any
other reason — move this branch onto its new base: **`origin/master` by default**, or a branch the
user specifies. This does two things:

1. **Change the PR base** on GitHub to the new base.
2. **Restack** — rebase the branch onto the new base so its diff is clean (only this PR's own delta
   remains, now that the predecessor's changes are in the new base).

Contrast with `/merge`, which keeps a branch current with its **existing** base mid-flight, and with
`/pr-stack-rebase`, which replays this branch onto the **latest tip of its existing base**. `/repoint`
**changes** the base. See the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`).

> **Repointing is not reshaping.** It answers *"the base branch drifted — keep the same
> predecessor"*. Changing which PR something is based on is a plan change, and belongs to
> `/split-pr-to-stack` or `/add-to-pr-stack`, followed by a re-registration. Do not use one for the
> other's question.

### What repointing changes

- **The PR's base ref**, which is what GitHub diffs against and what the merge will use.
- **The branch's history**, rebased onto the new base so the diff stays honest.
- **The registered stack**, if the repoint takes this PR out of it — a PR repointed onto `master` is
  no longer above anything, so re-register what remains.
- **A target that is the trunk means "leave the stack"**: this PR becomes independent.
- A target that names **neither** the resolved default branch **nor** any parent's branch is
  **rejected** — a stale label cannot silently rewrite the plan. Nothing is persisted, and the row
  stays blocked with the reason shown inline.
- A node that owns **no branch** repoints plan-only: there is nothing to rebase and no PR to
  re-target, and the node simply becomes startable again.

## Resolve the new base

- Default: `origin/master`.
- If the user specified a different target (`/repoint <branch>`), use that instead.
- In a stack with more than one ancestor, the correct target is the **nearest non-merged ancestor's
  branch**, not automatically `master`. Only when every ancestor has merged does the base collapse to
  `origin/master`. Say which one you resolved and why.

## Preconditions — verify before restacking

- **The predecessor's changes are actually present in the new base** (i.e. the predecessor PR really
  merged):
  ```bash
  git fetch origin master <old-base>
  git merge-base --is-ancestor origin/<old-base> origin/master && echo LANDED || echo NOT-LANDED
  ```
  If not landed, warn the user — repointing onto `master` before the predecessor lands pulls the
  predecessor's still-unmerged work into this PR's diff. Ask before proceeding.
- The working tree must have no tracked-file changes. Commit first; never `--no-verify`, never stash
  silently.

## Workflow

1. **Make a backup branch** of the current branch before touching anything:
   `git branch backup/<branch>-$(date +%Y%m%d-%H%M%S)`.
2. **Fetch** the new base, ref-scoped: `git fetch origin <new-base>`. Not a bare `git fetch origin` —
   a blanket fetch moves every remote-tracking ref, and the `--force-with-lease` in step 8 then
   compares against a remote state you never inspected.
3. **Change the PR base** on GitHub:
   ```bash
   gh pr edit <pr-number-or-branch> --base <new-base>
   gh pr view <pr-number-or-branch> --json baseRefName --jq .baseRefName   # confirm
   ```
   Plain `gh pr edit --base` is correct in this repo.

   **Golden rule:** never delete a branch while an open PR still bases on it — that closes the
   dependent PR, and it cannot be reopened via the API. Retarget the dependent's base **first**, then
   delete.
4. **Restack the branch onto the new base:**
   ```bash
   git rebase --onto origin/<new-base> <old-base> <current-branch>
   ```
   The `--onto` form replays **only this PR's own commits**, so the predecessor's (now-merged) commits
   drop out of the diff. A plain `git rebase origin/<new-base>` here would replay the predecessor's
   old commits as this PR's work. If `<old-base>` is itself stale (it was force-pushed), recover the
   tip from `git reflog show origin/<old-base>` or use `git merge-base` as the guard — the same guard
   the product's own repoint bridge uses.

   If the history genuinely makes a rebase unsafe, merge `origin/<new-base>` instead and say so — but
   the goal is a diff containing only this PR's delta.
5. **Resolve conflicts**, preferring this branch's own surface; resolve a shared catalog or index file
   by **union**. **Never implement a parent-owned symbol while resolving**, and never delete
   parent-owned files to tidy the diff.
6. **Compile** — `cargo build`; fix until it builds.
7. **Test and lint** — `./test -p <package>` for the packages this branch touches, plus
   `cargo clippy -- -D warnings` and `cargo fmt`. `./test` also writes `.verify-result.txt`; read it
   for evidence rather than trusting an exit code you cannot see. Scope the verification to the
   touched packages and **say that you scoped it**.
8. **Push** the restacked branch: `git push --force-with-lease origin <branch>`. Never plain
   `--force`, never `--no-verify`. If the PR is not a draft and carries an approval, say so and ask
   first — a force-push can dismiss approvals under branch protection, and the new head re-runs the
   required checks from scratch, so an armed `#automerge` may need re-arming
   ([`docs/dev/guides/ci.md` § Automerge](../../docs/dev/guides/ci.md#automerge)).
9. **Update this PR's own documents.**
   If this PR's `## Dependencies` names a predecessor that has now merged, record that it landed in
   `master`, and update the changeset in `docs/dev/1-WIP/` where it states the base. Edit **only this
   PR's** documents — a predecessor's are not yours to edit.
10. **Re-check the topology.**
    ```bash
    gh pr list --state open --json number,headRefName,baseRefName,title
    git log --oneline origin/<new-base>..HEAD    # must be ONLY this PR's own commits
    ```
    That second command is the leak check: if it shows predecessor commits, the restack replayed the
    wrong range — go back to step 4 with the correct `<old-base>`. Then re-register the stack
    (`gh stack link --base master <prs…>`) so the registration matches the new bases.

## After Repoint

Output which base was set, that the PR base was updated on GitHub, whether the restack was done by
rebase or merge, and the result of the leak check — the PR diff must now contain only this PR's own
delta. Name any successor branch that is now stale against this one, and who runs `/pr-stack-rebase`
on it.

## Related

**Commands**: `/merge`, `/pr-stack-rebase`, `/add-to-pr-stack`, `/split-pr-to-stack`, `/merge-pr-stack`,
`/pr`, `/pr-wrap`, `/fix-pr`
**Skill**: `pr-stack` (`.agents/skills/pr-stack/SKILL.md`)
**Product docs**: the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`)
