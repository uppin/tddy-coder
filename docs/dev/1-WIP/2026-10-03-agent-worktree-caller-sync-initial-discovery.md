# Initial Discovery: A resumed subagent works on the caller's current files

**Changeset**: [2026-10-03-agent-worktree-caller-sync.md](./2026-10-03-agent-worktree-caller-sync.md)
**Date**: 2026-10-03
**Base**: `origin/master` at `0ce696aa` (after `#agent-worktree` #560–#563 merged)
**Passes**: 1

## Combined Conclusions

1. **The gap.** A conversation worktree is cut once, from the caller's `HEAD` + dirty state
   (`inherit::base_commit`), and from then on moves only through the subagent's own tool calls. A
   caller that edits its worktree between turns leaves the subagent working on a stale tree; its next
   commits are diffs against content the caller no longer has.
2. **Snapshot is reusable.** `inherit::base_commit(caller)` snapshots `HEAD` + staged + unstaged +
   untracked-not-ignored through a scratch index and returns `HEAD` itself when clean, else a fresh
   `commit-tree -p HEAD` — non-deterministic hash, so "unchanged" must compare **trees**.
3. **Every listing walks `base..branch` unfiltered** — `range_pull::commits_after_base`
   (`rev-list --reverse`), `reset::reset_to` (`rev-list --topo-order --reverse`), `diff::diff`
   (`rev-list`), `worktree::pull_into_caller` (tree diff `base..tip`). A merge commit whose second parent
   is a caller snapshot would make caller commits and the merge itself look like subagent work: a pull
   would apply the caller's own changes back to it, a reset would report caller commits as dropped.
   ⇒ the subagent's work is the branch's **first-parent line without merges**.
4. **`git merge-tree --write-tree --merge-base`** is available (git 2.43 on PATH, ≥ 2.40 needed) and
   already used in `tddy-session-worktree/src/base_sync.rs:221-262` (`--name-only -z`; exit 0 clean,
   1 conflict, ≥2 error). An explicit merge base is required: the inherited base commit is off the
   caller's history, so git's automatic base would be the caller's `HEAD` ancestry and produce spurious
   conflicts.
5. **"Last synced" can be derived, not stored.** The second parent of the newest merge on the
   first-parent line is the last caller snapshot merged; with no merge it is the base. This survives a
   rewind reset for free (a reset past a merge drops it), where a separate ref would need invalidating.
6. **Turn order in `take_turn`** (`subagent.rs:1907-1969`): validate → `forget_earlier_calls` (only on
   prompt/correction/rewind/replacement) → reset (before `rewind_to`) → `rewind_to` → `appended_from` →
   push prompt / correction / replacement → loop. A sync belongs after the reset; it also must clear the
   repeated-call ledger (the same read can return new content).
7. **Wiring mirrors the reset port** end to end: discovery trait + `SubagentConfig` slot + session
   builder + `SubagentRegistry::create`; `tddy-tools` port struct in `subagent_config_for_conversation`,
   a `syncWorktree` parser on **both** prompt and resume (prompt parses no worktree option today);
   client fn + request builder + a hand-written Connect-JSON arm in `ask_daemon_over_http`; proto
   `oneof op` tag **15**; one arm in `conversation_worktree_op::run_conversation_worktree_op`. The runner
   (`bind_to_this_session`), bridge arm and ports need no change.
8. **No worktree ⇒ no sync.** Before the first write, reads already run in the live session worktree.
9. **Locking**: `serialise::exclusive(root)` (in-process) is held by `ensure`, `commit_changes`,
   `reset_to`, `remove`; the sync must hold it too.
10. **`pull_into_caller`** (`Op::Pull`) has no tool caller left (`subagent_end` uses `pull_range`); it is
    duplicated by `range_pull::pull_commit` (code issue
    `packages/tddy-subagent-worktree/docs/code-issues/duplication-range-pull-pull-commit.md`).

## Exploration 1 — Explore agent over the merged stack

### 1. Sequence (chronological)

1. `git log`, `git --version`, `ls` of the crate, `wc -l`, read `Cargo.toml`. To fix the HEAD, the git version and the crate layout.
2. Read `lib.rs` and `worktree.rs`. For base and branch naming, `existing`, `ensure`, `pull_into_caller` and `apply_3way`.
3. Read `inherit.rs`, `range_pull.rs`, `reset.rs`, `diff.rs`. For the snapshot, the commit listings and the validations.
4. Read `run.rs`, `git.rs`, `serialise.rs`, `change_facts.rs`, `tool_effect.rs`, `conversation_id.rs`. For the per-call rule, the git runner and the locking.
5. Read `tests/support/mod.rs` plus the crate README, `docs/conversation-worktree.md`, the code-issue and the changesets.
6. `ls` of discovery `src/subagent/`, then grep in `subagent.rs` for `reset|take_turn|push(|correction|replacement|worktree`.
7. Read `worktree_reset.rs`, `worktree_change.rs`, `turn_request.rs`, then `subagent.rs` lines 90-189, 760-829, 1380-1612 and 1880-2050.
8. Grep `transcript.rs` for its functions, then read lines 100-399.
9. Grep `subagent_runtime.rs` for `worktree`, then read 380-470 (`prompt_outcome_json`) and 580-670 (`DeferredTurn` / `run_turn`).
10. tddy-tools: `ls src`, grep for worktree items, then read `worktree_reset_port.rs`, `pull_ledger.rs`, `reset_worktree_choice.rs`, `subagent_end.rs`, `subagent_pull.rs`, `subagent_diff.rs`, `mcp_primitives.rs`.
11. Grep `server.rs`, then read 1700-2139 (new_session, prompt, resume, `take_a_turn`, await, cancel) and 2380-2610 (schemas).
12. Read `tddy-session-tool-client/src/conversation.rs` (all of it). Grep the proto and the client `lib.rs`.
13. Read `exec_tools.proto` (all of it).
14. Grep across packages for `conversation_worktree|ConversationWorktree`.
15. Read `conversation_worktree_op.rs`, `ports.rs` 1-40 and 340-413, the `daemon_rpc_handler.rs` arm, `local_exec_tools.rs` 90-240, `conversation_tool.rs`, `sandbox_session.rs` 180-310, `conversation_root.rs`, and the relay test in `runner.rs`.
16. `git show --stat` of #560-#563, `git diff --diff-filter=A fc49ff29 b16ddf9a` to list added tests and docs, then read the todo files.
17. Grep headings in `managed-codebase-subagents.md`, then read 336-520 and 735-814.
18. Read the test helpers: subagent-worktree suites, the daemon-rpc fixture, the discovery rewind fixture, the tddy-tools MCP fixture.
19. Checked: `which -a git`, both git versions, the nixpkgs `pkgs.git` pin, `merge-tree -h`, existing `merge-tree` users, and `base_sync.rs`.
20. Grepped for: gen paths, users of `ConversationWorktreeOp::Pull`, `PromptOutcome {` / `SubagentConfig {` literals, `with_worktree_reset` users, the recipes allowlist, and the runtime doc.

### 2. Inspected files and key excerpts

### packages/tddy-subagent-worktree

**`worktree.rs` — naming and storage**
- `SUBAGENT_WORKTREES_DIR = "tmp/subagent-worktrees"` (`:18`).
- Base ref (`:84-89`): `refs/tddy/subagent-base/{session_id}/{conversation}`.
- Branch (`:92-94`): `tddy/subagent/{session}/{conversation}`.
- `ConversationWorktree` (`:32-40`) holds `root`, `caller` (the session worktree), `branch`, `base` (full sha) and `base_ref`.

**`existing()`** (`:97-103`)
- Calls `recorded_base`, which runs `rev-parse --verify --quiet <base_ref>^{commit}` from the session worktree (`:106-125`).
- Then `live_worktree` returns `Some` only if `root.exists()` (`:128-141`).
- Never creates anything. Without a base ref, or without the directory, it answers `None`.

**`ensure()`** (`:152-201`)
- Takes `serialise::exclusive(&root)`.
- Returns the live worktree if there is one.
- Otherwise: `worktree prune`. If both the ref and the branch survived, `reattach`. Otherwise `clear_leftovers`, then `exclude_conversation_worktrees`, then `base = inherit::base_commit(caller)`, then `worktree add -q -b <branch> <root> <base>`, then `update-ref <base_ref> <base>`.

**`commit_changes(subject)`** (`:304-329`)
- Takes the lock, then `add -A`, then `diff --cached --name-status`.
- Empty means `WorktreeChange::default()` and no commit.
- Otherwise numstat, then `commit -q --no-verify -m subject`, then `rev-parse --short HEAD`.

**`pull_into_caller()`** (`:334-358`)
- Takes **no lock**.
- `range = format!("{}..{}", self.base, self.branch)`, then `git diff --name-status/--numstat/--binary/--name-only --no-renames -z <range>` run in the caller, then `apply_3way`.
- This is a tree-to-tree diff of base against tip, so any content merged in from the caller is included.

**`apply_3way`** (`:392-417`, `:419-442`)
- Copies the index to a scratch index, runs `update-index --add --remove -z --stdin` for the touched paths, then `apply --3way` under `GIT_INDEX_FILE=scratch`.
- Conflicts are parsed from `Applied patch to '…' with conflicts.` (`:470-479`).

**`remove()`** (`:361-384`): `worktree remove --force`, `branch -D`, `update-ref -d base_ref`.

**`inherit.rs`**
- `base_commit(caller)` (`:55-69`): `rev-parse HEAD`, the git-path index, a scratch index, then `snapshot_tree`.
- `snapshot_tree` (`:71-109`): `add -A -- .` with the scratch index, then `write-tree`. If the tree equals `HEAD^{tree}` it returns **HEAD itself**. Otherwise `commit-tree <tree> -p HEAD -m "Uncommitted changes inherited from the caller"`.
- `snapshot_tree` is private and returns only a commit, never the tree. Because of the fixed identity and the timestamp, the commit hash differs from call to call, so "has the snapshot changed" has to compare **trees**.
- `scratch_path` (`:113-122`) is `index.tddy-subagent-<pid>-<n>`.
- `common_dir` (`:12-19`) and `exclude_conversation_worktrees` (`:22-49`).

**`range_pull.rs`**
- `commits_after_base` (`:90-94`):
  ```rust
  let range = format!("{}..{}", self.base(), self.branch());
  let listed = git(self.root(), ["rev-list", "--reverse", &range], &[], None).await?;
  ```
  No `--first-parent` and no `--no-merges`.
- `position_on` (`:97-109`) uses `resolve_commit` and must find the commit in that list.
- `pull_commit` (`:112-147`) applies `git diff <full>^ <full>`. On a merge commit `^` is the first parent, so the patch would be the caller's own changes.
- Ledger matching is by prefix: `full.starts_with(pulled)` (`:46-50`).
- `PullRange { from, to }` and `RangePullOutcome { commits, skipped, files, lines, conflicts }`.

**`reset.rs`**
- `reset_to` (`:34-54`): takes the lock, then `rev-list --topo-order --reverse base..branch` (again unfiltered), then `resolve_reset`, then `reset -q --hard <target>`, then `clean -q -fd` (no `-x`, so ignored files are kept).
- `resolve_reset` (`:58-79`): `Base` returns the base and drops everything. `Commit(c)` must sit in `on_branch`, otherwise `Git{… "is not a commit of branch … after its base"}`. Dropped = everything after it.
- `resolve_commit` (`:82-95`) is `rev-parse --verify <abbr>^{commit}`.
- `short_hashes` (`:98-112`) is `rev-list --no-walk=unsorted --abbrev-commit`.
- `WorktreeReset { to, dropped_commits }` is camelCase.

**`diff.rs`**
- `diff(from, to)` (`:31-65`): `rev-list base..branch` (unfiltered). `resolve_bound` accepts the base or any listed commit (`:105-123`). Then `merge-base --is-ancestor from to`. Then `diff --no-ext-diff --no-textconv from to`, cut at 64 KiB (`:11`, `:128-139`).
- No lock.

**`run.rs`** — `run_in_conversation` (`:53-91`)
- ReadOnly: runs in `existing()`'s root, otherwise the session worktree.
- Mutating: `ensure`, execute, then `commit_changes(tool_name)`. A commit failure goes to `commit_error`, which `merge` writes as `"worktreeChange": {"error": …}`.
- `WORKTREE_CHANGE_KEY = "worktreeChange"`.

**`git.rs`** (`:20-28`, `:59-75`)
- Every call gets `-c core.hooksPath=/dev/null -c commit.gpgsign=false`, `GIT_OPTIONAL_LOCKS=0`, and author/committer `tddy-subagent <tddy-subagent@tddy.invalid>`.
- `git_raw` returns `(args, RawOutput{success, stdout: Vec<u8>, stderr})`. This is what a `merge-tree` call needs, since merge-tree exits 1 on conflict.

**`serialise.rs`** (`:14-29`)
- A process-wide `Mutex<HashMap<PathBuf, Arc<tokio::Mutex<()>>>>` keyed by the worktree root. `exclusive(root)` returns an `OwnedMutexGuard`, held until dropped. Entries are never removed.
- Held by `ensure`, `commit_changes`, `reset_to` and `remove`.
- Not held by `pull_into_caller`, `pull_range` or `diff`.
- It is in-process only: it serialises within the daemon process.

**`change_facts.rs`**
- `count_change` maps A/C to created, M/T to updated, D to removed, R to one removed plus one created; binary `-` adds no lines.
- `WorktreeChange { commit: Option, files, lines }`.

**`tool_effect.rs`**: Read, Glob, Grep, SemanticSearch and ReadLints are ReadOnly; everything else (including `Await`) is Mutating.

**`conversation_id.rs`**: `[A-Za-z0-9._-]{1,64}`, no leading `.`, no `..`, no trailing `.` or `.lock`.

**`tests/support/mod.rs` — builder API**
- `a_caller_worktree()` returns a `CallerWorktreeBuilder` with:
  - `.with_committed_file(p, c)`, `.with_committed_binary_file`
  - `.with_staged_edit`, `.with_unstaged_edit`, `.with_untracked_file`, `.with_ignored_file`
  - `.in_a_linked_worktree()`, `.build()`
- `CallerWorktree` methods: `path()`, `conversations()` (`ConversationWorktrees::new(path, "sess-1")`), `head()`, `current_branch()`, `status()`, `index_tree()`, `branches()`, `read`, `write`, `exists`, `delete_directory`, `install_post_commit_hook_leaving_hook_ran`, `lose_base_ref_of`.
- Free functions: `sever_git_link`, `conversation(id)`, `write(root, p, bytes)`, `remove`, `short_head`, `rev_parse`, `file_at`, `subjects_after(root, base, branch)`, `git(dir, args)`.
- Each suite has its own local helper: `a_conversation_that_committed(caller, &[(path, content)])` (range/diff) and `a_conversation_that_wrote` (reset) both return `(ConversationWorktree, Vec<short>)`. `AConversation::calls(tool, args)` runs through `run_in_conversation` with the real `tddy_tool_engine::execute_tool`.

### packages/tddy-discovery

**`subagent.rs` — `take_turn`** (`:1907-1969`), the exact code:
```rust
validate_yield_conditions(request.yield_conditions()).map_err(SubagentError)?;
if let Some(replacement) = request.replacement() { validate_replacement(replacement).map_err(SubagentError)?; }
let budget = request.budget_within(self.max_turns);
if request.prompt_text().is_some() || request.correction().is_some()
    || request.rewind_point().is_some() || request.replacement().is_some()
{ self.repeated_calls.forget_earlier_calls(); }
// The worktree goes back first: a reset that fails refuses the resume with the history whole…
let mut worktree_reset = None;
if let Some(rewind_point) = request.rewind_point() {
    if let (Some(port), true) = (&self.worktree_reset, request.resets_worktree()) {
        let target = self.transcript.commit_kept_by(rewind_point).map_err(|e| SubagentError(e.to_string()))?;
        worktree_reset = port.reset(target).await?;
    }
    self.transcript.rewind_to(rewind_point).map_err(|e| SubagentError(e.to_string()))?;
}
let appended_from = self.transcript.len();
if let Some(text) = request.prompt_text() { self.transcript.push(ChatMessage::user(text.to_string())); }
if let Some(correction) = request.correction() { self.transcript.push(ChatMessage::user(correction.to_string())); }
if let Some(replacement) = request.replacement() { self.transcript.append_replacement(replacement); }
let mut outcome = self.run_turn_loop(budget.turns, request.yield_conditions()).await?;
outcome.messages = self.transcript.descriptors_from(appended_from);
outcome.clamped_max_turns = budget.clamped_to;
outcome.worktree_reset = worktree_reset;
```

**Where a sync step fits.** Directly after the `if let Some(rewind_point)` block (`:1942`), so the order is reset, then sync. It goes either before `appended_from` (the notice is not reported in `messages`) or after it, before the prompt push (the notice is reported). Two constraints:
- Sync must also trigger `repeated_calls.forget_earlier_calls()`. A plain resume with no rewind currently does not clear it, and after a sync the same read can return new content.
- If a sync conflict must keep the history whole, the sync has to run **before** `rewind_to` (`:1939`), because the transcript cut happens right after the reset.

Other parts of `subagent.rs`:
- `PromptOutcome` (`:105-132`) has a `worktree_reset: Option<WorktreeReset>` field. `PromptOutcome::new` (`:137-148`) is the only constructor; there are no struct literals elsewhere.
- `SubagentConfig` (`:771-799`) has `worktree_reset: Option<Arc<dyn WorktreeResetPort>>`. It is built only through `new()` (`:803-810`), with `with_worktree_reset` (`:814`), `with_provider_queue` and `with_system_prompt`. There are no struct literals elsewhere.
- `SpecializedSubagentSession` has a `worktree_reset` field (`:1397`) and `resetting_worktree_through` (`:1430-1436`).
- `SubagentRegistry::create` wires it at `:2043-2045`.
- `run_one_turn` (`:1512-1612`) pushes each tool result with `worktree_change::of(&dispatch)` (`:1570-1579`). A fired yield returns mid-loop (`:1580-1591`), which can leave later `tool_calls` of the same assistant message unanswered. A user message appended after that point would sit inside an unanswered tool-call group.

**`subagent/worktree_reset.rs`** (`:17-22`):
```rust
#[async_trait] pub trait WorktreeResetPort: Send + Sync {
    async fn reset(&self, target: ResetTarget) -> Result<Option<WorktreeReset>, SubagentError>; }
```
It re-exports `ResetTarget` and `WorktreeReset` from tddy-subagent-worktree.

**`subagent/worktree_change.rs`**: `of(dispatch)` reads `result["worktreeChange"]`, ignores `{"error"}`, and deserializes the rest.

**`subagent/transcript.rs`**
- `TranscriptEntry.worktree_change` (`:211`).
- `MessageDescriptor.worktree_change` (`:141`), serialized `worktreeChange`.
- `commit_kept_by` (`:326-340`):
  ```rust
  let keep_through = self.last_kept_by(id)?;
  let commit = self.entries[..=keep_through].iter().rev()
      .find_map(|entry| entry.worktree_change.as_ref()?.commit.clone());
  Ok(commit.map_or(ResetTarget::Base, |commit| ResetTarget::Commit(commit)))
  ```
  Only tool results carry commits. Sync merges have no entry, so a rewind targets a subagent commit and drops any later sync merges.
- `rewind_to` / `last_kept_by` (`:355-378`) extend the cut over the tool messages that follow. `append_replacement` is at `:168-198`. `push`, `push_marked` and `push_tool_result` are at `:228-266`.

**`subagent/turn_request.rs`**
- Fields include `keep_worktree: bool` (`:44`).
- `keeping_worktree()` (`:83-86`) and `resets_worktree()` (`:89-91`). Unit tests at `:245-258`.
- A `keep_sync` / `syncs_worktree()` flag would go in the same place.

**`subagent_runtime.rs`**
- `prompt_outcome_json` (`:426-457`):
  ```rust
  if let (Some(object), Some(reset)) = (body.as_object_mut(), &outcome.worktree_reset) {
      object.insert("worktreeReset".to_string(), serde_json::json!(reset)); }
  ```
- `DeferredTurn` (`:586-598`) holds `session: Arc<tokio::Mutex<Box<dyn SubagentSession>>>`.
- `run_turn` (`:622-661`) locks the session, then calls `take_turn`, so turns of one conversation are serialised.

### packages/tddy-tools
- **`worktree_reset_port.rs`**: `ConversationWorktreeResetPort { conversation_id }`. `reset` calls `reset_conversation_worktree(id, commit)`, then `reset_from_answer`, which maps `is_error` to `Err`, `reset: null` to `None`, and parses anything else.
- **`pull_ledger.rs`**
  - `PullLedger { pulled: BTreeSet<String> }` with `record`, `pulled`, `dropped`, `annotate_reset` (which adds `droppedPulledCommits`).
  - The process-wide registry is `static PULLS: OnceLock<Mutex<HashMap<String, ConversationPulls>>>` (`:70-76`). `ConversationPulls { ledger, annotated: HashMap<response_id, String> }`.
  - Functions: `pulled_by`, `record_pulled`, `forget_conversation`, `annotated_turn_result(conv, response_id, result)`.
- **`reset_worktree_choice.rs`**: `with_reset_worktree_choice(request, args)`. Absent or `true` keeps the default, `false` calls `keeping_worktree()`, anything else returns `"resetWorktree must be a boolean"`. A `syncWorktree` parser would be a twin of this.
- **`subagent_end.rs`**
  - `subagent_end_tool` calls `pull_range(session_id, from, to)`, then `subagent_cancel_tool`, then returns `{"ended": true, "pulled": …}`.
  - `the_state_of` refuses an unknown conversation or a pending turn.
  - `discard_conversation_worktree` calls `forget_conversation`, then, if managed, `conversation_worktree(id, Remove)`.
- **`subagent_pull.rs`**: `pull_range` returns `Null` unless the loop runs here and access is managed. Otherwise it reads `pulled_by`, calls `pull_conversation_range`, records the `commits`, and returns `pulled`.
- **`subagent_diff.rs`**: refuses an unknown conversation, a remote one, or a non-managed one. Otherwise it calls `diff_conversation_worktree` and returns `answer["diff"]`.
- **`mcp_primitives.rs`** — `subagent_config_for_conversation` (`:111-120`):
  ```rust
  let config = SubagentConfig::new(subagent_codebase_access_from_env(Some(conversation_id)))
      .with_provider_queue(provider_queue());
  match subagent_access_is_managed() {
      true => config.with_worktree_reset(Arc::new(ConversationWorktreeResetPort::new(conversation_id))),
      false => config, }
  ```
  - The Managed closure is `managed_codebase_access(Some(conv))`, which calls `dispatch_conversation_tool(&conversation, &tool_name, args)` (`:63-80`).
  - `subagent_access_is_managed` (`:39-45`): `TDDY_SUBAGENT_CODEBASE_ACCESS`, else whether a transport was detected.
- **`server.rs`**
  - `subagent_new_session_tool` (`:1717-1807`) uses `subagent_config_for_conversation(&session_id)` at `:1781`. Remote agents go through `open_remote_agent_session` and get no port.
  - `subagent_prompt_tool` (`:1818-1848`): `turn_budget`, then `yield_conditions_of`, then `take_a_turn`. **It does not parse `resetWorktree`.**
  - `subagent_resume_tool` (`:1860-1909`): fromMessageId, correction, `with_reset_worktree_choice` (`:1892`), turn_budget, yield_conditions, replacement, `take_a_turn`.
  - `take_a_turn` (`:1959-2038`) spawns `run_turn` and wraps the result in `annotated_turn_result` (`:2028`). `subagent_await_tool` does the same at `:2067`.
  - `subagent_cancel_tool` (`:2082-2130`) calls `discard_conversation_worktree` when the loop ran locally.
  - Schemas: `subagent_prompt_schema` (`:2463-2490`) and `subagent_resume_schema` with its `resetWorktree` boolean description (`:2523-2531`). Routes are registered at `:2722-2733`.
- **How a conversation knows it has a worktree:** it doesn't. tddy-tools tracks no `worktreeChange`. It asks the daemon, whose `existing()` answers `reset: null` / `pulled: null` / a `FailedPrecondition` diff. `worktreeChange` appears in tddy-tools only inside tool descriptions.

### packages/tddy-session-tool-client/src/conversation.rs
- `ConversationWorktreeOp { Pull, Remove }` (`:27-32`).
- `dispatch_conversation_tool` (`:36-120`) builds an `ExecuteToolRequest` with `conversation_id` over SandboxIpc, DaemonUds, DaemonHttp or LiveKit (args clamped only on LiveKit).
- `conversation_worktree(conv, op)` (`:124`) calls `ask_conversation_worktree(builder)` (`:133-203`), which picks the same transports.
  - `ask_daemon` (`:234-253`) uses `call_unary("exec_tools.ExecToolService","ConversationWorktree")`. An RPC error becomes `error_body` (`{"error","is_error":true}`).
  - `ask_daemon_over_http` (`:256-315`) builds the Connect JSON by hand, one arm per op (`pull`, `remove`, `reset{commit}`, `diff{from,to}`, `pull_range{from,to,already_pulled}`). **A new op needs an arm here.**
  - `ask_daemon_over_livekit` is feature-gated.
- `reset_conversation_worktree`, `diff_conversation_worktree`, `pull_conversation_range` and their `*_request` builders use the `..conversation_worktree_request(envelope, id, Pull)` struct-update trick (`:332-405`).
- Request-seam unit tests are at `:450-547`.
- `ConversationWorktreeOp::Pull` has **no caller outside this file**. `Op::Pull` / `pull_into_caller` is now reached only by the daemon-rpc tests; `subagent_end` uses `pull_range`.

### Proto: packages/tddy-service/proto/exec_tools.proto
- `ConversationWorktreeRequest` fields 1-4: `session_token`, `session_id`, `daemon_instance_id`, `conversation_id`.
- `oneof op`: `pull = 10`, `remove = 11`, `reset = 12`, `diff = 13`, `pull_range = 14`. **The next free tag is 15.**
- Messages: `PullRangeOp{from=1, to=2, repeated already_pulled=3}`, `DiffOp{from=1, to=2}`, `ResetOp{commit=1}`, `PullOp{}`, `RemoveOp{}`, `ConversationWorktreeResponse{result_json=1}`.
- `ExecuteToolRequest.conversation_id = 6`.
- Generated TS is in `packages/tddy-web/src/gen/exec_tools_pb.ts` and `packages/tddy-rust-typescript-tests/gen/exec_tools_pb.ts`; every stack PR regenerated both.

### Daemon dispatch
- **`tddy-session-lifecycle/src/connection_service/conversation_worktree_op.rs`**
  - `run_conversation_worktree_op(session_worktree, session_id, conversation_id, op)` (`:31-93`) is the single dispatcher.
  - It parses `ConversationId`, then `ConversationWorktrees::new`, then `existing()`, then matches `(op, existing)`.
  - No-worktree answers: Pull and PullRange give `{"pulled": null}`, Remove gives `{"removed": false}`, Reset gives `{"reset": null}`, Diff gives `failed_precondition`.
  - Error mapping: `refused` gives `FailedPrecondition` (used by PullRange and Diff); `internal` gives `Internal` (used by Pull, Remove, Reset).
  - `DaemonSessionHost::conversation_worktree_from_jail` (`:113-139`) resolves the worktree from `session_dir_for(req.session_id)`, then `read_session_metadata().repo_path`.
- **`tddy-daemon-rpc/src/exec_tool/ports.rs` — `conversation_worktree`** (`:368-411`): peer routing (`rpc_served_by_peer`), then `authorize_exec_tool_caller`, then `resolve_exec_tool_worktree`, then `run_conversation_worktree_op`.
- **`daemon_rpc_handler.rs:193-195`** (the bridge arm): `(EXEC_TOOL_SERVICE, "ConversationWorktree") => RpcResult::Unary(conn.conversation_worktree_from_jail(payload).await)`.
- **`local_exec_tools.rs`**: `run_exec_tool_locally` branches on `conversation_id.is_empty()` (`:107-116`). `run_in_conversation_worktree` (`:192-226`) calls `run_in_conversation(…, |root| route_tool(request_at(req,&root,worktree_root), …))`.
- **`tddy-daemon-sandbox/src/sandbox_session.rs`**: `DaemonToolHandler::execute(session_id, conversation_id, tool_name, args_json)` (`:252-297`) calls `conversation_tool::execute_in_conversation(ToolCall{…})`. That function is in `tddy-daemon-sandbox/src/conversation_tool.rs:30-66` and runs `run_in_conversation` around `execute_tool_with_env`. DaemonToolHandler carries tool calls only; the worktree ops come through the runner relay and the bridge arm.
- **`tddy-sandbox-runner/src/conversation_root.rs`**: `bind_to_this_session` (`:42-64`) rewrites the `session_id` of any `IN_JAIL_RELAYABLE_EXEC_TOOLS` request and blanks the token and instance id. It is op-agnostic, so a new op needs no runner change.
- `tddy-tool-engine/src/exec_tool_entry.rs:15` lists `(EXEC_TOOL_SERVICE, "ConversationWorktree")` as relayable.

### 3. Grep / glob patterns and notable hits
- `reset|Reset|fn take_turn|push(|correction|replacement|worktree` in `subagent.rs`: `:1907-1969` and `:2043`.
- `commit_kept_by|worktree_change|…` in `transcript.rs`: `:141`, `:211`, `:239-266`, `:326`, `:355`.
- `worktree|prompt_outcome_json` in `subagent_runtime.rs`: `:426` and `:453-454`.
- `pull_ledger|worktree_reset_port|ConversationWorktree|resetWorktree|worktreeChange` in tddy-tools `src/`: 9 files.
- `subagent_config_for_conversation|resetWorktree|annotated_turn_result|…` in `server.rs`: `:1781`, `:1892`, `:2028`, `:2067`, `:2117`, `:2523`.
- `fn run_turn|struct DeferredTurn`: `subagent_runtime.rs:586` and `:622`.
- `conversation_worktree|ConversationWorktree` across packages: 44 files (listed in step 14).
- `run_in_conversation|conversation_worktree_from_jail|…`: `local_exec_tools.rs:16`, `:204`, `daemon_rpc_handler.rs:193`, `conversation_tool.rs:49`.
- `git diff --diff-filter=A fc49ff29 b16ddf9a`: the added tests and docs in section 6.
- `merge-tree`: `tddy-session-worktree/src/base_sync.rs:221-262` (precedent), `pr_stack/branch_legs.rs`, `base_sync_cache.rs`.
- `ConversationWorktreeOp::Pull` outside `conversation.rs`: no hits.
- `PromptOutcome {` / `SubagentConfig {` literals: none outside `subagent.rs`.
- `subagent_cancel|end|pull|diff` in `tddy-sandbox-recipes/src/claude_cli.rs`: allowlist at `:182-191`, tests at `:478-501`. No change is needed for a new argument.

### 4. Findings

**Git version.**
- `git --version` gives 2.43.0 (Homebrew, first on PATH); `/usr/bin/git` is 2.54.0 (Apple).
- `flake.nix:79` uses `pkgs.git` from a 2026 nixpkgs pin.
- So `merge-tree --write-tree` (needs ≥ 2.38) and `--merge-base` (needs ≥ 2.40) are available everywhere.
- The workspace already relies on it: `packages/tddy-session-worktree/src/base_sync.rs:221-262`. With `--name-only -z`, exit 0 means clean and exit 1 means conflict. In stdout, field 0 is the tree OID, the conflicted paths follow, and an empty field ends the list. Exit codes ≥ 2 are errors.

**Base recording.**
- The base is recorded in the ref `refs/tddy/subagent-base/<session>/<conv>` and the branch is `tddy/subagent/<session>/<conv>`.
- There is no ref for the last synced snapshot yet. A sibling such as `refs/tddy/subagent-synced/<session>/<conv>` would also need deleting in `remove()` (`worktree.rs:376-382`).

**Every listing and diff walks `base..branch` unfiltered.** After a merge whose second parent is the caller snapshot:
- **`commits_after_base`** (`range_pull.rs:90-94`) would list the merge commit, the snapshot commit, and every caller commit made since the base. `pull_commit` on a merge applies `merge^..merge`, which is the caller's own changes going back into the caller. This needs `--first-parent --no-merges` (or `--first-parent` plus skipping commits with two parents).
- **`reset_to`** (`reset.rs:36-44`, `--topo-order`) would accept a caller or snapshot commit as a target and report merges and caller commits in `droppedCommits`. That output feeds `PullLedger::annotate_reset`.
- **`diff`** (`diff.rs:36-40`) accepts caller commits as bounds. A tree diff of `base..tip` includes everything synced in. The diff semantics are an open decision. Options: first-parent-only bounds, diffing against the last merge's second parent, or excluding sync content.
- **`pull_into_caller`** (`worktree.rs:335`) is also a tree diff of `base..tip`, so it would re-apply the caller's synced content. Its only remaining callers are the daemon-rpc tests and the `Op::Pull` arm.

**Merge base matters.**
- When the caller was dirty at creation, `base` is an off-history "inherited" commit, and each new snapshot is a fresh `commit-tree -p HEAD` that is not chained to the previous one.
- Git's automatic merge base would then be the caller's HEAD ancestry, so inherited edits that the caller later changed again would conflict spuriously.
- Pass an explicit `--merge-base <base, or the last synced snapshot>`.
- Compare trees, not commits, to detect "no change". `base_commit` returns HEAD when the caller is clean, and the snapshot commit hash is non-deterministic.

**Conversation worktree state.**
- `commit_changes` stages everything. After writing the merge commit with `commit-tree -p tip -p snapshot`, the branch has to move and the worktree has to be updated (for example `update-ref` plus `reset --hard`, or `read-tree -m -u`) under `serialise::exclusive(root)`.
- A dirty conversation worktree is possible: a failed commit (`run.rs:80-83`) or a background job writing between turns. The sync must commit first or refuse.

**Rewind interaction.**
- `commit_kept_by` only sees tool-result commits, so a reset drops any later sync merges.
- With the order "reset, then sync", the "last synced" record must be invalidated or recomputed after a reset. One way is to check whether the recorded merge is still an ancestor of the tip.
- In `take_turn`, the reset currently runs before `rewind_to` so that a failed reset leaves the history whole. A sync conflict refusal needs the same placement: before `rewind_to` at `:1939`.

**No worktree.** `existing()` returning None should make sync a no-op: before the first write, reads already run in the live session worktree (`run.rs:65-68`).

**Messages.**
- The prompt and correction are plain `transcript.push(ChatMessage::user(..))` calls after `appended_from`.
- A sync notice pushed after `appended_from` appears in `outcome.messages`.
- `subagent_resume` documents that it sends "no new user message", so the notice changes that contract.

**Wiring for a sync port.** It mirrors the reset port:
- discovery: a new trait, a `SubagentConfig` field with a `with_…` builder, a session field with a `…_through` builder, and a line in `SubagentRegistry::create`.
- tddy-tools: a port struct, `subagent_config_for_conversation`, a `syncWorktree` parser for **both** `subagent_prompt` and `subagent_resume` plus schema entries, and an `annotated_turn_result` / `prompt_outcome_json` field.
- client: a new fn, its request builder, and an HTTP arm.
- proto: tag 15.
- daemon: a `run_conversation_worktree_op` arm. The bridge, ports and runner need no change.

**Open issues the stack recorded.**
- In `docs/dev/todo/`: abandoned branch; daemon-run conversations; host-bridge session binding; HTTP op encoding unverified; file-budget growth (`server.rs`, `subagent.rs`, `session-tool-client/lib.rs`); hooks/signing consent not given; the `subagent_end` race; transport selection repeated three times; the runner linking git code.
- Code-issues: `tddy-subagent-worktree/docs/code-issues/duplication-range-pull-pull-commit.md`, which proposes an `apply_range` extraction; plus oversized-file records in tddy-tools, discovery, session-tool-client, sandbox-runner and session-agents.
- Changesets: `docs/dev/changesets/2026-10-01-agent-worktree-{isolated-edits,rewind-reset,diff,range-pull}.md` and per-package copies.
- Feature doc `docs/ft/coder/managed-codebase-subagents.md`: § The conversation worktree (`:336-394`), § A rewind takes the worktree back (`:396-432`), § `subagent_diff` (`:434-468`), § `subagent_pull` (`:470-504`), § Known gaps (`:506-518`), AC 47-71 (`:735-786`), Non-goals (`:809-813`).
- The test headers still point at `docs/ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-isolated-edits.md`, which no longer exists.

### 6. Tests added by the stack
- **tddy-subagent-worktree**
  - `tests/conversation_worktree_acceptance.rs`: 26 tests.
  - `tests/run_in_conversation_acceptance.rs`: 11 tests, helper `AConversation`.
  - `tests/reset_acceptance.rs`: 5 tests.
  - `tests/diff_acceptance.rs`: 9 tests.
  - `tests/range_pull_acceptance.rs`: 9 tests.
  - `tests/support/mod.rs`: the builder above.
- **tddy-daemon-rpc** — `tests/conversation_worktree_{exec_tool,host_bridge,reset,diff,range_pull}_acceptance.rs`.
  - Each file copies the same fixture: `ADaemonWithASession` with `a_daemon_with_a_git_session()`, `tool_call(conv, tool, args)`, `conversation_worktree(conv, Op)`, `conversation_root`, `an_execute_tool`, `a_conversation_worktree_request(token, conv, op)`, and local `git` / `short_head`.
  - It is built on `test_util::test_service(sessions_dir)`, `TEST_TOKEN`, `a_session_metadata().with_repo_path(..)`.
  - There is no shared support module.
- **tddy-discovery**
  - `tests/rewind_resets_worktree_acceptance.rs`: `RecordingResets` port, which records `(ResetTarget, provider requests seen)` and answers via `PortAnswer::{Reset, NoWorktree, Fails}`; a wiremock provider; `a_codebase_committing_each_write()` (`worktreeChange.commit = c1, c2…`); `a_conversation_that_committed_twice(answer)`; `ids_of(role)`; `resets_asked()`; `requests_so_far()`; 9 tests.
  - `tests/worktree_change_summary_acceptance.rs`: `a_codebase_answering(result)`, `a_turn_where_the_model_calls`; 4 tests.
- **tddy-tools**
  - `tests/subagent_end_mcp_acceptance.rs`, `subagent_pull_mcp_acceptance.rs` and `subagent_diff_mcp_acceptance.rs`: each copies an `AnOpenConversation` fixture that spawns `tddy-tools --mcp` with `TDDY_SUBAGENTS_JSON` and offers `call`, `advertised_tool_names`, `advertised_schema_of`, `prompt_without_waiting`. With no transport configured, access is Local, so no worktree is ever created in these tests.
  - `tests/subagent_resume_reset_worktree_mcp.rs`: schema check only.
  - `mcp_tool_advertisement_audit.rs` was edited (`:77-79`).
