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

`commit_changes` takes the worktree's exclusive lock (`serialise::exclusive`, not re-entrant);
`commit_changes_held` is the same commit for a caller already holding it — the sync.

## Subagent work: the first-parent line without merges

A [caller sync](#caller-sync) puts merge commits on the branch whose second parent is a snapshot of
the caller. Walking `base..branch` would list the caller's own commits and snapshots as if the
subagent had made them, and a pull would hand the caller its own changes back. So the subagent's work
is defined in one place (`src/lineage.rs`):

- `subagent_commits()` = `git rev-list --first-parent --no-merges --reverse base..branch` — full
  hashes, oldest first. `pull_range`, `pull_into_caller`, `reset_to` (targets and `dropped_commits`)
  and `diff` (bounds) all read this listing; none walks `base..branch` itself.
- **The last caller state taken in** (`caller_state_taken_in`, crate-private) is the second parent of
  the newest merge on the first-parent line of `base..branch`, or the base when there is none. It is
  derived from the branch, not recorded, so a reset past a merge drops it with nothing to invalidate.
- `merges_between(from, to)` says whether a merge lies on the first-parent line of `from..to` — what
  `diff` reports as `includes_caller_changes`.

Every merge on the first-parent line is read as a sync. A merge the subagent made itself (a host-run
subagent running `git merge` in its worktree) would therefore be read as one too; a jailed subagent
cannot, since the jail cannot see the common dir. Recorded as a known gap in the backlog (*A merge
the subagent makes itself is read as a caller sync*).

## Caller sync

`sync_with_caller()` (`src/sync.rs`) brings the conversation worktree up to the caller's current
files before a turn, under the worktree's exclusive lock:

1. **`ours`.** The conversation worktree's uncommitted state as a commit on top of the tip, subject
   `OUTSIDE_A_TOOL_CALL_SUBJECT` (`Changes made outside a tool call`) — built through
   `inherit::uncommitted_state_as_commit`, the scratch-index snapshot `ensure` uses for the base, so no
   branch points at it yet. The tip itself when the worktree is clean. This is subagent work (a
   background job that finished between turns).
2. **The caller snapshot** — `inherit::base_commit(caller)`, exactly as the base is taken: `HEAD`, or
   a commit on top of it holding staged, unstaged and untracked (not ignored) changes. The caller's
   index, branch and files are only read.
3. **`taken`** — the last caller state taken in (above).
4. **Unchanged.** When the snapshot's tree equals `taken`'s, the branch moves onto `ours` with a
   plain `reset -q` (the files stay as they are) and the answer is `SyncOutcome::Unchanged`. Trees,
   not hashes: a snapshot of uncommitted state is a fresh commit every time.
5. **The merge.** `git merge-tree --write-tree --name-only -z --merge-base <taken> <ours> <snapshot>`
   — 3-way against the last caller state taken in, not git's own merge base (a conversation that
   started from the caller's uncommitted changes starts from a commit off the caller's history, and
   each later snapshot is a fresh one). Changes the caller pulled from the subagent are identical on
   both sides and merge cleanly. `parse_merge_tree` reads the answer: exit 0 (`MERGE_TREE_CLEAN`) is
   the tree; exit 1 (`MERGE_TREE_CONFLICTED`) is `SyncOutcome::Conflicted { paths }`, sorted and
   deduplicated, with **nothing moved** — the branch stays where it was and a dirty worktree stays
   dirty. A missing or non-hex tree, a conflict that names no path, or any other exit is
   `WorktreeError::Git` carrying git's stderr. `git_raw` carries the exit code (`RawOutput::code`) for
   this. `--merge-base` with `--write-tree` needs **git ≥ 2.40**; an older git fails every sync.
6. **Record.** `commit-tree <tree> -p <ours> -p <snapshot> -m SYNC_MERGE_SUBJECT` (`Merge the
   caller's changes`), then `reset -q --hard <merge>`. The first parent is `ours`, so the subagent's
   tip — and its uncommitted work, now a commit — stays on the first-parent line. No `clean` is
   needed: the tree is the merge's.
7. **Facts.** `diff --no-renames --name-status / --numstat / --name-only -z ours merge` (through
   `diff_output`, `--no-ext-diff --no-textconv`) → `WorktreeSync { commit, files, lines, paths,
   more_paths }`: the short hash, the counts through the parser `commit_changes` uses, the changed
   paths sorted, at most `SYNC_NOTICE_PATHS` (20), and how many it leaves out. Without rename
   detection every path is one entry and a rename counts as one removed and one created.

A merge whose tree equals `ours`'s — the caller changed only by what the conversation already had,
i.e. it pulled the subagent's work and changed nothing else — is **recorded** and answered
`SyncOutcome::Unchanged`: the next sync merges against it, so a later subagent edit of a line the
caller pulled does not conflict against the older state, and there is nothing to tell the subagent.

A background job does not take the worktree lock, so a write it makes between step 1 and step 6 can
be lost or committed as a later turn's work — a known gap in the backlog (*A background job can race
the caller sync*).

## Reset

`reset_to(&ResetTarget)` takes the worktree and its branch back for a caller that rewound the
conversation's transcript (`src/reset.rs`). `ResetTarget` is `Base` or `Commit(abbreviation)`.

1. It takes the worktree's lock, the same one `commit_changes` and the sync hold.
2. It lists `subagent_commits()`. A `Commit` that is not on that list — off the branch, a sync
   merge, or the base named by hash (the base is reached only through `Base`) — is **refused before
   anything moves**: "… is not one of the subagent's commits on branch …".
3. It runs `reset --hard <target>` and `clean -fd`. `clean` omits `-x`, so **ignored files are kept**:
   a `SHELL cargo build` leaves build output the reset must not delete.
4. It answers `WorktreeReset { to, dropped_commits }` — the short hash the tree now stands at and the
   short hashes of the subagent commits past it, oldest first, serialized `droppedCommits`. A list
   rather than a count, so a caller that already took some of them can tell which. A sync merge past
   the target is dropped from the branch with them but never listed; the next sync then finds an
   older caller state taken in and merges the caller's files in again.

Resetting to the tip drops nothing. The base ref under `refs/tddy/subagent-base/` is untouched, so a
reset never loses the base.

## Diff

`diff(from, to)` reads what the conversation changed between two of its commits (`src/diff.rs`).
Nothing is written to either worktree and it takes no lock, so it is safe while a call runs.

1. `from` defaults to the base and `to` to the branch tip — the tip with no membership check, so a
   tip that is a sync merge is diffed to. A bound that is **named** must be the base or one of
   `subagent_commits()`, resolved through the commit lookup `reset_to` uses (`resolve_commit`); a
   commit outside the conversation, one a reset dropped from the branch, or a sync merge is refused:
   "… is not the base or one of the subagent's commits on branch …".
2. `from` must be an ancestor of `to` (`merge-base --is-ancestor`); otherwise it is refused.
3. The text is `git diff --no-ext-diff --no-textconv from to`. It is deliberately **not** `--binary`:
   a binary change shows as git's `Binary files … differ` line and carries no payload.
4. The counts come from `--name-status` and `--numstat` over the same range, through the parser
   `commit_changes` uses, so they match a turn's `worktreeChange`.
5. Text past `DIFF_TEXT_CAP_BYTES` (64 KiB) is cut at the last whole line and `truncated` is set; the
   counts still describe the whole range.

It answers `ConversationDiff { from, to, files, lines, diff, truncated, includes_caller_changes }`
with `from` and `to` as short hashes. `from` is exclusive and `to` inclusive, git's `from..to`. The
diff stays a tree diff: a range that spans a sync includes the caller's merged changes, and
`includes_caller_changes` (serialized `includesCallerChanges`) is set when a merge lies on the
first-parent line of `from..to`.

## Hand-over

`pull_into_caller` applies every subagent commit since the base to the caller's worktree — it is
`pull_range(&PullRange::default(), &BTreeSet::new())`, reshaped into `PullOutcome` — so it shares the
one per-commit apply below and never applies a sync merge: the caller never gets its own changes
back. Its `files` and `lines` are the sum of what each commit changed, as a range pull reports them.
The caller's own inherited changes are in the base, so they are not applied twice.

Each commit is applied with `git apply --3way`, as **uncommitted** changes. `apply --3way` implies `--index` and refuses a file whose working copy differs from the index
(the caller's unstaged edits), so it runs against a scratch index refreshed from the working tree for
the touched paths; the caller's real index is neither read for staging state nor written. Where the
caller changed the same lines since, the file is written with conflict markers and its path is
listed in `PullOutcome::conflicts`. The caller's `HEAD` never moves.

`remove` deletes the worktree (`git worktree remove --force`), the branch and the base ref.

## Range pull

`pull_range(&PullRange { from, to }, already_pulled)` hands a chosen part of the branch to the caller
(`src/range_pull.rs`), commit by commit, and is how a pull happens mid-conversation. The branch and the
conversation's worktree are not touched.

1. The commits are `subagent_commits()`, oldest first. `from` defaults to the first commit not in
   `already_pulled` and `to` to the tip; both are inclusive and must each be a commit on that list (the
   base and a sync merge are not), and `from` must not come after `to`. Otherwise the pull is refused
   naming the commit ("… is not one of the subagent's commits on branch …").
2. `already_pulled` holds short hashes; a branch commit is pulled when one of them is its prefix. The
   caller supplies it, so the worktree keeps no per-conversation state.
3. Each commit in the range not already pulled is applied by `pull_commit` as `git diff --binary c^ c`
   through the scratch-index `apply --3way` (`apply_3way`), so conflicts are attributable to one
   commit. Commits already pulled are reported in `skipped`. `pull_commit` is the one place that
   name-status / numstat / binary patch / touched paths / `apply_3way` sequence lives.
4. The answer is `RangePullOutcome { commits, skipped, files, lines, conflicts }`: short hashes oldest
   first, counts summed over the applied commits, conflicted paths deduplicated. A range with nothing
   left to pull is an empty outcome, not an error.

A git failure part-way leaves the commits applied so far in the caller's worktree.

## On the wire

The daemon serves these operations as `ConversationWorktree` (`exec_tools.proto`,
`ConversationWorktreeRequest.op`: `pull` 10, `remove` 11, `reset` 12, `diff` 13, `pull_range` 14,
`sync` 15) through `run_conversation_worktree_op` in `tddy-session-lifecycle`, one helper per op. The
sync's answers:

| Outcome | `result_json` |
|---|---|
| `Merged(sync)` | `{"sync": {commit, files, lines, paths, morePaths}}` |
| `Unchanged`, or no worktree yet | `{"sync": null}` — also a merge that was recorded but changed no file |
| `Conflicted { paths }` | `{"conflicts": [≤ 20 paths], "moreConflicts": n}` — nothing moved |
| `WorktreeError` | `Internal`, git's stderr in the message |

A merge and a conflict are each logged at `info` (commit and counts; the path count).

## Git runs on the host

A linked worktree's `.git` points into the repository's common dir, which a jail mounting only the
checkout cannot see, so all of this runs on the daemon host. A sandboxed workspace session's jail
only *finds* the conversation root under its own mount
(`tddy_sandbox_runner::CONVERSATION_WORKTREES_DIR`, pinned equal to `SUBAGENT_WORKTREES_DIR` by a test
in `tddy-daemon-sandbox`).
