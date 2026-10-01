# The conversation worktree — git mechanics

## Where it lives

`<session worktree>/tmp/subagent-worktrees/<conversation id>` (`SUBAGENT_WORKTREES_DIR`), on the
branch `tddy/subagent/<session id>/<conversation id>`. Inside the session worktree because that is the
one directory every exec-tool route, host and jail alike, can already reach. The branch name carries
the session id because sessions of one project share the repository's common dir and the caller
chooses conversation ids.

`/tmp/subagent-worktrees/` is added to `<common dir>/info/exclude` the first time, so the worktrees
never show in the caller's `git status`.

## Creation — lazy, on the first mutating call

`run_in_conversation` calls `ensure` only for a `ToolEffect::Mutating` call. A conversation that only
reads never creates anything; its reads run in the session worktree until it does.

`ensure`:

1. adds the exclude entry if absent;
2. takes the **base**: the caller's `HEAD` when the caller is clean, else a commit on top of `HEAD`
   holding the caller's uncommitted state — staged, unstaged and untracked, not ignored. It is built
   through a scratch `GIT_INDEX_FILE` seeded from the caller's own index (`add -A`, `write-tree`,
   `commit-tree -p HEAD`), so the caller's index, branch and files are never touched;
3. `git worktree add -b <branch> <path> <base>`;
4. records the base under `refs/tddy/subagent-base/<session>/<conversation>`, which is what `existing`
   finds later and what keeps an inherited-changes commit no branch points at alive.

## One commit per mutating call

`commit_changes(subject)` stages everything (`add -A`), derives the facts from
`diff --cached --name-status` and `--numstat`, and commits only when something changed:

- `A`/`C` = created, `M`/`T` = updated, `D` = removed, `R` = one removed and one created;
- a binary file (`-` in numstat) counts as a file and adds no lines;
- no change means counts of zero and **no commit**, and `commit` is absent from the result.

The subject is the tool's name. Commits use a fixed committer identity
(`tddy-subagent <tddy-subagent@tddy.invalid>`), `--no-verify` and no signing: a subagent's snapshot is
not the developer's commit. `Shell` and `Await` are mutating by the classifier, because what a shell
command or a background job wrote is only knowable from the tree.

`WorktreeChange` is deliberately bounded — counts and a short hash, never a path list — because it
rides every mutating tool result in a turn's outcome.

## Reset

`reset_to(&ResetTarget)` takes the worktree and its branch back for a caller that rewound the
conversation's transcript (`src/reset.rs`). `ResetTarget` is `Base` or `Commit(abbreviation)`.

1. It takes the worktree's lock, the same one `commit_changes` and `pull_into_caller` hold.
2. It lists `base..branch` oldest first. A `Commit` that is not on that list — off the branch, or the
   base named by hash (the base is reached only through `Base`) — is **refused before anything
   moves**, with an error naming the commit and the branch.
3. It runs `reset --hard <target>` and `clean -fd`. `clean` omits `-x`, so **ignored files are kept**:
   a `SHELL cargo build` leaves build output the reset must not delete.
4. It answers `WorktreeReset { to, dropped_commits }` — the short hash the tree now stands at and the
   short hashes of the commits past it, oldest first, serialized `droppedCommits`. A list rather than a
   count, so a caller that already took some of them can tell which.

Resetting to the tip drops nothing. The base ref under `refs/tddy/subagent-base/` is untouched, so a
reset never loses the base.

## Diff

`diff(from, to)` reads what the conversation changed between two of its commits (`src/diff.rs`).
Nothing is written to either worktree and it takes no lock, so it is safe while a call runs.

1. `from` defaults to the base and `to` to the branch tip. Each must be the base or a commit on
   `base..branch`, resolved through the commit lookup `reset_to` uses (`resolve_commit`); a commit
   outside the conversation, or one a reset dropped from the branch, is refused with an error naming it.
2. `from` must be an ancestor of `to` (`merge-base --is-ancestor`); otherwise it is refused.
3. The text is `git diff --no-ext-diff --no-textconv from to`. It is deliberately **not** `--binary`:
   a binary change shows as git's `Binary files … differ` line and carries no payload.
4. The counts come from `--name-status` and `--numstat` over the same range, through the parser
   `commit_changes` uses, so they match a turn's `worktreeChange`.
5. Text past `DIFF_TEXT_CAP_BYTES` (64 KiB) is cut at the last whole line and `truncated` is set; the
   counts still describe the whole range.

It answers `ConversationDiff { from, to, files, lines, diff, truncated }` with `from` and `to` as
short hashes. `from` is exclusive and `to` inclusive, git's `from..to`.

## Hand-over

`pull_into_caller` applies `base..tip` to the caller's worktree with `git apply --3way`, as
**uncommitted** changes. The caller's own inherited changes are in the base, so they are not applied
twice. `apply --3way` implies `--index` and refuses a file whose working copy differs from the index
(the caller's unstaged edits), so it runs against a scratch index refreshed from the working tree for
the touched paths; the caller's real index is neither read for staging state nor written. Where the
caller changed the same lines since, the file is written with conflict markers and its path is
listed in `PullOutcome::conflicts`. The caller's `HEAD` never moves.

`remove` deletes the worktree (`git worktree remove --force`), the branch and the base ref.

## Git runs on the host

A linked worktree's `.git` points into the repository's common dir, which a jail mounting only the
checkout cannot see, so all of this runs on the daemon host. A sandboxed workspace session's jail
only *finds* the conversation root under its own mount
(`tddy_sandbox_runner::CONVERSATION_WORKTREES_DIR`, pinned equal to `SUBAGENT_WORKTREES_DIR` by a test
in `tddy-daemon-sandbox`).
