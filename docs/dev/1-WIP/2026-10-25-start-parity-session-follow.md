# Changeset: read a session's activity, transcript and pending question, paged

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — the pending question has two writers in two processes (tddy-coder for tool sessions, the daemon's hook path for claude-cli) plus a proto field; six crates; a paging contract on two tools
**PR**: [#755](https://github.com/uppin/tddy-coder/pull/755)
**Stack**: `#start-parity` 6/9 — branch `feature/start-parity/session-follow`, base `n5` `start-options` (`feature/start-parity/start-options`)
**PRD**: [PRD-2026-10-25-start-parity-session-follow.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-session-follow.md)

## Initial Discovery

See [./2026-10-25-start-parity-session-follow-initial-discovery.md](./2026-10-25-start-parity-session-follow-initial-discovery.md).

## Responsibility

- Both tools, the shared paging type, and the forward pager over `agent-activity.jsonl`.
- `SessionMetadata.pending_question`: written by both session families, and cleared on answer.

## Boundaries

- Does not add user and assistant text to claude-cli transcripts. That history does not exist
  server-side; recorded as a TODO.
- Does not change `StreamAcpReplay` or the UI.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` `remote-targets` (`feature/start-parity/remote-targets`) | the delegated home caller, the `daemon_instance_id` target argument, and the forward of a whole project-home tool call to the target's host | its tool takes `daemon_instance_id` and is forwarded by n1's route; one cross-host test drives it | add a forward, a delegated-caller check, or a per-tool peer call |

## Draft PR contract

The **first push of this PR**, not its deliverable — this PR goes on to implement all of it:

- **The pending question gets its own file, not a `SessionMetadata` field.**
  - `tddy-changeset::pending_question`: `PENDING_QUESTION_FILE` (`pending-question.json` in the
    session dir), `PendingQuestion {kind, header, question, options, allow_other}`, and
    `write_pending_question`, `clear_pending_question` and `read_pending_question`, all
    `TODO(session-follow)`.
  - Adding a field to `SessionMetadata` would have touched about 69 struct literals in 45 files.
  - The existing `pending_elicitation` flag is unchanged.
- **claude-cli writer.**
  - `activity.proto`: `ReportSessionStatusRequest.message = 5`, the hook event's own message.
  - The 17 literals that list every field (in tddy-tools, tddy-daemon tests, tddy-session-lifecycle
    and tddy-telegram-control) now set an empty `message`, so behaviour is unchanged.
  - `tddy_tools::session_hook::session_status_request(args, stdin_json)`, the request a hook event
    becomes, is `TODO(session-follow)`.
- **tool-session writer.** `tddy_coder::session_participant::pending_question::{pending_question_for_mode,
  record_pending_question}` are `TODO(session-follow)`. The tool session's own process writes the
  file from the presenter's `AppMode` (select, multi-select, text).
- **Reader.** `tddy-tool-engine::session_follow`:
  - `READ_SESSION_ACTIVITY` and `READ_SESSION_TRANSCRIPT`;
  - `FOLLOW_PAGE_DEFAULT` = 50, `FOLLOW_PAGE_MAX` = 200 and `TRANSCRIPT_ITEM_MAX_BYTES` = 4096;
  - `FollowPage<T> {items, next_cursor, has_more, clamped_to}`, `ActivityPage` (the page plus
    `pending_question`) and `TranscriptItem`;
  - `page_activity`, `truncate_transcript_text` and `session_follow_tool_defs`, all
    `TODO(session-follow)`.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — its tests exercise `n1` `remote-targets` (`feature/start-parity/remote-targets`)'s behaviour, which must be green first
**Concurrent with:** `n3` `worktree-list` (`feature/start-parity/worktree-list`), `n4` `async-start` (`feature/start-parity/async-start`), `n5` `start-options` (`feature/start-parity/start-options`), `n7` `terminate` (`feature/start-parity/terminate`)
**Blocks:** nothing

Real dependency edges (the branch line is only their linear flattening):

    n1 → n3, n5, n6, n7      n2 → n4, n9      n3 → n8      n4 → n9

Line: `n1 remote-targets · n2 start-tool | n3 worktree-list · n4 async-start · n5 start-options ·
n6 session-follow · n7 terminate | n8 disk-refresh · n9 spawn-conversation`, waves `1 1 2 2 2 2 2 3 3`.

## Successor PRs

`n7` `terminate` (`feature/start-parity/terminate`), `n8` `disk-refresh` (`feature/start-parity/disk-refresh`), `n9` `spawn-conversation` (`feature/start-parity/spawn-conversation`)

## Affected Packages

- [ ] `tddy-tool-engine` — the two tools, paging, truncation
- [ ] `tddy-daemon-rpc` — serving them (local, forwarded, against the replay RPCs)
- [ ] `tddy-changeset` — the pending-question store
- [ ] `tddy-service` — `ReportSessionStatusRequest.message`
- [ ] `tddy-tools` — the claude-cli hook sends the message
- [ ] `tddy-session-lifecycle` — `ReportSessionStatus` records and clears the question
- [ ] `tddy-coder` — a tool session records its presenter's question
- [ ] `tddy-daemon` (tests), `tddy-telegram-control` (tests) — an empty `message` in exhaustive literals only

## Related Feature Documentation

- PRD: [PRD-2026-10-25-start-parity-session-follow.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-session-follow.md)
- [docs/ft/daemon/project-concept.md § Project-home tools](../../ft/daemon/project-concept.md)

## Summary

An orchestrator can see a session's status but not what it is doing or asking. A child stuck in
WaitingForInput is a dead end. This node adds **`ReadSessionActivity`**, with the open pending
question, and **`ReadSessionTranscript`**. Both are paged under one explicit contract and work on
any host.

## Scope

- [ ] An orchestrator can read a session's activity page by page and poll for what is new.
- [ ] It can read the session's transcript page by page, and fetch any truncated item whole.
- [ ] It can see the question a session is waiting on.
- [ ] No call returns an unbounded amount of data.

## Technical Changes

### State A

- **Activity.** `agent-activity.jsonl` holds `AgentActivityRecord`s
  (`tddy-changeset/src/agent_activity.rs`): tool calls with status, result and error, plus
  `activity_seq`. `read_agent_activity` caps the read at 500 calls.
- **Transcript.** The UI's Activities view pages it through `ActivityService.GetAcpReplayPage
  {before_seq, page_size}` and fetches a whole tool call with `GetAcpToolCallDetail`. Both
  forward to a peer.
- **Source.** `read_session_transcript` (`tddy-service/src/acp_replay.rs:99`) merges
  `acp-transcript.jsonl` (tool sessions only) with the activity rows. **A claude-cli history holds
  tool calls only.**
- **The pending question is not persisted.** The UI reads it live from the ACP `requestPermission`
  (`useAcpSession.ts:381-406`). The server keeps only the bool
  `SessionMetadata.pending_elicitation`, which tool sessions set and claude-cli leaves empty
  (`hooks_and_urls.rs:72`).

### State B

- **Paging contract** (developer requirement), the same for both tools:
  - in: `cursor` (empty = the start) and `limit` (default 50, hard maximum 200; a larger value is
    clamped, and the response says so);
  - out: `items`, `next_cursor` and `has_more`;
  - there is no unpaged call.
- `ReadSessionActivity {session_id, daemon_instance_id?, cursor?, limit?, kinds?}` pages forward by
  `activity_seq` ("everything since X"), so a poll resumes where the last one stopped. The response
  carries `pending_question: {kind, header, question, options, allow_other} | null`.
- `ReadSessionTranscript {session_id, daemon_instance_id?, cursor?, limit?, direction:
  newest_first|from_cursor}` returns the frames `GetAcpReplayPage` returns, each item cut at 4 KiB
  with `truncated: true`. `ReadSessionTranscript {…, tool_call_id}` returns one whole item through
  `GetAcpToolCallDetail`.
- **The pending question is persisted** where the flag is set. Tool sessions store the
  elicitation's text next to `pending_elicitation`. claude-cli sessions store it from the
  permission or notification hook that already reports WaitingForInput. It is cleared when
  answered.
- Both tools are limited to the caller's project, and reach another host through `remote-targets`.

## Testing Plan

### Acceptance Tests

- [ ] **Unit** `packages/tddy-changeset/tests/pending_question.rs`:
  - `a_written_question_is_read_back_whole`
  - `a_new_question_replaces_the_last`
  - `a_cleared_session_waits_on_nothing_and_clearing_twice_is_fine`
  - `a_session_that_never_waited_has_no_question`
- [ ] **Unit** `packages/tddy-tools/tests/session_hook_pending_question.rs`:
  - `a_permission_prompt_reports_waiting_with_the_agents_question`
  - `a_status_that_is_not_waiting_carries_no_message`
  - `an_event_that_reports_no_status_sends_nothing`
- [ ] **Unit** `tddy-session-lifecycle` `report_session_status_unit_tests.rs`:
  - `waiting_for_input_records_the_agents_message_as_the_pending_question`
  - `the_next_status_clears_the_pending_question`
- [ ] **Unit** `packages/tddy-coder/tests/pending_question_from_mode.rs`:
  - `a_select_mode_is_a_select_question_with_its_options`
  - `a_multi_select_and_a_text_prompt_are_questions_too`
  - `a_running_workflow_waits_on_nothing`
  - `the_question_is_recorded_while_asked_and_cleared_after`
- [ ] **Unit** `packages/tddy-tool-engine/tests/session_follow_paging.rs`:
  - `activity_pages_forward_without_repeats_or_gaps`
  - `a_poll_from_the_last_cursor_returns_only_what_is_new`
  - `there_is_no_unbounded_read`
  - `a_status_filter_keeps_only_those_calls_and_still_pages_by_sequence`
  - `a_cursor_that_is_not_a_position_is_refused`
  - `a_long_transcript_item_is_cut_on_a_character_boundary_and_marked`
  - `a_short_transcript_item_is_returned_whole`
  - `both_tools_state_the_paging_contract_in_their_schemas`
- [ ] **Integration** `packages/tddy-daemon-rpc/tests/session_follow_acceptance.rs` (two hosts):
  - `activity_is_read_a_page_at_a_time_without_repeats_or_gaps` (250 calls → 100, 100, 50)
  - `polling_from_the_last_cursor_returns_only_new_activity`
  - `a_limit_above_the_maximum_is_clamped_and_says_so`
  - `the_question_a_session_waits_on_comes_with_its_activity`
  - `a_session_not_waiting_has_no_question`
  - `the_transcript_is_the_page_the_activities_view_reads` (against `GetAcpReplayPage`)
  - `a_tool_call_is_read_whole_by_its_id` (against `GetAcpToolCallDetail`)
  - `a_session_of_another_project_is_refused`
  - `activity_is_read_from_a_session_on_another_host`

**Activity filter.** The brief's "filter by kind" is `statuses` (`running`, `completed`, `error`):
`agent-activity.jsonl` records only tool calls, so status is the kind there is. Status
transitions and questions are not activity rows. The pending question comes back with every
activity page instead.

Tests are written in the `fluent-tests` style (Given / When / Then). 

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Activity pages forward and the transcript pages newest-first,** as asked. The activity cursor
  is `activity_seq`; the transcript cursor is the replay `seq`.
- **The pending question is persisted, not inferred** (developer, 2026-10-25). This is new
  behaviour in two writers. Score 5 because it widens the node beyond read tools.

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |

## TODO

- [x] Record initial discovery (`2026-10-25-start-parity-session-follow-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
