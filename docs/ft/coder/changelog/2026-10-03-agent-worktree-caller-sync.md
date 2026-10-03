# 2026-10-03 — A resumed subagent works on the caller's current files

PR [#576](https://github.com/uppin/tddy-coder/pull/576).

Every `subagent_prompt` and `subagent_resume` first merges the caller's current files — `HEAD` plus
staged, unstaged and untracked changes — into the conversation's worktree as a merge commit, merged
3-way against the last caller state the conversation took in. The subagent's own unpulled commits
survive beside the caller's changes, and changes the caller already pulled merge cleanly. When files
changed, the subagent is told in one message appended last before the turn (at most 20 paths, then a
count), and the turn outcome carries `worktreeSync { commit, files, lines, paths, morePaths }`.
Nothing happens when the conversation has no worktree yet or the caller has not changed since the
last sync. A conflict refuses the turn before any model call, names the paths, and moves nothing; a
resume that rewound stays rewound. `syncWorktree: false` runs a turn on the worktree as it stands.

A caller's merged changes are never counted as the subagent's work: `subagent_pull`, `subagent_end`,
a rewind's reset and `subagent_diff`'s bounds see only the subagent's commits (the branch's
first-parent line without merges); a diff whose range spans a sync says so with
`includesCallerChanges`, and an omitted `to` diffs to the tip even when it is a sync merge.

The jail host bridge serves `ConversationWorktree` only for the session its jail was built for
(`PermissionDenied` otherwise) and finds that session's worktree under its owner's sessions base. The
sync needs git ≥ 2.40 on the host that owns the session worktree.

See [managed-codebase-subagents.md](../managed-codebase-subagents.md) § Every turn takes in the
caller's current files and § A caller's merged changes are never the subagent's work.
