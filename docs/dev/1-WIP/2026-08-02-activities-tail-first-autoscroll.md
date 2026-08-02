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

**28 written, against 22 planned.** The Rust counts are exactly as planned; the six extra are all in
the web unit layer, where the added cases are boundary rows of pure functions — noted inline.

**`tddy-service` — `src/acp_replay.rs` (8)**

1. `tail_page_returns_the_newest_frames_stamped_with_the_seq_of_its_first_frame`
2. `a_transcript_shorter_than_the_page_size_tails_to_the_whole_transcript_at_its_head`
3. `tail_page_of_an_empty_transcript_is_empty_and_at_the_head`
4. `a_page_size_of_zero_falls_back_to_the_default_page_size`
5. `page_before_returns_the_frames_immediately_older_than_the_cursor`
6. `page_before_the_head_returns_nothing_and_reports_at_oldest`
7. `page_before_reports_at_oldest_when_the_page_reaches_the_first_frame`
8. `page_before_a_cursor_past_the_transcript_end_returns_the_newest_page`

**`tddy-daemon` — `src/connection_service.rs` (4)**

9. `stream_acp_replay_in_tail_mode_replays_only_the_newest_page`
10. `stream_acp_replay_tail_frames_carry_their_absolute_transcript_position`
11. `get_acp_replay_page_serves_the_frames_before_the_cursor`
12. `get_acp_replay_page_strips_tool_bodies_like_the_replay_stream_does`

**`tddy-web` — `src/components/chat/acpReplayProjection.test.ts` (4)**

13. `projects a page of frames into transcript entries in recorded order`
14. `coalesces a tool call's running then completed frames within one page`
15. `gives two pages disjoint entry keys`
16. `accumulates the live tail onto the entries the page already produced` — *added*: the plan named
    `createReplayProjector` in the delta but left the incremental path uncovered, and auto-follow
    rides on it

**`tddy-web` — `src/components/sessions/agentActivityRegistry.test.ts` (4)**

17. `prepends an older page above the loaded entries`
18. `advances the reverse cursor to the prepended page's first seq`
19. `marks the range closed when the prepended page reaches the head`
20. `ignores a page whose first seq is not older than the current cursor`

**`tddy-web` — `src/lib/scrollFollow.test.ts` (8)**

21. `treats a viewport resting at the very bottom as following the newest entry`
22. `treats a viewport within the pin threshold of the bottom as still following`
23. `treats a viewport scrolled past the pin threshold as detached`
24. `treats content shorter than the viewport as following, since there is nothing to scroll` —
    *added*: a transcript that fits shows its newest entry by definition, and the arithmetic goes
    negative there
25. `treats a viewport within the paging threshold of the top as reaching the loaded range's start` —
    *added*: the plan left `isNearTop` untested, and AC8 rides entirely on it
26. `treats a viewport below the paging threshold as still inside the loaded range` — *added*
27. `keeps the read position by adding the prepended content's height to the offset`
28. `leaves the offset alone when the prepended page added no height` — *added*: the compensating
    scroll must not become an unrequested nudge

## Findings from the red phase

- **The inline layout contract must cover `AgentActivityOverlay`'s panel too**, not only the
  transcript root and message list. Its `flex h-full w-full flex-col` is equally inert in the
  stylesheet-less harness, so the transcript still has no bounded height inside it. PRD § Inline
  layout contract amended. Confirmed empirically: the follow/paging specs fail with
  `cy.scrollTo() failed because this element is not scrollable`.
- **`agentActivityRegistry` needs a third new writer, `setOldestSeq(sessionId, seq, atOldest)`** —
  the delta table named only `prependMessages` and `setLoadingOlder`, but the *tail page* also has to
  record where the loaded range starts, and folding that into `setMessages` would force a signature
  change on the live-tail path that does not move the cursor.
- **Two `TODO(/green)` casts in `cypress/support/rpc/acpReplay.ts`**, both discharged by the proto
  step: `AcpReplayFrame.seq` (`create()` drops an init key the schema does not carry) and the
  `getAcpReplayPage` handler (the router ignores a handler for an undeclared method).

## Deliberate cross-cutting changes

**`LIVE_ONLY` now reads the resolved transcript.** The mode previously did no transcript read at all,
which left it no base position to number a live frame from — a frame would have claimed `seq = 0`
while the transcript held hundreds of entries. Since `seq` is documented as carried by *every*
transcript frame and only `COUNT_THEN_LIVE` is exempt, the read is now unconditional. Neither the PRD
nor this changeset authorized it, so it is recorded here rather than left as an implementation
detail: it adds one local file read per `LIVE_ONLY` subscription. The cost profile is unchanged in
kind — `COUNT_THEN_LIVE`, the mode explicitly billed as cheap, already pays exactly this read.

**The live tail dedupes positions by `tool_call_id`.** A tool call broadcasts twice (its `running`
then its terminal record) but `read_session_transcript` coalesces both into one entry, so a naive
increment-per-delivered-record drifts one position ahead per completed call and gives a refinement a
position of its own. Both hosts now map `tool_call_id → absolute seq`, pre-seeded from the resolved
transcript, so a refinement carries the position of the entry it refines. This required making
`tddy_service::acp_replay::tool_call_id_of` public — `tool_call_ids()` beside it returns only a set
and cannot give positions. Without it the PRD's § Server contract invariant ("a frame appended while
the stream is open carries the position it would have had on a later re-read") is simply false.

One divergence remains and is now stated in the PRD rather than left implied: coalescing keeps a call
at its *last* record's slot, so a re-read orders tool rows by completion time while the live tail can
only number them by first-record time. Two interleaved calls therefore get swapped positions. Closing
it would mean renumbering frames already sent, which the wire cannot express. It does not reach the
client — the cursor comes off the tail page's first frame, and live frames merge by `tool_call_id`.

## Validation Results

Four analyses over the branch diff: change risk, test quality, production readiness, clean code. The
gates here are `cargo test` / `clippy` for the three Rust packages and `bun test:unit` + the Cypress
component suite + `vite build` for the web. `tsc --noEmit` is not a gate and is not consulted.

### Defects found and fixed

1. **A transcript kept one session's scroll state across a switch.** `AgentChatView` is rendered by
   `SessionActivitiesPane` and `AgentActivityOverlay` without a key, and `SessionMainPane` re-renders
   the pane rather than remounting it. Switching to an **already-visited** session (where
   `hasActivity` is true across both renders, so the view is never unmounted) carried the previous
   session's `pinned` flag, detach anchor and measured height into the new one: it opened detached
   instead of at its newest entry, compensated a phantom prepend against the wrong height, and —
   since entry keys are scoped by position, which two sessions can share — could show a
   jump-to-latest count belonging to the other session. Exactly the case PRD § Edge cases documents
   as "a cached transcript on switch-back". Fixed with `key={sessionId}` on both surfaces; pinned by
   a new acceptance test, verified by reverting the key and watching only that test fail.
2. **Four acceptance tests recorded "live" frames before the component mounted.** `replay.pushLive`
   is a plain call, so it ran during test-body evaluation, ahead of `cy.mount` in the command queue;
   the frames arrived as part of the initial page load and were never live. Three failed outright,
   and one — "scrolls a live activity frame into view while pinned" — **passed vacuously**, satisfied
   by the tail-first open itself. Fixed with a `recordLive()` helper that enqueues via `cy.then`.
3. **Two of those tests also raced the detach.** `scrollTo` completing is not the component's scroll
   handler having run, so the push landed while the transcript still believed it was pinned and the
   arriving frame re-followed. Fixed by asserting `expectDetachedFromNewest()` first — synchronising
   on the signal rather than on a delay.
4. **The older-page indicator had only negative coverage.** Both references to
   `agent-chat-older-loading` asserted `not.exist`, so deleting the element outright would have left
   the suite green. The fake resolved `GetAcpReplayPage` instantly, making the in-flight window
   unobservable. Added `aTailReplayWithHeldPages` and a test that pins the indicator showing and
   then clearing.
5. **`LIVE_ONLY`'s new `seq` base was untested.** Two existing tests use the mode but neither asserts
   a position, so a regression to `seq = 0` would have gone unnoticed and the added transcript read
   would have been pure cost. Pinned by
   `stream_acp_replay_live_only_frames_are_numbered_from_the_recorded_transcript`.

### Production readiness

No new `TODO`/`FIXME`/`HACK` markers (the branch diff adds zero marker lines); both
`TODO(acp-replay)` markers were removed by fixing the drift they recorded, and the two `TODO(/green)`
casts in the test support were discharged by the proto step. No `println!`/`eprintln!`/`dbg!`, no
`console.log` (three `console.debug` calls sit on genuine error paths, matching the file's
convention), no mock code outside `cypress/support/`, no dead exports, no silent fallbacks.

### Clean code

Score **C** on entry: two functions over the 60-line threshold (`useTailFollow` at 111,
`createReplayProjector` at 81) and one file over 500 (`AgentChat.tsx` at 569, grown from 336). No
magic values, no duplication, consistent naming, nesting within bounds. All three were split at the
developer's direction rather than deferred — see § Module layout.

## Known gaps

- **A switch between sessions is not covered by the paging/follow specs.** Validation found that
  `AgentChatView` kept one session's scroll state across a switch (see § Validation Results); the fix
  keys the transcript per session and is pinned by a new acceptance test. No *other* multi-session
  interaction is covered.
- **`tddy-coder`'s live tail subscribes to the presenter broadcast before reading the snapshot**
  (`session_participant/mod.rs:582`, a deliberate gap-avoidance choice), so an event landing in that
  window is both replayed and delivered live. Pre-existing duplicate delivery that `seq` now makes
  visible; out of scope here.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write failing acceptance tests (12)
- [x] Write failing unit/integration tests (28)
- [x] `/green` — proto + `tddy-service` paging
- [x] `/green` — `tddy-daemon` + `tddy-coder` handlers
- [x] `/green` — web projection, registry, hook
- [x] `/green` — `AgentChatView` scroll behavior
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
