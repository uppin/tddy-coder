# Changeset — activities-tail-first-autoscroll

**Created:** 2026-08-02
**Packages:** `tddy-service`, `tddy-daemon`, `tddy-web`
**PRD:** [PRD-2026-08-02-activities-tail-first-autoscroll.md](../../ft/web/1-WIP/PRD-2026-08-02-activities-tail-first-autoscroll.md)
**Amends:** `docs/ft/web/agent-activity-pane.md`, `docs/ft/web/inactive-session-activities.md`

## Problem

The recorded ACP transcript opens at its **oldest** entry, never follows live activity, and
transfers the entire persisted history to render a screenful of its end. `AgentChatView`
(`packages/tddy-web/src/components/chat/AgentChat.tsx:195`) renders a bare `overflow-y-auto` div
with no scroll management, and `StreamAcpReplay` has no notion of a page or a cursor
(`packages/tddy-service/proto/connection.proto:1028`).

## Technical delta

### `tddy-service`

| Change | Where |
|---|---|
| `StreamMode.TAIL_THEN_LIVE = 3` | `proto/connection.proto` |
| `StreamAcpReplayRequest.page_size` (field 5) | `proto/connection.proto` |
| `AcpReplayFrame.seq` (field 3) — absolute 0-based transcript position | `proto/connection.proto` |
| `rpc GetAcpReplayPage` + request/response messages | `proto/connection.proto` |
| `DEFAULT_REPLAY_PAGE_SIZE`, `TranscriptPage`, `tail_page`, `page_before` | `src/acp_replay.rs` |
| Regenerated TS bindings | `packages/tddy-web/src/gen/connection_pb.ts` (`bun run generate`) |

Paging operates on the **resolved** transcript (`read_session_transcript`), so a `seq` means the
same thing to the pager, the counter and the snapshot replay.

### `tddy-daemon`

| Change | Where |
|---|---|
| `stream_acp_replay` serves `TAIL_THEN_LIVE` (tail page + `seq`-stamped live tail) | `src/connection_service.rs` |
| `seq` stamped on `SNAPSHOT_THEN_LIVE` frames too (positions stay consistent across modes) | `src/connection_service.rs` |
| `get_acp_replay_page` handler — routing/auth/`strip_tool_body` mirroring `get_acp_tool_call_detail`, including peer-forwarding | `src/connection_service.rs` |
| Tonic adapter wiring for the new RPC | `src/connection_tonic_adapter.rs` |

### `tddy-coder`

`session_participant/mod.rs` also implements `stream_acp_replay`. It serves the same modes from the
same `tddy-service` helpers; `TAIL_THEN_LIVE` and `GetAcpReplayPage` are added there in lockstep so
a session-scoped (LiveKit) client is not silently downgraded.

### `tddy-web`

| Change | Where |
|---|---|
| `projectReplayFrames` / `createReplayProjector` — the frame→`ChatMessage` projection extracted out of the stream effect so a whole page can be projected at once, with page-scoped keys | `src/components/chat/acpReplayProjection.ts` (new) |
| `useAcpReplay` opens `TAIL_THEN_LIVE`; new `atOldest` / `loadingOlder` / `loadOlder()` | `src/components/chat/useAcpReplay.ts` |
| `oldestSeq`, `atOldest`, `loadingOlder`, `prependMessages()`, `setLoadingOlder()` | `src/components/sessions/agentActivityRegistry.ts` |
| `isPinnedToBottom`, `isNearTop`, `scrollTopAfterPrepend` — pure viewport arithmetic | `src/lib/scrollFollow.ts` (new) |
| Read-only `AgentChatView`: sticky-bottom follow, jump-to-latest, scroll-to-top page trigger, hidden scroll-state mirror, **inline flex/overflow layout contract** on the root + message list | `src/components/chat/AgentChat.tsx` |
| Pass `onLoadOlder` / `hasOlder` / `loadingOlder` through | `src/components/sessions/SessionActivitiesPane.tsx`, `src/components/sessions/AgentActivityOverlay.tsx` |
| `agentChatJumpToLatest`, `agentChatOlderLoading`, `agentChatScrollState` | `cypress/support/testIds.ts` |
| `aTailReplayBackend` (tail mode + `GetAcpReplayPage` + `pushLive`) | `cypress/support/rpc/acpReplay.ts` |
| Scroll/follow helpers on the page object | `cypress/support/pages/agentChatPage.ts` |
| `test:unit` glob extended to cover `src/components/chat` | `package.json` |

## Acceptance tests

`packages/tddy-web/cypress/component/ActivitiesTailScrollAcceptance.cy.tsx`

1. opens the recorded transcript scrolled to its newest entry
2. requests only the newest page of a long transcript when the view opens
3. scrolls a live activity frame into view while pinned to the newest entry
4. leaves the read position untouched when a live frame arrives after scrolling up
5. counts the entries that arrived while the reader was scrolled away
6. returns to the newest entry and clears the counter when jump-to-latest is clicked
7. re-attaches to the newest entry when the reader scrolls back to the bottom
8. fetches the page before the cursor when the top of the loaded range is reached
9. prepends the older page above the loaded range without moving the read position
10. stops fetching older pages once the transcript head is reached
11. leaves the loaded range intact and retryable when an older page fetch fails
12. shows the same tail-first transcript in the agent activity overlay

## Unit / integration tests

**`tddy-service` — `src/acp_replay.rs`**

1. `tail_page_returns_the_newest_frames_stamped_with_the_seq_of_its_first_frame`
2. `a_transcript_shorter_than_the_page_size_tails_to_the_whole_transcript_at_its_head`
3. `tail_page_of_an_empty_transcript_is_empty_and_at_the_head`
4. `a_page_size_of_zero_falls_back_to_the_default_page_size`
5. `page_before_returns_the_frames_immediately_older_than_the_cursor`
6. `page_before_the_head_returns_nothing_and_reports_at_oldest`
7. `page_before_reports_at_oldest_when_the_page_reaches_the_first_frame`
8. `page_before_a_cursor_past_the_transcript_end_returns_the_newest_page`

**`tddy-daemon` — `src/connection_service.rs`**

9. `stream_acp_replay_in_tail_mode_replays_only_the_newest_page`
10. `stream_acp_replay_tail_frames_carry_their_absolute_transcript_position`
11. `get_acp_replay_page_serves_the_frames_before_the_cursor`
12. `get_acp_replay_page_strips_tool_bodies_like_the_replay_stream_does`

**`tddy-web` — bun unit**

13–15. `src/components/chat/acpReplayProjection.test.ts` — page projection, key distinctness across
pages, tool-call coalescing within a page
16–19. `src/components/sessions/agentActivityRegistry.test.ts` — prepend ordering, cursor advance,
head marking, duplicate-page rejection
20–22. `src/lib/scrollFollow.test.ts` — pinned threshold, detached threshold, prepend anchor

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Write failing acceptance tests (12)
- [ ] Write failing unit/integration tests (22)
- [ ] `/green` — proto + `tddy-service` paging
- [ ] `/green` — `tddy-daemon` + `tddy-coder` handlers
- [ ] `/green` — web projection, registry, hook
- [ ] `/green` — `AgentChatView` scroll behavior
- [ ] Update context docs
- [ ] `/pr-wrap`

## Risks

- **Mode default.** `SNAPSHOT_THEN_LIVE` is the proto3 zero value and stays the default; every
  existing caller is unaffected. Only the two read-only transcript surfaces opt into tail mode.
- **`seq` on live frames.** The daemon must continue numbering from the snapshot length, or a live
  frame's position disagrees with what a re-read would give and the cursor drifts.
- **Scroll assertions in Cypress.** Bound to the hidden `agent-chat-scroll-state` mirror rather than
  to layout arithmetic, so a style change cannot silently turn a scroll test green.
- **Two hosts implement `StreamAcpReplay`** (`tddy-daemon` and `tddy-coder`'s session participant).
  They must gain the mode together or a LiveKit-routed session opens head-first.
