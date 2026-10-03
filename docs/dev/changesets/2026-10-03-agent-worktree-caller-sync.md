# 2026-10-03 — A resumed subagent works on the caller's current files

**Type:** Feature

PR [#576](https://github.com/uppin/tddy-coder/pull/576), base `master`. Feature doc:
[managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md) § Every turn takes in
the caller's current files, § A caller's merged changes are never the subagent's work. Mechanics:
[`tddy-subagent-worktree`](../../../packages/tddy-subagent-worktree/docs/conversation-worktree.md)
§ Caller sync; turn order:
[`tddy-discovery`](../../../packages/tddy-discovery/docs/roster-and-subagent-runtime.md) § Every turn
takes in the caller's current files.

## What was delivered

- **`tddy-subagent-worktree`** — `sync_with_caller` (`src/sync.rs`): the conversation worktree's
  uncommitted state as an unattached commit (`Changes made outside a tool call`), the caller
  snapshot, `git merge-tree --write-tree --merge-base <last caller state taken in> <ours> <snapshot>`,
  `commit-tree` with the subagent's tip as first parent, `reset --hard`; `SyncOutcome::{Unchanged,
  Merged(WorktreeSync), Conflicted { paths }}`, nothing moved on a conflict. `src/lineage.rs`:
  `subagent_commits` (`rev-list --first-parent --no-merges`), `caller_state_taken_in` (the newest
  merge's second parent, else the base), `merges_between`. `pull_range`, `reset_to` and `diff` read
  `subagent_commits`; `diff`'s omitted `to` is the tip with no membership check and
  `ConversationDiff::includes_caller_changes` marks a range spanning a sync; `pull_into_caller` is
  `pull_range` over the whole branch with an empty ledger. `git_raw` carries the exit code;
  `commit_changes_held` serves a caller already holding the lock.
- **`tddy-service`** — `SyncOp sync = 15` on `ConversationWorktreeRequest.op`; TypeScript regenerated
  in `tddy-web` and `tddy-rust-typescript-tests`.
- **`tddy-session-lifecycle`** — the `Sync` arm of `run_conversation_worktree_op` (one helper per op,
  `{"sync": {…} | null}`, `{"conflicts": [≤ 20], "moreConflicts": n}`, `info` logs). The host bridge
  is bound to its jail's session: `BoundJailSession { session_id, session_dir }` on a per-jail
  `DaemonRpcHandler` built by `sandbox_rpc_handler(session_id, session_dir)` at all three jail-launch
  sites; the host keeps a `Weak<DaemonSessionHost>`; `conversation_worktree_from_jail` refuses another
  session with `PermissionDenied` and resolves the worktree through
  `workspace_session::resolve_worktree_root_in_session_dir`, which the token route shares.
- **`tddy-session-tool-client`** — `sync_conversation_worktree`, `conversation_sync_request`, the
  `{"sync": {}}` Connect-JSON key; request builders take an `Op` (`conversation_op_request`) and the
  HTTP body is `connect_json_body`.
- **`tddy-discovery`** — `WorktreeSyncPort`, `SyncAnswer`, `sync_notice`, `sync_refusal`,
  `RewindApplied` (`subagent/worktree_sync.rs`); `subagent/turn_start.rs` runs ledger clear → reset →
  rewind → sync → appends (notice last); a merge clears the repeat ledger; `SubagentConfig::
  with_worktree_sync`, `syncing_worktree_through`, `TurnRequest::{without_sync, syncs_worktree}`,
  `PromptOutcome::worktree_sync`, `worktreeSync` in `prompt_outcome_json`. `take_turn` 77 → 25 lines.
- **`tddy-tools`** — `ConversationWorktreeSyncPort` (`src/worktree_sync_port.rs`) wired in
  `subagent_config_for_conversation`; `syncWorktree` on `subagent_prompt` and `subagent_resume`
  (`src/sync_worktree_choice.rs`); reset and sync answer parsing shared in `src/worktree_answer.rs`.
- **`tddy-daemon-rpc`** — tests only: `conversation_worktree_sync_acceptance`, and
  `conversation_worktree_host_bridge_acceptance::a_relayed_call_naming_another_session_is_refused`.

## Decisions & Trade-offs

- Subagent work = first-parent line without merges, over tagging sync commits by subject (a subagent
  commit could forge one).
- The last caller state is derived from the newest merge's second parent, over a ref that every reset
  would have to invalidate.
- The notice is appended last, after prompt / correction / replacement, so a replacement's tool-call
  group is complete before any user message.
- `subagent_diff` stays a tree diff and flags `includesCallerChanges`; a named merge bound is refused,
  an omitted `to` diffs to a sync-merge tip.
- A dirty conversation worktree is committed as subagent work before a sync rather than refusing — a
  finished background job would otherwise block every later turn. On a conflict it stays uncommitted.
- A sync refusal after a rewind leaves the rewind done, and says so.
- A merge that changes no file in the conversation worktree (the caller only pulled) is recorded and
  answered `{"sync": null}`, so a later subagent edit of a pulled line does not conflict spuriously.
- Requires git ≥ 2.40 (`merge-tree --write-tree --merge-base`).
- `pull_into_caller`'s `files`/`lines` are the sum of each commit's change, as a range pull reports,
  not a squashed base..tip diff; `Op::Pull` has no tool caller.

## Backlog entries resolved

- **A jail can name another session's conversation worktree over the host bridge** —
  `2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge`. Both halves
  closed: the session is bound host-side (`PermissionDenied` for another) and the worktree is resolved
  from the session directory the daemon resolved under the owner's sessions base. Test:
  `tddy-daemon-rpc/tests/conversation_worktree_host_bridge_acceptance.rs::a_relayed_call_naming_another_session_is_refused`.
  Deleted from `docs/dev/todo/`.

## Code issues closed

- **`tddy-subagent-worktree` — duplication `pull_commit` / `pull_into_caller`**
  (`duplication-range-pull-pull-commit`, first detected 2026-10-01 at ~14 repeated lines). Final
  measurement on `master` (`4f2a3b66`), read by hand: **0 repeated lines** — `pull_into_caller` is a
  10-line delegation to `pull_range`, and the name-status / numstat / `--binary` / name-only /
  `apply_3way` sequence exists only in `range_pull.rs::pull_commit` (the one `--binary` and the one
  `apply_3way` call site in the crate). Record deleted.

## Code issues re-measured (kept)

Production lines to the first column-0 `#[cfg(test)]`, `0ce696aa` → `4f2a3b66`:
`tddy-discovery/src/subagent.rs` 2,050 → 2,046; `subagent_runtime.rs` 732 → 735;
`tddy-tools/src/server.rs` 2,800 → 2,810; `tddy-session-tool-client/src/lib.rs` 1,110 → 1,111 — each
already carries its row. `tddy-session-lifecycle` `handle_rpc` 182 → 185 lines, nesting 8 unchanged
(the `ConversationWorktree` arm passes `&self.bound`); `start_sandboxed_cursor_cli_session` 414 lines,
unchanged (one argument added).

## Deferred, with the developer's consent of 2026-10-03

Open `docs/dev/todo/` entries created here: *A background job can race the caller sync*
(`2026-10-03-a-background-job-can-race-the-caller-sync`), *A merge the subagent makes itself is read
as a caller sync* (`2026-10-03-a-subagent-made-merge-is-read-as-a-caller-sync`), *Files the
caller-sync change grew past the file budget* (`2026-10-03-caller-sync-grew-over-budget-files`).
Touched and still open: hooks/signing consent (now covering the merge and the outside-a-tool-call
commits), `subagent_end` racing a prompt (an end mid-sync fails that turn's sync with a git error),
the unverified HTTP Connect-JSON arm (one more key, `{"sync": {}}`). No HTTP timeout on the per-turn
sync call: the client has no timeout pattern to reuse.

## Verification

Scoped, 2026-10-03: `tddy-subagent-worktree` (lib 35, `caller_sync_acceptance` 17,
`subagent_lineage_acceptance` 10, and the existing suites), `tddy-discovery` (lib 97,
`caller_sync_acceptance` 12, every other suite), `tddy-session-tool-client` lib 15, `tddy-tools`
(`subagent_sync_worktree_mcp` 3 and the rest), `tddy-session-lifecycle --lib` 150, `tddy-daemon-rpc`
conversation-worktree suites (sync 5, host bridge 3, …), `tddy-service`; clippy `-D warnings` and
`fmt --check` clean over those crates. Whole workspace: CI.
