# Changeset: read a session's activity, transcript and pending question, paged

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — persisting the pending question touches the session metadata writer for two session families plus the claude-cli hook path; three or more crates; a paging contract on two tools
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

- `Page<T>{items, next_cursor, has_more, clamped_to?}` and `PAGE_DEFAULT` / `PAGE_MAX`.
- The `READ_SESSION_ACTIVITY` and `READ_SESSION_TRANSCRIPT` defs.
- `PendingQuestion` in `tddy-changeset::session_metadata`.
- Port methods and failing tests.

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

- [ ] `tddy-tool-engine`
- [ ] `tddy-daemon-rpc`
- [ ] `tddy-changeset`
- [ ] `tddy-session-activity`
- [ ] `tddy-agent-launch (claude-cli hook path)`
- [ ] `tddy-presenter (tool sessions)`

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

- [ ] **Integration** `packages/tddy-daemon-rpc/tests/session_follow_acceptance.rs` — `activity_pages_forward_from_a_cursor_without_repeats_or_gaps`: 250 activity rows read in pages of 100 give 100, 100, 50. Concatenated, they equal the file. The last page has `has_more = false`.
- [ ] **Integration** `same file` — `a_limit_above_the_maximum_is_clamped_and_says_so`: `limit` 10000 returns 200 items and `clamped_to: 200`.
- [ ] **Integration** `same file` — `there_is_no_unpaged_read`: No `limit` returns exactly the default (50) with `has_more`.
- [ ] **Integration** `same file` — `a_poll_from_the_last_cursor_returns_only_new_activity`: Append 3 rows: the next poll from the saved cursor returns exactly those 3.
- [ ] **Integration** `same file` — `a_tool_session_waiting_for_input_shows_its_question`: The presenter raises a select elicitation. `pending_question` equals its header, question and options; it is null after the answer.
- [ ] **Integration** `same file` — `a_claude_cli_session_waiting_for_input_shows_its_question`: The hook reports a permission request. `pending_question.question` equals its text.
- [ ] **Integration** `same file` — `transcript_pages_newest_first_like_the_activities_view`: The frames equal `GetAcpReplayPage`'s for the same cursor and size.
- [ ] **Integration** `same file` — `a_truncated_transcript_item_is_fetched_whole_by_tool_call_id`: Full `raw_output` equals `GetAcpToolCallDetail`'s.
- [ ] **Integration** `packages/tddy-daemon/tests/project_home_remote_targets_acceptance.rs` — `session_activity_is_read_from_another_host`: Peer rows returned.

Tests are written in the `fluent-tests` style (Given / When / Then). Multi-daemon tests use the
in-memory peer forwarder if it exists on the base, and the LiveKit testkit otherwise.

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
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
