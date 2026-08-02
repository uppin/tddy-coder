# Activities — tail-first, auto-scrolling transcript with a reverse cursor

> **Status:** Amendment (State A → State B). Not yet folded into the feature docs.
> **Product area:** `docs/ft/web`
> **Created:** 2026-08-02
>
> **Amends — ALL affected feature documents:**
> - [agent-activity-pane.md](../agent-activity-pane.md) — §"Read-only ACP transcript", §"Server-side
>   replay", §"Persisted, lazily-counted activity" (the `StreamAcpReplay` two-phase contract and the
>   `AgentChatView` transcript body the overlay renders).
> - [inactive-session-activities.md](../inactive-session-activities.md) — §"Activities view" (the
>   eager snapshot pull and the transcript that fills a dormant session's main pane).
>
> **Related, not amended:**
> - [terminal-replay-lazy-scroll.md](../terminal-replay-lazy-scroll.md) — the same *shape* of problem
>   for the terminal (open at the tip, page older history backwards). Its solution is
>   ghostty-specific (two overlaid terminals, forward-fill) because ghostty-web has no prepend API.
>   The transcript is a React list and **can** prepend, so this amendment pages backwards directly.
>   The `at_end` / cursor vocabulary is deliberately borrowed.
> - [url-state-routing.md](../url-state-routing.md) — scroll position stays out of the URL.

## State A — what the transcript does today

`SessionActivitiesPane` and `AgentActivityOverlay` both render the recorded ACP transcript through
the same `useAcpReplay` hook and the same read-only `AgentChatView`. That path has three properties
the operator experiences as defects:

1. **It opens at the beginning.** `AgentChatView` renders the entries into a plain
   `overflow-y-auto` div with **no scroll management of any kind**. The viewport starts at the top,
   which for a recorded session is its *oldest* entry — the least interesting one. Reading what the
   agent last did means scrolling to the bottom by hand, every time the pane is opened and every
   time the operator switches sessions and back.
2. **It does not follow live activity.** Frames that arrive while the pane is open append below the
   fold. On a live session the newest tool call is invisible until the operator scrolls; the pane
   silently stops reflecting what the agent is doing.
3. **It transfers the whole history to show the end of it.** `StreamAcpReplay` in
   `SNAPSHOT_THEN_LIVE` replays every persisted frame, head first. A session with a multi-megabyte
   `agent-activity.jsonl` pays for its entire recorded history to render a screenful of it.

## State B — what it does after this change

The transcript behaves like a chat log: **it opens at the end, it follows the end, and older
history arrives on demand as the operator scrolls back toward the beginning.**

1. **Tail-first open.** Opening the Activities view (or the overlay) lands the operator on the
   **newest** entry, with the page of entries immediately before it already rendered above.
2. **Auto-follow in real time.** While the viewport is pinned to the bottom, each arriving frame
   scrolls into view. The pane keeps showing the agent's latest action without a gesture.
3. **Reading history is never interrupted.** Scrolling up **detaches** follow mode. Frames still
   arrive and still render; the viewport does not move. A **jump-to-latest** affordance appears,
   labelled with how many entries arrived while detached, and re-attaches on click. Scrolling back
   to the bottom re-attaches too.
4. **A reverse cursor pages backwards.** The wire serves the newest page first and hands back the
   absolute position of that page's oldest frame. Reaching the top of the loaded range fetches the
   page before it and prepends it **without moving the read position**. This repeats until the
   transcript head is reached, after which nothing more is fetched.

## Scope

### In scope

- `ConnectionService.StreamAcpReplay` gains a `TAIL_THEN_LIVE` stream mode and a `page_size`.
- A new unary `ConnectionService.GetAcpReplayPage` — the reverse cursor.
- `AcpReplayFrame` gains `seq`: the frame's absolute 0-based position in the resolved transcript.
- `tddy-service::acp_replay` gains the two pure paging functions the daemon serves from.
- `tddy-daemon` serves both, reusing the existing routing, auth and `strip_tool_body` seam.
- `useAcpReplay` opens in tail mode, tracks the reverse cursor, and exposes `loadOlder`.
- `agentActivityRegistry` holds the loaded range's cursor and prepends older pages.
- `AgentChatView` (read-only mode) owns sticky-bottom auto-follow, the jump-to-latest affordance,
  and the scroll-to-top trigger for the older page.
- Both consuming surfaces — `SessionActivitiesPane` and `AgentActivityOverlay`.

### Out of scope

- The **live**, interactive `AgentChat` / `WorkflowChatScreen` transcripts (`readOnly = false`).
  They have the same missing-scroll problem; fixing them is a separate change with its own
  composer-focus and elicitation interactions to reason about. Recorded in `docs/dev/TODO.md`.
- `StreamSessionActivity` (the legacy tool-call-record stream) — unchanged, unpaged.
- Persisting the scroll position across a session switch or a page reload. Switching away and back
  re-opens at the tail, which is the documented default.
- Cross-daemon forwarding of `StreamAcpReplay`, which is already blocked on an unrelated keepalive
  gap (`TODO(acp-replay)` in `connection_service.rs`). `GetAcpReplayPage` is unary and **does**
  forward, exactly as `GetAcpToolCallDetail` already does.
- Virtualized rendering of the loaded range. Paging bounds what is fetched; it does not bound what
  is mounted once the operator has paged a long way back.

## Behavior

### Opening

- The transcript opens in `TAIL_THEN_LIVE` with a page size of **100** frames. The daemon replays
  the last 100 persisted frames in recorded order (oldest-first *within* the page), each stamped
  with its absolute `seq`, then tails live.
- The first frame delivered carries the page's `first_seq`. That value **is** the reverse cursor.
  `first_seq == 0` means the page already reaches the transcript head, so there is nothing older.
- The viewport lands on the newest entry, not on `first_seq`.
- A session that recorded nothing delivers no transcript frames; the existing empty state
  (gated on the count feed, unchanged) still owns that case.

### Following

- The transcript is **pinned** while its viewport is within 32 px of the bottom. Pinned is the
  state it opens in.
- While pinned, an arriving frame scrolls the newest entry into view.
- Scrolling up past the threshold detaches. Detached, arriving frames render but the scroll offset
  is untouched — including the offset shifting that a prepended older page would otherwise cause.
- Detached, a **jump-to-latest** affordance shows the number of entries that arrived since
  detaching. Clicking it scrolls to the newest entry and re-pins. Scrolling back to within the
  threshold also re-pins, and clears the counter.
- The counter counts *entries*, not frames: a tool call refined from `running` to `completed`
  coalesces into the entry it already had and does not increment.

### Paging backwards

- Reaching the top of the loaded range (within 64 px) fetches the page before the cursor:
  `GetAcpReplayPage(before_seq = <current cursor>, page_size = 100)`.
- Exactly one page fetch is in flight at a time. A scroll that re-crosses the threshold while a
  fetch is running does not issue a second one.
- The resolved page is **prepended** to the loaded range and the cursor moves to the new
  `first_seq`. The read position is preserved: the entry the operator was looking at stays under
  the same pixel.
- A response with `at_oldest` closes the range. No further fetch is issued no matter how the
  operator scrolls, and the loading indicator does not reappear.
- A failed page fetch leaves the loaded range exactly as it was and clears the in-flight flag, so
  a later scroll retries. Nothing fabricated, no partial page.

### Interaction with the existing count feed

Unchanged. `COUNT_THEN_LIVE` still drives `hasActivity`, `countLoaded` and the unread badge, and
still costs no transcript payload. The unread **badge** (activity since the overlay was last
opened) and the jump-to-latest **counter** (entries since the viewport detached) answer different
questions and are tracked separately.

## Wire contract

```proto
enum StreamMode {
  SNAPSHOT_THEN_LIVE = 0;
  LIVE_ONLY          = 1;
  COUNT_THEN_LIVE    = 2;
  // Tail-first (StreamAcpReplay only): replay only the NEWEST `page_size` persisted frames — in
  // recorded order, oldest-first within the page — then tail live. Older frames are reached by
  // paging backwards with GetAcpReplayPage from the first delivered frame's `seq`.
  TAIL_THEN_LIVE     = 3;
}

message StreamAcpReplayRequest {
  string     session_token      = 1;
  string     session_id         = 2;
  string     daemon_instance_id = 3;
  StreamMode mode               = 4;
  // TAIL_THEN_LIVE only: how many newest frames to replay. 0 ⇒ the server's default page size.
  uint32     page_size          = 5;
}

message AcpReplayFrame {
  bytes  acp_agent_message = 1;
  uint64 activity_count    = 2;
  // The frame's absolute 0-based position in the session's resolved transcript, oldest = 0. Carried
  // by every transcript frame (snapshot, tail page, and live tail). On the FIRST frame of a tail
  // page it doubles as the reverse cursor: `seq == 0` means the page reaches the transcript head.
  // COUNT_THEN_LIVE frames carry no transcript payload and leave it 0.
  uint64 seq               = 3;
}

// One page of transcript frames strictly OLDER than `before_seq` — the reverse cursor behind the
// Activities transcript's scroll-up. Mirrors GetAcpToolCallDetail's routing (peer-forwarding on
// daemon_instance_id) and StreamAcpReplay's auth.
message GetAcpReplayPageRequest {
  string session_token      = 1;
  string session_id         = 2;
  string daemon_instance_id = 3;
  uint64 before_seq         = 4;   // exclusive upper bound; frames with seq < before_seq
  uint32 page_size          = 5;   // 0 ⇒ the server's default page size
}

message GetAcpReplayPageResponse {
  // Protobuf-encoded `tddy.acp.v1.AcpAgentMessage` frames, oldest-first, tool bodies stripped —
  // byte-identical in shape to AcpReplayFrame.acp_agent_message.
  repeated bytes frames    = 1;
  uint64          first_seq = 2;   // absolute seq of frames[0]; meaningless when `frames` is empty
  bool            at_oldest = 3;   // true when this page reaches the transcript head
}
```

`at_oldest` is an explicit field rather than `first_seq == 0` because an **empty** page (the cursor
is already at the head) would otherwise be indistinguishable from a one-frame page at the head.

## Server contract

Two pure functions in `tddy-service::acp_replay`, over the already-resolved transcript
(`read_session_transcript` — both stores merged, coalesced, time-ordered), so paging and counting
agree about what a frame *is*:

```rust
pub const DEFAULT_REPLAY_PAGE_SIZE: usize = 100;

pub struct TranscriptPage<'a> {
    pub first_seq: u64,
    pub frames: &'a [AcpAgentMessage],
    pub at_oldest: bool,
}

/// The newest `page_size` frames, oldest-first within the page.
pub fn tail_page(frames: &[AcpAgentMessage], page_size: usize) -> TranscriptPage<'_>;

/// The page of frames immediately older than `before_seq` (exclusive), oldest-first.
pub fn page_before(frames: &[AcpAgentMessage], before_seq: u64, page_size: usize) -> TranscriptPage<'_>;
```

- `page_size == 0` ⇒ `DEFAULT_REPLAY_PAGE_SIZE`.
- An empty transcript pages to an empty, `at_oldest` page.
- `before_seq == 0` pages to an empty, `at_oldest` page — there is nothing older than the head.
- `before_seq` beyond the transcript length clamps to the length, so a stale cursor returns the
  newest page rather than nothing.
- Both apply the existing `strip_tool_body` seam at the daemon before sending, exactly as
  `SNAPSHOT_THEN_LIVE` does — a paged frame is not a back door to the bodies.

The live tail continues `seq` from the snapshot length, so a frame appended while the stream is open
carries the position it would have had on a later re-read — **provided the transcript's coalesced
order does not change**.

Two things make that proviso necessary, and both hosts implement the same answer:

- A tool call broadcasts **twice** (its `running` then its terminal record) but resolves to a
  *single* coalesced entry. Numbering per delivered record would therefore drift one position ahead
  per completed call, and would give a refinement a position of its own. Each host instead keeps a
  `tool_call_id → seq` map, pre-seeded from the resolved transcript, so a refinement carries the
  position of the entry it refines and only a genuinely new entry advances the counter.
- Coalescing keeps a call at its **last** record's slot, so a re-read orders tool rows by
  *completion* time while the live tail can only number them by *first-record* time. Interleave two
  calls — `a` starts, `b` starts, `b` finishes, `a` finishes — and the live tail says `a, b` where a
  re-read resolves `b, a`. Closing that would mean renumbering frames already sent, which the wire
  cannot express: a frame carries one immutable `seq`. A stable row is worth more than an exact
  match with a hypothetical re-read, so the append-only semantic stands.

Neither divergence reaches the client's cursor: `useAcpReplay` reads `seq` only off the **first**
frame of the tail page, and merges live frames by `tool_call_id` rather than by position.

## Client contract

### `useAcpReplay`

- Opens the transcript feed in `TAIL_THEN_LIVE` with `page_size = 100` instead of
  `SNAPSHOT_THEN_LIVE`. `loadSnapshot()` keeps its name and its laziness contract (the overlay still
  defers; the Activities view still pulls eagerly).
- New returns: `atOldest: boolean`, `loadingOlder: boolean`, `loadOlder: () => void`.
- `loadOlder()` is a no-op while `atOldest`, while a fetch is in flight, or before the tail page has
  delivered a cursor.

### `agentActivityRegistry`

Per-session state gains `oldestSeq: number | null`, `atOldest: boolean`, `loadingOlder: boolean`,
and a `prependMessages(sessionId, messages, firstSeq, atOldest)` writer. Prepending a page whose
`firstSeq` is not strictly below the current cursor is ignored — a duplicate in-flight response can
never double-render a page.

### `AgentChatView` (read-only only)

New props: `onLoadOlder?`, `hasOlder?`, `loadingOlder?`. New elements:

| `data-testid` | What it is |
|---|---|
| `agent-chat-jump-to-latest` | The detached-mode affordance; its text carries the arrived-since-detach count |
| `agent-chat-older-loading` | The top-edge indicator shown while an older page is in flight |
| `agent-chat-scroll-state` | Hidden mirror of the viewport: `data-pinned`, `data-scroll-top`, `data-scroll-height`, `data-client-height`. The single source of truth for scroll assertions, mirroring `terminal-page-scrollbar` in the terminal work |

**Inline layout contract.** The transcript root and its message list additionally declare
`display/flex/min-height/overflow-y` **inline**, duplicating the equivalent Tailwind classes. The
scroll container's ability to overflow is not decoration — it is the precondition for every
behavior above — and the Cypress component harness loads no stylesheet
(`docs/dev/TODO.md` § "The component harness renders without any CSS"), so a Tailwind-only
declaration leaves the container unscrollable and every follow/paging test vacuous. Specs mount the
surface inside a fixed-height, inline-styled host.

The contract runs the **whole ancestor chain down to the scroll container**, not just the two
elements above. `AgentActivityOverlay`'s panel is `flex h-full w-full flex-col`, equally inert
without a stylesheet, so a transcript root that declares its own flex/overflow inline still has no
bounded height inside it and still cannot overflow. The overlay panel therefore carries the same
inline height declaration. (Confirmed empirically while writing the acceptance specs: the
follow/paging cases fail with `cy.scrollTo() failed because this element is not scrollable` on
`agent-chat-messages`, and the overlay case cannot be satisfied by the transcript's own declarations
alone.)

## Acceptance criteria

1. **Opens at the newest entry.** A recorded transcript longer than the viewport opens with its
   last entry visible and its first entry out of view.
2. **Opens with the newest page only.** The transcript feed is opened in `TAIL_THEN_LIVE` with
   `page_size = 100`; a transcript longer than the page renders only its newest page.
3. **Follows live activity while pinned.** A frame arriving while pinned scrolls into view.
4. **Never yanks a reader.** After scrolling up, an arriving frame changes neither the scroll offset
   nor the pinned state.
5. **Counts what arrived while detached.** Detached, `agent-chat-jump-to-latest` is visible and
   reports the number of entries that arrived since detaching.
6. **Jump re-attaches.** Clicking it scrolls to the newest entry, re-pins, and removes the
   affordance.
7. **Scrolling back to the bottom re-attaches** without the click.
8. **Reaching the top fetches the previous page.** `GetAcpReplayPage` is called exactly once, with
   `before_seq` equal to the loaded range's current cursor.
9. **The older page prepends without moving the read position.** The prepended entries render above
   the range, and the entry the operator was looking at stays at the same offset.
10. **The head closes the range.** After a page carrying `at_oldest`, no further `GetAcpReplayPage`
    is issued however far the operator scrolls, and no loading indicator reappears.
11. **A failed page fetch is retryable.** The loaded range is unchanged, the indicator clears, and a
    later scroll-to-top issues a new call.
12. **The overlay behaves identically.** Opening the top-bar Agent Activity overlay shows the same
    tail-first transcript with the same follow behavior.
13. **The empty state is unchanged.** A session that recorded nothing still renders its explicit
    empty state, gated on the count feed as before.

## Edge cases

- **Transcript shorter than one page** — the tail page *is* the whole transcript; `at_oldest` is
  true from the first frame and no page fetch is ever issued.
- **Frames arriving while a page fetch is in flight** — the live tail appends at the bottom and the
  page prepends at the top; the two are independent and neither resets the other.
- **A session switch mid-fetch** — the resolved page is discarded rather than written under the new
  session's key. `sessionId` keys every registry write, as it already does.
- **A cached transcript on switch-back** — the registry's cached range (and its cursor) is reused,
  and the viewport re-opens pinned to the bottom of it. No re-fetch, no re-page.
- **A stale cursor after eviction** — the daemon clamps `before_seq` to the transcript length rather
  than returning nothing, so a cursor from a transcript that has since been rewritten still resolves
  to a real page.
- **Detached when the head is reached** — the affordance state is independent of `at_oldest`; a
  reader at the head of a live session still sees jump-to-latest.

## Decisions & trade-offs

- **Prepend into the same list, rather than the terminal's two-overlay double-buffer.** The terminal
  needs the overlay because ghostty-web cannot insert above written content. A React list can, so
  the transcript takes the simpler route and keeps one scroll container.
- **`seq` on the frame, not a separate cursor message.** The stream already carries a frame per
  transcript entry; stamping it with its absolute position makes the cursor free and makes the
  live tail's positions consistent with a later re-read.
- **A unary `GetAcpReplayPage` rather than a second stream mode.** A page is a bounded, one-shot
  answer, and unary is the shape that already peer-forwards (`GetAcpToolCallDetail`). The streaming
  modes cannot forward yet.
- **Absolute `seq` over an opaque cursor token.** The transcript is a resolved, ordered list rebuilt
  per read; an index is the honest identity of a position in it, and it lets the client detect
  duplicate pages arithmetically.
- **Sticky-bottom over force-to-bottom.** Forcing every frame to the bottom would make history
  unreadable on a busy session — the exact failure this change exists to fix, in the other
  direction.
- **An inline layout contract on the scroll container over importing the app stylesheet into the
  component harness.** Importing `src/index.css` would make Tailwind live and all layout testable,
  but it perturbs ~163 existing specs and is its own changeset (already recorded in
  `docs/dev/TODO.md`). Declaring the scroll container's own flex/overflow inline is contained to
  this feature and costs only a duplicated layout declaration on two elements.
- **A hidden `agent-chat-scroll-state` mirror over reading scroll offsets in specs.** Same reasoning
  as the terminal's `terminal-page-scrollbar`: one element is the declared source of truth for
  viewport position, so tests bind to a contract rather than to layout arithmetic.
- **Page size 100.** Comfortably more than a viewport of entries at any realistic row height, so the
  first scroll-up is rare, while staying two orders of magnitude below a large session's transcript.

## Future scope

- Auto-follow for the live, interactive chat surfaces.
- Virtualized rendering, once paging back far enough makes the mounted list itself the cost.
- Persisting the read position per session across switches.
- Forwarding `StreamAcpReplay` to a peer daemon (blocked on the keepalive gap it already shares with
  `StreamSessionActivity`).
