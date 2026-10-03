# 2026-10-03 — A merge the subagent makes itself is read as a caller sync

**Category:** Known gap
**Source:** changeset [`2026-10-03-agent-worktree-caller-sync`](../changesets/2026-10-03-agent-worktree-caller-sync.md)
(`/validate-changes` first pass, ℹ info)

The conversation branch's lineage (`packages/tddy-subagent-worktree/src/lineage.rs`) treats **every**
merge commit on the first-parent line as a caller sync:

- `subagent_commits` (`rev-list --first-parent --no-merges`) leaves it out, so `subagent_pull`,
  `subagent_end`, a rewind's reset targets and `subagent_diff` bounds never see it as the subagent's
  work;
- `caller_state_taken_in` takes its **second parent** as "the last caller state the conversation took
  in", so the next sync merges 3-way against that commit;
- `subagent_diff` marks any range spanning it `includesCallerChanges`.

A subagent that runs `git merge` (or `git pull`) itself inside its worktree would make such a merge.
Its changes would then be dropped from every pull, and the next sync would merge against an unrelated
second parent — at best a spurious conflict, at worst a merge that silently reverts the caller's
changes the merge base no longer explains.

## Why it is narrow

A conversation worktree is a *linked* worktree: its `.git` points into the repository's common dir,
which a jailed subagent cannot see (the jail mounts only the session checkout). Git inside the jail
fails, so a jailed subagent cannot commit or merge at all. Only a host-run (unsandboxed) subagent with
a shell could, and its system prompt does not ask it to.

## Possible fix

Mark sync merges so the lineage recognises only its own:

- a ref per sync (`refs/tddy/subagent-sync/<session>/<conversation>/<n>`), or
- a trailer on the merge (`Tddy-Caller-Snapshot: <snapshot hash>`) **verified** against the merge's
  second parent — a trailer alone is forgeable by a subagent commit message, which is why the
  planning decision rejected subject-based tagging; tying it to the parent hash it names makes a
  forged one detectable.

Then `subagent_commits` lists every first-parent commit that is not a recognised sync (a
subagent-made merge included, applied by its first-parent diff), and `caller_state_taken_in` walks
recognised syncs only. A ref per sync needs pruning on reset, which the planning decision avoided;
the verified trailer does not.
