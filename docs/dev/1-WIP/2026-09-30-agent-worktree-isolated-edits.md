# Changeset: A subagent edits its own worktree, commits every change, and hands the result back

**Date**: 2026-09-30
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: `#agent-worktree` 1/4 — branch `feature/agent-worktree/isolated-edits`, base `master`

## Initial Discovery

[2026-09-30-agent-worktree-isolated-edits-initial-discovery.md](./2026-09-30-agent-worktree-isolated-edits-initial-discovery.md)

## Responsibility

The **conversation worktree** as a whole working capability, for the in-process subagent loop in
`tddy-tools` running with Managed access:

- lazy creation on the first mutating call, cut from the caller's `HEAD` plus its uncommitted state
  as one inherited commit;
- routing every tool call of the conversation to it once it exists;
- one commit per mutating call that changed files, and the change facts (`worktreeChange`) on that
  call's summary;
- `subagent_end` (pull base..tip 3-way into the caller as uncommitted changes, then remove) and
  `subagent_cancel` (remove without pulling).

It owns the new crate `tddy-subagent-worktree`, `ExecuteToolRequest.conversation_id`, the
`ExecToolService/ConversationWorktree` RPC with its `Pull` and `Remove` operations, the jail relay of
both, and the fail-closed read-only tool classifier.

## Boundaries

- **No reset on rewind** — `resetWorktree`, the `Reset` operation and `worktreeReset` belong to
  `feature/agent-worktree/rewind-reset`.
- **No diff tool** — the `Diff` operation and `subagent_diff` belong to `feature/agent-worktree/diff`.
- **No ranges and no ledger** — `Pull` here always applies base..tip as one diff and takes no commit
  list; `subagent_pull`, `subagent_end`'s `from`/`to` and the pulled-commit ledger belong to
  `feature/agent-worktree/range-pull`. Do not add optional range fields "for later".
- **No daemon-run conversations.** `open_local`, `open_owned` and `RemoteAgentSession` keep today's
  root. `local_agent_codebase_access` is not edited.
- **No peer-clone changes.** `session_agent_clone.rs`, `run_hosted_clone_tool` and the roster PRD's
  non-goals are untouched.

## Dependencies

None — this is the root node, cut from `master`.

## Draft PR contract

The first push after this planning commit publishes:

- `packages/tddy-subagent-worktree` with its public surface — `ConversationId::parse`,
  `ConversationWorktrees::{new, ensure, existing}`, `ConversationWorktree::{root, base,
  commit_changes, pull_into_caller, remove}`, `WorktreeChange`, `FileCounts`, `LineCounts`,
  `PullOutcome`, `ToolEffect::of`, and `run_in_conversation(…)` (route + execute + commit + merge the
  change facts into the result JSON) — every body `// TODO(isolated-edits): implement`;
- `exec_tools.proto`: `ExecuteToolRequest.conversation_id = 6`, `ConversationWorktreeRequest`
  (`oneof op { PullOp pull; RemoveOp remove; }`), `ConversationWorktreeResponse`, the
  `ConversationWorktree` RPC, and `ExecToolHandler::conversation_worktree`;
- `tddy_discovery::subagent::transcript::MessageDescriptor::worktree_change`;
- `subagent_end` registered in `tddy-tools`' router with its schema;
- the failing acceptance and unit tests listed below.

That surface is the interface this PR goes on to implement; it is not this PR's deliverable, and the
PR must not merge in that state.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — nothing it tests depends on another node
**Concurrent with:** none (it is wave 1 alone)
**Blocks:** `feature/agent-worktree/rewind-reset`, `feature/agent-worktree/diff`,
`feature/agent-worktree/range-pull` — each drives a conversation worktree this node creates

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n4

## Prerequisites

Open items this change runs into. No code issue in any touched package carries `Claimed by:`.
`tddy-git` has no `docs/code-issues/` and has not been analyzed; this change does not edit it.

### ⚠ DURING — A jail rebuild can re-run a tool call that already executed — [`2026-09-26-a-jail-rebuild-can-re-run-a-tool-call-that-already-executed.md`](../todo/2026-09-26-a-jail-rebuild-can-re-run-a-tool-call-that-already-executed.md)

A re-run `Write` or `Shell` now runs in the conversation worktree, and the commit follows the second
run, so the at-least-once property becomes visible as *one* commit holding both effects (a `Write`
is idempotent; a `Shell` side effect is not). Recorded, not fixed here: the accepted-risk decision
stands, and the commit makes it easier to see, not worse.

### ℹ ANSWERED — A conversation id is not bound to the session that opened it — [`2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md`](../todo/2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md)

The gap does not transfer to the new RPC: the conversation worktree is resolved **under the
token-resolved session worktree**, so a foreign conversation id can only ever name a directory inside
the caller's own session. The entry's own gap in `SessionAgentService` stays open.

### ⚠ DURING — A turn's message list can overflow the chunk-framing threshold — [`2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md`](../todo/2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md)

`worktreeChange` adds a bounded ~120-byte object to each mutating tool-role descriptor. It rides only
the MCP path in this node (daemon-run conversations are out of scope), which does not frame. Keep it
bounded — no path lists in the per-call facts.

### ⚠ DURING — Seven files over budget — [`2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md`](../todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md)

`tddy-tools/src/server.rs`, `tddy-discovery/src/subagent.rs`, `tddy-sandbox-runner/src/runner.rs`
and `tddy-session-tool-client/src/lib.rs` are over 500 lines (code issues
`packages/tddy-tools/docs/code-issues/oversized-file-server.md`,
`packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md`,
`packages/tddy-sandbox-runner/docs/code-issues/oversized-file-runner.md`,
`packages/tddy-session-tool-client/docs/code-issues/oversized-file-lib.md`). New logic goes into new
modules — `subagent_end` into its own module under `tddy-tools/src/`, the per-conversation dispatch
into its own module of `tddy-session-tool-client` — and each of those files grows only by its wiring
line(s).

### ⚠ DURING — `stream_execute_tool` complexity — `packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-stream-execute-tool.md`

Unclaimed; nesting 5. `conversation_id` must not add a branch inside it: the conversation root is
resolved in the shared route (`run_exec_tool_locally`) that both `execute_tool` and
`stream_execute_tool` already reach.

## Affected Packages

- **tddy-subagent-worktree** (new): README + `docs/conversation-worktree.md` — the git mechanics
- **tddy-service**: [README.md](../../packages/tddy-service/README.md) — `exec_tools.proto`
- **tddy-tool-engine**: [README.md](../../packages/tddy-tool-engine/README.md) — `ExecToolHandler::conversation_worktree`, service adapter
- **tddy-daemon-rpc**: [README.md](../../packages/tddy-daemon-rpc/README.md) — the handler
- **tddy-session-lifecycle**: [README.md](../../packages/tddy-session-lifecycle/README.md) — the exec-tool route resolves the conversation root
- **tddy-sandbox-runner**: [README.md](../../packages/tddy-sandbox-runner/README.md) — relays `conversation_id` and `ConversationWorktree`
- **tddy-session-tool-client**: [README.md](../../packages/tddy-session-tool-client/README.md) — per-conversation dispatch and the `ConversationWorktree` call
- **tddy-discovery**: [roster-and-subagent-runtime.md](../../packages/tddy-discovery/docs/roster-and-subagent-runtime.md) — `worktree_change` on the descriptor and in `prompt_outcome_json`
- **tddy-tools**: [README.md](../../packages/tddy-tools/README.md) — `subagent_end`, `subagent_cancel` removes, the Managed closure carries the conversation id
- **tddy-sandbox-recipes**: allowlists `mcp__tddy-tools__subagent_end`

## Related Feature Documentation

- [PRD-2026-09-30-agent-worktree-isolated-edits.md](../../ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-isolated-edits.md)
- [managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md)

## Summary

A subagent conversation gets its own branch and worktree inside the session worktree, created on its
first mutating tool call and seeded with the caller's uncommitted state. Each mutating call that
changes files is committed there, and its summary reports the files and lines it created, updated and
deleted with the commit's short hash. `subagent_end` hands the result to the caller as uncommitted
changes; `subagent_cancel` discards it.

## Background

See the PRD. In short: today the subagent and its caller write the same tree, a summary cannot say
what changed on disk, and nothing can be taken back.

## Scope

- [ ] **Package Documentation**: new crate README + docs; updated READMEs for the packages above
- [ ] **Implementation**: mechanics crate, wire, daemon route, jail relay, subagent loop, MCP tools
- [ ] **Testing**: all acceptance tests below passing
- [ ] **Integration**: an in-jail subagent's write lands in its conversation worktree end-to-end
- [ ] **Technical Debt**: TODOs filed for daemon-run conversations and the orphan sweep
- [ ] **Code Quality**: scoped clippy/fmt per touched package

## Technical Changes

### State A (Current)

- `CodebaseAccess::Managed` (`tddy-discovery/src/subagent.rs:211`) dispatches each call through
  `tddy_session_tool_client::dispatch_session_tool(tool, args)` →
  `ExecuteTool{session_token, session_id, tool_name, args_json, daemon_instance_id}`
  (`exec_tools.proto:52`). No conversation id travels.
- The jail's relay forwards only `tool_name` and `args_json` (`tddy-sandbox-runner/src/runner.rs:79`).
- The daemon resolves the root from `.session.yaml` `repo_path` (`resolve_exec_tool_worktree`) and
  runs the tool on `HostWorktree` or in the workspace `Jail` (`local_exec_tools.rs:100-166`). The jail
  mounts only the session worktree (`workspace_tool_sandbox.rs:226`).
- Results: `Write` → `{bytes_written}`, `StrReplace` → `{replaced, matchedOccurrences, bytes_written,
  edited_region, edited_line}`, `Delete` → `{deleted}`, `Shell` → `{stdout, stderr, exit_code}`.
- `ResultSummary` (`subagent/result_summary.rs`) has no file/line/commit facts; `MessageDescriptor`
  (`subagent/transcript.rs:107`) has no change field.
- Two read-only classifiers disagree: `SubagentTool::is_mutating` (`agent_def.rs:46`) and the
  fail-closed `agent_tool_reads_the_clone` (`tddy-session-lifecycle/.../agent_roster.rs:154`).
- `subagent_cancel` (`tddy-tools/src/server.rs:2065`) retires the conversation; nothing on disk.
- Git helpers exist only privately (`session_room::write_wip_tree_within`, `diff_between`,
  `git_output`) or for other purposes (`tddy-session-sync::Mirror::apply`,
  `tddy-worktree-service::parse_git_diff_numstat`, `tddy-git::worktree`).

### State B (Target)

```
tddy-tools (in jail)                             facilitating daemon
 subagent loop ──Managed(conv)──► ExecuteTool{…, conversation_id} ──► exec route
                                                                     │ ToolEffect::of(tool)
                                                                     │ ConversationWorktrees::existing / ensure
                                                                     │ run tool at root = conv worktree
                                                                     │ commit_changes → WorktreeChange
                                                                     ▼ result_json + "worktreeChange"
 subagent_end ──► ConversationWorktree{Pull} then {Remove}  ──► pull_into_caller (3-way) / remove
 subagent_cancel ──► ConversationWorktree{Remove}
```

- **`tddy-subagent-worktree`** (git CLI, `std::process::Command`, `GIT_OPTIONAL_LOCKS=0`, null stdin,
  fixed committer identity `tddy-subagent <tddy-subagent@tddy.invalid>`):
  - `ConversationId::parse(&str) -> Result<ConversationId, UnsafeConversationId>` —
    `[A-Za-z0-9._-]{1,64}`, no leading `.`.
  - `ConversationWorktrees::new(session_worktree, session_id)`; `existing(&id) -> Option<…>`;
    `ensure(&id) -> Result<ConversationWorktree, WorktreeError>` — on first call: add
    `/tmp/subagent-worktrees/` to `<common dir>/info/exclude` if absent; snapshot the caller's
    uncommitted state through a scratch `GIT_INDEX_FILE` (seeded from the caller's index, `add -A`
    excluding `tmp/subagent-worktrees/`, `write-tree`, `commit-tree -p HEAD`) when it differs from
    `HEAD`; `git worktree add -b tddy/subagent/<session>/<conv> <path> <base>`.
  - `ConversationWorktree::{root, base, commit_changes(subject) -> WorktreeChange,
    pull_into_caller() -> PullOutcome, remove()}`. `commit_changes` stages everything, derives the
    facts from `diff --cached --name-status` + `--numstat` (A = created, M/T = updated, D = deleted,
    R = one deleted + one created; binary `-` counts no lines) and commits only when non-empty.
    `pull_into_caller` runs `git diff --binary base..tip | git apply --3way` in the caller's worktree
    and reports conflicted paths. `remove` = `git worktree remove --force` + `git branch -D`.
  - `ToolEffect::of(tool_name) -> ToolEffect::{ReadOnly, Mutating}` — `ReadOnly` for exactly
    `Read | Glob | Grep | SemanticSearch | ReadLints`, `Mutating` for everything else.
  - `run_in_conversation(worktrees, &id, tool_name, execute) -> String` — the whole per-call rule:
    read-only before creation → session root; mutating → `ensure`; run `execute(root)`; mutating →
    `commit_changes` and merge `"worktreeChange"` into the result object.
- **Wire**: `ExecuteToolRequest.conversation_id = 6` (empty = today's behaviour);
  `rpc ConversationWorktree(ConversationWorktreeRequest) returns (ConversationWorktreeResponse)` with
  `session_token, session_id, daemon_instance_id, conversation_id, oneof op { PullOp pull = 10;
  RemoveOp remove = 11; }` and `result_json`.
- **Daemon**: `run_exec_tool_locally` wraps the route in `run_in_conversation` when
  `conversation_id` is set; `ExecToolRpcHandler::conversation_worktree` authorizes like
  `execute_tool`, resolves the session worktree, and runs `Pull` / `Remove`.
- **Jail relay**: `SandboxSessionRelay::call_tool` carries `conversation_id`; the runner forwards
  `ConversationWorktree` to the host.
- **Discovery**: `MessageDescriptor::worktree_change: Option<WorktreeChange>` read from the tool
  result's `worktreeChange` at the append site; serialized by `prompt_outcome_json` as
  `worktreeChange`.
- **tddy-tools**: the Managed closure is built per conversation and passes its id;
  `subagent_end{sessionId}` (own module) refuses while a turn is pending, sends `Pull` then `Remove`,
  retires the conversation, answers `{ended, pulled}`; `subagent_cancel` sends `Remove` after
  retiring.

### Delta (What's Changing)

#### tddy-subagent-worktree (new)
- Crate, README, docs; dev-deps `tempfile`, `tddy-testing-commons`, `tddy-tool-engine`, `tokio`.

#### tddy-service
- `exec_tools.proto` fields and RPC above; regenerated bindings (and `tddy-rust-typescript-tests` if
  the TS bindings include `exec_tools`).

#### tddy-tool-engine
- `ExecToolHandler::conversation_worktree`; `ExecToolServiceImpl` dispatch.

#### tddy-daemon-rpc
- `ExecToolRpcHandler::conversation_worktree`.

#### tddy-session-lifecycle
- `run_exec_tool_locally` routes through `run_in_conversation`; `agent_tool_reads_the_clone`
  delegates to `ToolEffect::of` (one classifier).

#### tddy-sandbox-runner
- `call_tool(conversation_id, tool, args)`; `ConversationWorktree` forwarded.

#### tddy-session-tool-client
- `dispatch_conversation_tool(conversation, tool, args)` and `conversation_worktree(conversation,
  op)` in a new module.

#### tddy-discovery
- `MessageDescriptor::worktree_change`; `prompt_outcome_json` emits it.

#### tddy-tools
- `subagent_end` module; `subagent_cancel` removes; per-conversation Managed closure.

#### tddy-sandbox-recipes
- `mcp__tddy-tools__subagent_end` on the three allowlists that carry `subagent_cancel`.

## Implementation Milestones

- [ ] `ConversationId`, `ToolEffect` and their unit tests green
- [ ] `ensure` creates the worktree with the inherited commit and exclude entry
- [ ] `commit_changes` facts green for create / update / delete / rename / binary / no-op
- [ ] `pull_into_caller` clean and conflicted cases green; `remove` green
- [ ] `run_in_conversation` routing green with the real tool engine
- [ ] Wire + daemon handler + jail relay green
- [ ] Discovery descriptor + MCP `subagent_end` / `subagent_cancel` green
- [ ] Allowlists updated

## Testing Plan

**Levels.** The mechanics are pure git and are tested against real temporary repositories — no
doubles, because a git double would test nothing. The per-call rule is tested with the **real**
`tddy_tool_engine::execute_tool` as the executor, so a `Write` really writes. The daemon handler is
tested in `tddy-daemon-rpc`'s existing exec-tool acceptance style. The subagent loop is tested with
the established wiremock provider plus a Managed closure answering with a real-shaped result. The MCP
surface is tested over the real `tddy-tools --mcp` stdio wire.

**Options weighed.** A single end-to-end test through a jail would prove the relay but costs a
sandbox per run and was rejected as the primary proof; the relay gets a focused test instead
(`conversation_id` survives `call_tool`), which is the one field the discovery shows is dropped today.

### Acceptance Tests

#### tddy-subagent-worktree — `packages/tddy-subagent-worktree/tests/conversation_worktree_acceptance.rs`
- `a_conversation_that_only_reads_creates_no_worktree_and_no_branch`
- `the_first_mutating_call_cuts_the_worktree_from_the_callers_head`
- `the_callers_uncommitted_and_untracked_changes_become_one_commit_on_the_subagent_branch`
- `creating_the_worktree_leaves_the_callers_index_branch_and_files_untouched`
- `the_conversation_worktree_never_shows_in_the_callers_status`
- `a_change_is_committed_with_its_created_updated_and_deleted_counts`
- `a_call_that_changed_nothing_makes_no_commit`
- `a_binary_file_counts_as_a_file_and_adds_no_lines`
- `an_unsafe_conversation_id_is_refused_before_any_git_state_exists`
- `pulling_applies_everything_since_the_base_as_uncommitted_changes`
- `a_pull_over_the_callers_own_edit_leaves_conflict_markers_and_names_the_path`
- `pulling_never_moves_the_callers_head`
- `removing_deletes_the_worktree_and_its_branch`

#### tddy-subagent-worktree — `packages/tddy-subagent-worktree/tests/run_in_conversation_acceptance.rs`
- `a_read_before_any_write_reads_the_session_worktree`
- `a_write_lands_in_the_conversation_worktree_and_reports_its_commit`
- `a_read_after_a_write_sees_the_subagents_own_write`
- `a_shell_call_that_creates_files_reports_them`
- `await_is_treated_as_mutating`
- `a_read_only_call_carries_no_worktree_change`

#### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/conversation_worktree_exec_tool_acceptance.rs`
- `an_execute_tool_carrying_a_conversation_id_writes_to_that_conversations_worktree`
- `an_execute_tool_without_a_conversation_id_writes_to_the_session_worktree`
- `pull_hands_the_conversations_changes_to_the_session_worktree`
- `remove_deletes_the_conversation_worktree`
- `a_conversation_worktree_call_with_a_foreign_token_is_refused`

#### tddy-sandbox-runner — `packages/tddy-sandbox-runner/tests/conversation_tool_relay.rs`
- `the_relay_carries_the_conversation_id_to_the_host`
- `the_relay_forwards_conversation_worktree_calls_to_the_host`

#### tddy-discovery — `packages/tddy-discovery/tests/worktree_change_summary_acceptance.rs`
- `a_mutating_call_reports_its_worktree_change_in_the_turn_outcome`
- `a_read_reports_no_worktree_change`
- `a_mutating_call_that_changed_nothing_reports_counts_without_a_commit`

#### tddy-tools — `packages/tddy-tools/tests/subagent_end_mcp_acceptance.rs`
- `subagent_end_is_advertised`
- `ending_a_conversation_that_never_wrote_pulls_nothing`
- `ending_a_conversation_with_a_turn_running_is_refused`
- `an_ended_conversation_can_no_longer_be_prompted`

#### tddy-sandbox-recipes — unit tests in `packages/tddy-sandbox-recipes/src/claude_cli.rs`
- `subagent_end_is_allowlisted_wherever_subagent_cancel_is`

### Unit tests
- `tddy-subagent-worktree/src/conversation_id.rs` — charset, length, leading dot, `..`, `/`, empty
- `tddy-subagent-worktree/src/tool_effect.rs` — the five read-only names, `Await`, unknown, case
- `tddy-subagent-worktree/src/change_facts.rs` — `--name-status` / `--numstat` parsing incl. renames and binary

## Technical Debt & Production Readiness

(populated during development)

## Decisions & Trade-offs

- **Worktree inside the session worktree** (developer choice): it is the one directory every route —
  host and jail — can already reach. Cost: it relies on `info/exclude` to stay out of the caller's
  status, and it is deleted with the session worktree.
- **Branch name carries the session id**: sessions of one project share the common git dir, and the
  caller chooses conversation ids.
- **Git runs on the daemon host** — the jail may not see the common dir a linked worktree points at.
- **Typed RPC over reserved tool names** (developer choice) — costs a relay entry, buys a typed op.
- **3-way with conflict markers** (developer choice) over refuse-and-keep.
- **Pull at the end only** in this node (developer choice); mid-conversation pulls are 4/4.
- **Fail-closed classifier** — one list of read-only tools, everything else commits.

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## Successor PRs

- `feature/agent-worktree/rewind-reset` — a rewind resets the worktree
- `feature/agent-worktree/diff` — `subagent_diff`
- `feature/agent-worktree/range-pull` — `subagent_pull` and ranges

## TODO

- [x] Record initial discovery (`2026-09-30-agent-worktree-isolated-edits-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests per touched package; full workspace on CI
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — also deletes the initial-discovery companion
- [ ] USER REVIEW — work complete, decide next steps
