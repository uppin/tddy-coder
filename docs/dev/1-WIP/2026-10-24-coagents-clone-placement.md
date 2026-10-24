# Changeset: A subagent can work on its own clone instead of the main agent's worktree

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — proto change (attach + entry placement), git worktree seam on the daemon, three crates plus the web
**Branch**: `feature/coagents/clone-placement`, base `feature/coagents/conversation-bubble` (#723) — `#coagents` 5/8
**PR**: [#724](https://github.com/uppin/tddy-coder/pull/724)

## Initial Discovery

[2026-10-24-coagents-clone-placement-initial-discovery.md](./2026-10-24-coagents-clone-placement-initial-discovery.md)

## Prerequisites

No code issue in a touched package carries `Claimed by:`.

### ✅ RESOLVED HERE — Daemon-run subagent conversations still write the session worktree — [`2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md`](../todo/2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md)

Resolved for the **local** daemon-run case by `clone` placement (the user's choice that `shared`
stays the default is recorded in the PRD). The peer-owned case keeps the roster's "no write-back from
a remote clone" non-goal — wrap narrows the entry to that remainder.

### ⚠ DURING — Automatic subagent commits skip hooks and signing — consent NOT given — [`2026-10-01-subagent-commits-skip-hooks-and-signing-without-consent.md`](../todo/2026-10-01-subagent-commits-skip-hooks-and-signing-without-consent.md)

The turn-end commit goes through the same git runner and inherits the same overrides. No new policy.

### ⚠ DURING — An abandoned subagent conversation leaves its branch — [`2026-09-30-an-abandoned-subagent-conversation-leaves-its-branch.md`](../todo/2026-09-30-an-abandoned-subagent-conversation-leaves-its-branch.md)

A daemon restart now leaves daemon-run conversation branches too. Not swept here.

### ⚠ DURING — A background job can race the caller sync — [`2026-10-03-a-background-job-can-race-the-caller-sync.md`](../todo/2026-10-03-a-background-job-can-race-the-caller-sync.md)

A coding agent's own background processes are outside every lock. Same exposure, larger surface.

## Responsibility

- `AgentPlacement { SHARED = 0, CLONE = 1 }`; `AttachSessionAgentRequest.placement` (5); `SessionAgentEntry.placement`; `SessionAgentRecord.placement` (serde default `Shared`).
- Daemon open path: for `Clone`, `ConversationWorktrees::ensure` at open; `local_agent_codebase_access` sends the conversation id so `run_in_conversation` applies; a coding agent's cwd / MCP exec target is the conversation worktree.
- `turn_start` sync before every daemon-run turn (reusing `tddy-subagent-worktree`'s sync).
- One commit per coding-agent turn on the conversation branch, through the existing `Worktree::commit_changes` (no new worktree-crate API).
- `tddy-tools`: `subagent_diff` / `_pull` / `_end` route daemon-run `Clone` conversations to the daemon's `ConversationWorktree` ops; descriptions updated for `Shared`.
- Web: placement select per agent in `CreateSessionAgentPickerSection` and `AgentPicker`; placement on `RosterAgentRow`; `specialized_agents` entries carry the placement (`StartSessionRequest` seeding).

## Boundaries

- No remote placement — n8. No sweeping of orphaned branches. No change to in-jail conversations.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n2 `acp-subagent` (#721) | `CodingAgentLauncher` (cwd, MCP config), daemon open path, `SessionAgentRecord.kind` | passes the conversation worktree as cwd / exec target; adds `placement` beside `kind` | change the launcher contract or the catalog |
| n1, n3, n5 | inherited, not consumed | — | — |

## Draft PR contract

Proto + record fields, `commit_turn_changes`, the placement branch's signatures in the open path,
the web select — bodies `TODO(clone-placement)` — and the failing tests below.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — needs n2's daemon-run coding-agent behaviour
**Concurrent with:** n3, n5
**Blocks:** n8 `remote-codebase` (its clone-through-the-relay test needs this node's placement behaviour)

    n1 → n2      n2 → n3, n5, n7, n8      n3 → n4, n6      n5 → n6      n7 → n8

## Affected Packages

- **tddy-service**: `session_agents.proto` (regenerated TS)
- **tddy-changeset**: `SessionAgentRecord.placement`
- **tddy-session-agents**: open path, turn-start sync, turn-end commit
- **tddy-subagent-worktree**: `commit_turn_changes` (if not already expressible with its commit helper)
- **tddy-tools**: diff/pull/end routing
- **tddy-web**: placement select and roster label

## Related Feature Documentation

- [PRD-2026-10-24-coagents-clone-placement.md](../../ft/coder/1-WIP/PRD-2026-10-24-coagents-clone-placement.md)

## Summary

`clone` placement gives a daemon-run conversation its own conversation worktree, synced before each
turn and handed back through `subagent_diff` / `_pull` / `_end`.

## Scope

- [ ] **Implementation** · [ ] **Testing** · [ ] **Package Documentation** · [ ] **Code Quality**

## Technical Changes

### State A (Current)

- `svc_start_hosted_agent_clone.rs:269-334` `local_agent_codebase_access` → `ExecuteToolRequest { conversation_id: "" }` → `run_exec_tool_locally` on the session worktree.
- `local_exec_tools.rs:195-231` `run_in_conversation` applies only when `conversation_id` is set.
- `tddy-subagent-worktree`: worktree at `<session worktree>/tmp/subagent-worktrees/<conv>`, branch `tddy/subagent/<session>/<conv>`; ops in `tddy-agent-launch/src/conversation_worktree_op.rs:41-60` (`Pull`, `PullRange`, `Remove`, `Reset`, `Diff`, `Sync`), reachable over `exec_tools.ConversationWorktree`.
- `tddy-tools/src/subagent_pull.rs:62-85` returns `null` unless the loop runs in-process.
- `AttachSessionAgentRequest` fields 1-4; no placement anywhere.

### State B (Target)

```
attach{placement: CLONE} → record.placement
open(conv) → ensure conversation worktree → coding agent cwd / exec target = it
turn: sync(caller files) → prompt → commit_turn_changes
subagent_pull/diff/end (daemon-run, CLONE) → ConversationWorktree{PullRange|Diff|Remove}
```

## Testing Plan

**Level:** integration — real git worktrees under a temp repo and a stub coding agent that writes a
file (the `tddy-acp-stub` scenario gets a `write_file` action, owned here); `tddy-tools-cli` MCP tests
for diff/pull/end; Cypress component for the select.

## Acceptance Tests

Build the stub first (`cargo build -p tddy-acp-stub`).

`packages/tddy-daemon-rpc/tests/clone_placement_acceptance.rs` (real git session worktree, a recording launcher):

1. `attaching_with_clone_records_the_placement` — fails: the entry's `placement` stays SHARED
2. `a_clone_coding_agent_conversation_launches_on_a_worktree_and_branch_of_its_own` — branch `tddy/subagent/<session>/<conv>`, holds the caller's files; needs #721 green (coding-agent rows) and this node
3. `a_shared_coding_agent_conversation_launches_on_the_session_worktree` — the default; needs #721 green, then passes unchanged (pins the default)

`packages/tddy-tools/tests/worktree_route_acceptance.rs` — `worktree_route::has_its_own_worktree`, the decision `subagent_diff` / `_pull` / `_end` take:

4. `an_in_process_conversation_under_managed_access_has_its_own_worktree` (fails)
5. `an_in_process_conversation_reading_the_host_directly_has_none` (negative, passes)
6. `a_daemon_run_conversation_of_a_clone_placed_agent_has_its_own_worktree` (fails)
7. `a_daemon_run_conversation_of_a_shared_agent_has_none` (negative, passes)

`packages/tddy-web/cypress/component/CreateSessionAgentPicker.cy.tsx` (extended):

8. `an agent can be attached with clone placement` — `StartSession.clonePlacedAgents == [id]` (fails: no placement control)
9. `an agent picked without choosing a placement works on the session's worktree` (negative, passes)

**Added in green (2026-10-24).** Every test below is green. The daemon-side routing test drives the
`ConversationWorktree` operations the tools call; `subagent_diff` / `_pull` / `_end` themselves are
not run, and `has_its_own_worktree` stays pinned by `worktree_route_acceptance`.

`packages/tddy-session-agents/tests/clone_placement_reattach_acceptance.rs` (stub ports, a spy admission):

- `attaching_an_agent_shared_then_clone_is_refused_naming_its_shared_placement`
- `attaching_an_agent_clone_then_shared_is_refused_naming_its_clone_placement`
- `a_refused_placement_change_leaves_the_roster_unchanged_and_admits_nothing`
- `attaching_the_same_placement_again_returns_the_roster_at_the_same_revision`
- `the_roster_store_refuses_a_second_placement_for_an_attached_agent`

`packages/tddy-session-agents/src/clone_placement/sync_notice_tests.rs` (unit, recording and scripted fake sessions):

- `a_turn_after_a_caller_change_is_sent_a_notice_naming_the_changed_file`
- `a_turn_with_no_caller_change_is_sent_its_prompt_as_it_was_written`
- `a_merge_on_a_turn_with_no_prompt_sends_the_agent_no_prompt`
- `a_notice_held_past_a_turn_with_no_prompt_rides_on_the_next_prompt_once`
- `a_notice_survives_a_prompt_turn_that_fails`
- `a_turn_that_skips_the_caller_sync_is_sent_no_notice_and_leaves_the_caller_change_out`
- `a_turn_that_skips_the_caller_sync_is_not_refused_by_a_conflicting_caller_change`

`packages/tddy-daemon-rpc/tests/clone_placement_exec_call_acceptance.rs` (managed claude-acp session):

- `a_clone_coding_agent_is_launched_naming_the_conversation_its_exec_calls_run_in`
- `a_clone_coding_agents_tool_server_is_handed_the_conversation_its_exec_calls_run_in`
- `a_shared_coding_agents_tool_server_is_handed_no_conversation`
- `an_exec_call_of_a_clone_conversation_lands_in_its_conversation_worktree_not_the_session_worktree`
- `an_exec_call_of_a_shared_conversation_lands_in_the_session_worktree`

`packages/tddy-daemon-rpc/tests/clone_placement_routing_acceptance.rs` (real git session worktree; the
stub writes `notes/from-agent.txt` on its turn):

- `the_diff_of_a_clone_conversation_shows_what_its_agent_wrote`
- `pulling_a_clone_conversation_brings_its_work_into_the_session_worktree`
- `ending_a_clone_conversation_removes_its_worktree_and_its_branch`
- `a_shared_conversation_has_no_worktree_to_diff_pull_or_end`

`packages/tddy-tools/tests/clone_placement_exec_dispatch_acceptance.rs` (a local listener stands in for the daemon's exec endpoint):

- `an_exec_call_names_the_conversation_the_launch_named_and_none_otherwise`

**Changed from the plan:** no `commit_turn_changes` — `Worktree::commit_changes` already commits everything a turn changed. The per-turn sync, per-turn commit and the `pull`/`diff`/`end` round trips for daemon-run conversations are pinned at their decision seam (4–7) and the launch (2); the daemon-side routing harness now exists: `packages/tddy-daemon-rpc/tests/clone_placement_routing_acceptance.rs` drives the `ConversationWorktree` Diff, PullRange and Remove ops on a real git session worktree. The tools' MCP process end to end against a real daemon is still not run. A model agent's per-call commits in a clone run through the existing `run_in_conversation`, already covered by `conversation_worktree_exec_tool_acceptance`. Seeding at start carries the placement as `StartSessionRequest.clone_placed_agents = 44` (the subset of `specialized_agents`), since `specialized_agents` is a list of ids. New proto: `types.AgentPlacement`, `AttachSessionAgentRequest.placement = 5`, `SessionAgentEntry.placement = 15`.

**Baseline (2026-10-24):** `cargo check --all-targets` clean across the changeset, service, session-agents, tools, daemon-rpc, session-lifecycle, session-split, daemon, discovery, tools-cli; scoped clippy clean; the existing picker specs pass.

## Technical Debt & Production Readiness

- **A merge on a turn with no new prompt is held, not sent.** A permission answer continues a turn and
  carries no prompt for the sync notice to ride on, so the notice waits and rides on the next
  prompt-bearing turn (`take_turn` in `clone_placement.rs`). The permission-answer turn itself still
  runs without it, because the ACP session has no channel for it: a stated limit, not a silent drop.
- **Not this node:** 16 `tddy-session-lifecycle` integration tests fail with "sandbox RPC bridge not
  installed" (`sandboxed_session_*`, `sandboxed_claude_cli_*`, `sandboxed_cursor_cli_*`, the sandbox
  delete/resume tests). The same set is in the baseline run recorded earlier in this worktree; none of
  them touches the coding-agent path.
- **`tddy-tools` is not run against a daemon.** No harness runs the MCP process end to end. The
  tools' choice of dispatch is covered by a test that stands a local listener in for the daemon's exec
  endpoint; the daemon side is covered through the RPCs the tools call.

## Decisions & Trade-offs

- **Commit per turn for coding agents.** Their edits do not pass through tddy's tools when they run
  with native tools, so per-call commits are impossible; per-turn keeps `subagent_pull` meaningful.
- **Reuse the existing `ConversationWorktree` RPC** rather than adding end/pull/diff to
  `SessionAgentService` — the ops and their auth already exist.
- **`CodingAgentLaunch.conversation_id` runs a clone conversation's exec calls in its worktree.**
  Additive: `None` (every shared launch, and every launch that predates this node) keeps the session
  worktree. The clone open sets it. `DaemonCodingAgentLauncher::mcp_servers_for` hands it to
  `tddy-tools --mcp` as `TDDY_CONVERSATION_ID` (`tddy_coding_subagent::CONVERSATION_ID_ENV`), and
  `tddy-tools`' `dispatch_dynamic_tool` then calls `dispatch_conversation_tool`, so the daemon runs the
  call through `run_in_conversation`. The variable is added in `mcp_servers_for`, not in the
  tool-server resolver, so every resolver gets it.
- **A re-attach under the other placement is refused before admission.** `attach_session_agent` reads
  the entry the session already holds. A different placement is `FAILED_PRECONDITION` naming the
  placement it holds, and no checkout is admitted. The same placement keeps its idempotent path.
  `SessionAgentRoster::attach` refuses the same change too, as defence in depth.
- **The sync notice rides on the prompt.** A merged sync prepends `sync_notice` (the in-jail loop's
  text) to the new prompt through `TurnRequest::with_prompt_preface` (additive, `tddy-discovery`). No
  merge means no notice. A conflict still refuses the turn before the agent sees it.
- **`open_owned` (peer-owned) and remote placement belong to n8 `remote-codebase`.** Not implemented
  here.
- **The notice test uses a recording fake, not the stub.** The stub records lifecycle events, not
  prompts. The fake sits behind the real `CodingAgentConversationTurns` and a real conversation
  worktree, so the prompt observed is the one the agent would receive.
- **A merge on a prompt-less turn is held for the next prompt.** A permission answer has no prompt for
  the notice to ride on, and the notice must not be lost, so it waits, in order, for the next
  prompt-bearing turn. A prompt-bearing turn that fails keeps its notice too: a failed turn does not
  say whether the agent read its prompt. The prompt-less turn itself is still sent no prompt text.
- **A coding agent's turn honours `syncWorktree: false`.** The conflict refusal names
  `syncWorktree: false` as a way forward, and that promise is only true if the turn skips the sync; the
  in-jail loop already skips it the same way. A skipped sync merges nothing, notices nothing and is
  refused by no conflict.

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

## TODO

- [x] Record initial discovery (`2026-10-24-coagents-clone-placement-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer 2026-10-24: "finish the remaining nodes without stopping")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (every test this node touched is green; see "Added in green")
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-session-agents -p tddy-subagent-worktree -p tddy-daemon-rpc -p tddy-tools-cli -p tddy-changeset`; the Cypress spec) — verify 100% pass. Open: `tddy-session-lifecycle` has 16 pre-existing sandbox-bridge failures (Technical Debt), and the Cypress spec was not run (no web change)
- [ ] Carry to n8 `remote-codebase`: `open_owned` (peer-owned) and remote placement
- [x] Permission-answer turns announce no merge (`TODO(clone-placement)` in `clone_placement.rs`) — resolved here: a merge on a prompt-less turn is held and rides on the next prompt-bearing turn; the permission-answer turn itself still carries none (stated limit, Technical Debt)
- [x] Decide whether a coding agent's turn honours `syncWorktree: false` (Technical Debt) — resolved here: it does, and the skip is pinned by two tests (Decisions & Trade-offs)
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
- [ ] Linting and formatting (`cargo clippy -p <each> -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
