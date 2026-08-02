/**
 * Acceptance: the recorded ACP transcript is **tail-first, auto-following, and paged backwards**.
 *
 * Today the transcript opens on its oldest entry, never follows live activity, and transfers the
 * whole persisted history to render a screenful of its end. These specs pin the inversion: the view
 * opens on the newest entry of the newest page, follows the agent while the reader is at the bottom,
 * never yanks a reader who has scrolled away, and pages older history in on demand via a reverse
 * cursor.
 *
 * Both consuming surfaces are covered — `SessionActivitiesPane` (a dormant session's main pane, which
 * pulls eagerly) and `AgentActivityOverlay` (the top-bar popover, which pulls on open) — over an
 * in-memory backend that models the daemon in both directions: it serves a tail page only for a
 * tail-mode request and the whole head-first history for anything else, so a spec fails when the
 * client opens the wrong mode rather than passing on the fake's generosity.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Tail-first, auto-scrolling transcript
 */

import React from "react";
import { SessionActivitiesPane } from "../../src/components/sessions/SessionActivitiesPane";
import { AgentActivityOverlay } from "../../src/components/sessions/AgentActivityOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentChatPage } from "../support/pages/agentChatPage";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import {
  aRecordedTranscript,
  aTailReplayBackend,
  aTailReplayWithHeldPages,
  replayAgentText,
  Code,
  DEFAULT_REPLAY_PAGE_SIZE,
  TAIL_THEN_LIVE,
  type TailReplayBackend,
} from "../support/rpc/acpReplay";
import type { AcpAgentMessage } from "../../src/gen/tddy/acp/v1/acp_pb";

/** Entries in a transcript long enough that its newest page is not its whole history. */
const A_LONG_TRANSCRIPT = 250;

/** Entries in a transcript whose newest page plus one older page is the whole history — so paging
 *  back exactly once reaches the head. */
const A_TRANSCRIPT_ONE_PAGE_FROM_ITS_HEAD = 150;

/** Entries in a transcript shorter than one page: the tail page IS the whole history, so no page
 *  fetch is ever issued and scrolling up cannot perturb a test about following. */
const A_SHORT_TRANSCRIPT = 60;

/** Index of the newest entry within a full loaded page. */
const NEWEST_IN_A_FULL_PAGE = DEFAULT_REPLAY_PAGE_SIZE - 1;

/**
 * A fixed-height, inline-styled host. The component harness loads no stylesheet
 * (`docs/dev/TODO.md` § "The component harness renders without any CSS"), so every Tailwind class is
 * inert and a surface mounted bare measures the full viewport and can never overflow. These specs
 * are entirely about a scroll container, so the host declares its bounded height inline; the
 * transcript's own flex/overflow declarations are the component's contract to keep
 * (`docs/ft/web/agent-activity-pane.md` § Tail-first — the scroll container must declare its own
 * layout).
 */
function inAFixedHeightPane(surface: React.ReactElement) {
  return (
    <div style={{ display: "flex", flexDirection: "column", height: 320, width: 480 }}>
      {surface}
    </div>
  );
}

function mountActivitiesView(replay: TailReplayBackend) {
  mountWithRpc(
    inAFixedHeightPane(<SessionActivitiesPane sessionId="s1" sessionToken="tok" />),
    replay.backend,
  );
}

/**
 * Deliver frames on the live tail **in command-queue order**.
 *
 * A bare `replay.pushLive(...)` is a plain function call, so it runs while the test body is being
 * evaluated — before `cy.mount` has even been dequeued. The frames would then be sitting in the
 * fake's tail when the stream subscribes, arrive as part of the initial page load, and never be
 * "live" at all. Wrapping the push in `cy.then` puts it where the `// When` it sits under says it is.
 */
function recordLive(replay: TailReplayBackend, ...frames: AcpAgentMessage[]) {
  cy.then(() => {
    for (const frame of frames) replay.pushLive(frame);
  });
}

/**
 * Hosts the Activities view and swaps which session it shows **without remounting the pane** — which
 * is how `SessionMainPane` behaves: selecting another session re-renders the same element position
 * with a new `sessionId`. Reproducing that is the whole point of the switch-back case; a second
 * `cy.mount` would reset the transcript's scroll state by construction and prove nothing.
 */
function ASwitchableActivitiesView() {
  const [sessionId, setSessionId] = React.useState("s1");
  return (
    <div style={{ display: "flex", flexDirection: "column", height: 320, width: 480 }}>
      <button data-testid="switch-session-s1" onClick={() => setSessionId("s1")}>
        s1
      </button>
      <button data-testid="switch-session-s2" onClick={() => setSessionId("s2")}>
        s2
      </button>
      <SessionActivitiesPane sessionId={sessionId} sessionToken="tok" />
    </div>
  );
}

function mountSwitchableActivitiesView(replay: TailReplayBackend) {
  mountWithRpc(<ASwitchableActivitiesView />, replay.backend);
}

/** Select another session in the host above. Keeps the raw selector out of the test body. */
function selectSession(sessionId: "s1" | "s2") {
  cy.get(`[data-testid="switch-session-${sessionId}"]`).click();
}

function mountActivityOverlay(replay: TailReplayBackend) {
  mountWithRpc(
    inAFixedHeightPane(
      <AgentActivityOverlay sessionId="s1" sessionToken="tok" sessionType="tool" />,
    ),
    replay.backend,
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("opens the recorded transcript scrolled to its newest entry", () => {
  // Given — a recorded transcript far taller than the pane can show at once
  const replay = aTailReplayBackend({ transcript: aRecordedTranscript(A_LONG_TRANSCRIPT) });

  // When — the operator opens a dormant session's Activities view
  mountActivitiesView(replay);

  // Then — the newest entry is on screen and the oldest of the loaded range has been scrolled past
  agentChatPage.expectTranscriptScrollable();
  agentChatPage.chatMessage(NEWEST_IN_A_FULL_PAGE).should("be.visible").and("have.text", "Entry 250");
  agentChatPage.chatMessage(0).should("not.be.visible");
});

it("requests only the newest page of a long transcript when the view opens", () => {
  // Given — 250 recorded entries, of which one page is 100
  const replay = aTailReplayBackend({ transcript: aRecordedTranscript(A_LONG_TRANSCRIPT) });

  // When
  mountActivitiesView(replay);

  // Then — the feed was opened tail-first for one page…
  cy.wrap(null).should(() => {
    expect(replay.transcriptOpens()).to.deep.equal([
      { mode: TAIL_THEN_LIVE, pageSize: DEFAULT_REPLAY_PAGE_SIZE },
    ]);
  });

  // …and only that page is loaded: the range starts at entry 151, and nothing sits past its end
  agentChatPage.chatMessage(0).should("have.text", "Entry 151");
  agentChatPage.chatMessage(DEFAULT_REPLAY_PAGE_SIZE, { timeout: 1000 }).should("not.exist");
});

it("scrolls a live activity frame into view while pinned to the newest entry", () => {
  // Given — a transcript open at its newest entry
  const replay = aTailReplayBackend({ transcript: aRecordedTranscript(A_SHORT_TRANSCRIPT) });
  mountActivitiesView(replay);
  agentChatPage.chatMessage(59).should("have.text", "Entry 60");

  // When — the agent records one more entry while the reader is at the bottom
  recordLive(replay, replayAgentText("Entry 61", 61_000));

  // Then — it scrolls into view and the transcript keeps following
  agentChatPage.chatMessage(60).should("be.visible").and("have.text", "Entry 61");
  agentChatPage.expectFollowingNewest();
});

it("leaves the read position untouched when a live frame arrives after scrolling up", () => {
  // Given — a reader who has scrolled back into the history of a transcript with no older page
  const replay = aTailReplayBackend({ transcript: aRecordedTranscript(A_SHORT_TRANSCRIPT) });
  mountActivitiesView(replay);
  agentChatPage.chatMessage(59).should("have.text", "Entry 60");
  agentChatPage.scrollTranscriptToTop();
  agentChatPage.expectDetachedFromNewest();

  agentChatPage.readScrollTop().then((offsetWhileReading) => {
    // When — the agent records another entry
    replay.pushLive(replayAgentText("Entry 61", 61_000));

    // Then — the entry renders, but neither the offset nor the detached state moves
    agentChatPage.chatMessage(60).should("have.text", "Entry 61");
    agentChatPage.expectScrollTop(offsetWhileReading);
    agentChatPage.expectDetachedFromNewest();
  });
});

it("counts the entries that arrived while the reader was scrolled away", () => {
  // Given — a reader scrolled away from the newest entry
  const replay = aTailReplayBackend({ transcript: aRecordedTranscript(A_SHORT_TRANSCRIPT) });
  mountActivitiesView(replay);
  agentChatPage.chatMessage(59).should("have.text", "Entry 60");
  agentChatPage.scrollTranscriptToTop();
  agentChatPage.expectDetachedFromNewest();

  // When — three entries are recorded while they read
  recordLive(
    replay,
    replayAgentText("Entry 61", 61_000),
    replayAgentText("Entry 62", 62_000),
    replayAgentText("Entry 63", 63_000),
  );

  // Then — the affordance reports how many arrived, not merely that something did
  agentChatPage.chatMessage(62).should("have.text", "Entry 63");
  agentChatPage.chatJumpToLatest().should("contain.text", "3");
});

it("returns to the newest entry and clears the counter when jump-to-latest is clicked", () => {
  // Given — an entry arrived while the reader was scrolled away
  const replay = aTailReplayBackend({ transcript: aRecordedTranscript(A_SHORT_TRANSCRIPT) });
  mountActivitiesView(replay);
  agentChatPage.chatMessage(59).should("have.text", "Entry 60");
  agentChatPage.scrollTranscriptToTop();
  agentChatPage.expectDetachedFromNewest();
  recordLive(replay, replayAgentText("Entry 61", 61_000));
  agentChatPage.chatJumpToLatest().should("contain.text", "1");

  // When
  agentChatPage.jumpToLatest();

  // Then — the newest entry is on screen, following resumes, and the affordance is gone
  agentChatPage.chatMessage(60).should("be.visible").and("have.text", "Entry 61");
  agentChatPage.expectFollowingNewest();
  agentChatPage.chatJumpToLatest({ timeout: 1000 }).should("not.exist");
});

it("re-attaches to the newest entry when the reader scrolls back to the bottom", () => {
  // Given — an entry arrived while the reader was scrolled away
  const replay = aTailReplayBackend({ transcript: aRecordedTranscript(A_SHORT_TRANSCRIPT) });
  mountActivitiesView(replay);
  agentChatPage.chatMessage(59).should("have.text", "Entry 60");
  agentChatPage.scrollTranscriptToTop();
  agentChatPage.expectDetachedFromNewest();
  recordLive(replay, replayAgentText("Entry 61", 61_000));
  agentChatPage.chatJumpToLatest().should("contain.text", "1");

  // When — they scroll back down themselves rather than clicking
  agentChatPage.scrollTranscriptToBottom();

  // Then — following resumes and the counter clears without the click
  agentChatPage.expectFollowingNewest();
  agentChatPage.chatJumpToLatest({ timeout: 1000 }).should("not.exist");
});

it("fetches the page before the cursor when the top of the loaded range is reached", () => {
  // Given — a loaded range starting at entry 151, i.e. a reverse cursor of seq 150
  const replay = aTailReplayBackend({ transcript: aRecordedTranscript(A_LONG_TRANSCRIPT) });
  mountActivitiesView(replay);
  agentChatPage.chatMessage(0).should("have.text", "Entry 151");

  // When — the reader reaches the top of what is loaded
  agentChatPage.scrollTranscriptToTop();

  // Then — exactly one page is fetched, from that cursor
  cy.wrap(null).should(() => {
    expect(replay.pageCursors()).to.deep.equal([150]);
  });
});

it("prepends the older page above the loaded range without moving the read position", () => {
  // Given — a reader at the top of the loaded range, holding entry 151 at the viewport's top edge
  const replay = aTailReplayBackend({ transcript: aRecordedTranscript(A_LONG_TRANSCRIPT) });
  mountActivitiesView(replay);
  agentChatPage.chatMessage(0).should("have.text", "Entry 151");

  // When — reaching the top pages the previous 100 entries in
  agentChatPage.scrollTranscriptToTop();

  // Then — they land above the range, which now runs entry 51 → 250…
  agentChatPage.chatMessage(0).should("have.text", "Entry 51");
  agentChatPage.chatMessage(DEFAULT_REPLAY_PAGE_SIZE).should("have.text", "Entry 151");

  // …and entry 151 has not moved under the reader
  agentChatPage.expectEntryAtViewportTop(DEFAULT_REPLAY_PAGE_SIZE);
});

it("marks the older page as loading while the fetch is in flight, and clears it once it lands", () => {
  // Given — a replay host that has received the page request but not yet answered it
  const { replay, releasePage } = aTailReplayWithHeldPages({
    transcript: aRecordedTranscript(A_LONG_TRANSCRIPT),
  });
  mountActivitiesView(replay);
  agentChatPage.chatMessage(0).should("have.text", "Entry 151");

  // When — the reader reaches the top of the loaded range
  agentChatPage.scrollTranscriptToTop();

  // Then — the wait is shown rather than left to look like a transcript that simply stops…
  agentChatPage.chatOlderLoading().should("exist");

  // …and it goes away when the page arrives, rather than sticking behind the prepended entries
  cy.then(() => releasePage());
  agentChatPage.chatMessage(0).should("have.text", "Entry 51");
  agentChatPage.chatOlderLoading({ timeout: 1000 }).should("not.exist");
});

it("stops fetching older pages once the transcript head is reached", () => {
  // Given — a transcript exactly one page short of its head
  const replay = aTailReplayBackend({
    transcript: aRecordedTranscript(A_TRANSCRIPT_ONE_PAGE_FROM_ITS_HEAD),
  });
  mountActivitiesView(replay);
  agentChatPage.chatMessage(0).should("have.text", "Entry 51");

  // When — the reader pages back to the head and then keeps scrolling up
  agentChatPage.scrollTranscriptToTop();
  agentChatPage.chatMessage(0).should("have.text", "Entry 1");
  agentChatPage.scrollTranscriptToTop();
  agentChatPage.scrollTranscriptToTop();

  // Then — the range is closed: one fetch ever made, and no loading indicator returns
  cy.wrap(null).should(() => {
    expect(replay.pageCursors()).to.deep.equal([50]);
  });
  agentChatPage.chatOlderLoading({ timeout: 1000 }).should("not.exist");
});

it("leaves the loaded range intact and retryable when an older page fetch fails", () => {
  // Given — a replay host that cannot serve pages
  const replay = aTailReplayBackend({
    transcript: aRecordedTranscript(A_LONG_TRANSCRIPT),
    failPagesWith: Code.Unavailable,
  });
  mountActivitiesView(replay);
  agentChatPage.chatMessage(0).should("have.text", "Entry 151");

  // When — the reader reaches the top and the fetch fails
  agentChatPage.scrollTranscriptToTop();
  cy.wrap(null).should(() => {
    expect(replay.pageCursors()).to.deep.equal([150]);
  });

  // Then — the loaded range is exactly what it was, with no fabricated or partial page…
  agentChatPage.chatMessage(0).should("have.text", "Entry 151");
  agentChatPage.chatOlderLoading({ timeout: 1000 }).should("not.exist");

  // …and reaching the top again retries from the same cursor
  agentChatPage.scrollTranscriptToBottom();
  agentChatPage.scrollTranscriptToTop();
  cy.wrap(null).should(() => {
    expect(replay.pageCursors()).to.deep.equal([150, 150]);
  });
});

it("opens an already-visited session at its newest entry after reading back through another", () => {
  // Given — two sessions the operator has both opened, so each has a cached transcript and the pane
  // keeps one mounted transcript across the switch…
  const replay = aTailReplayBackend({ transcript: aRecordedTranscript(A_SHORT_TRANSCRIPT) });
  mountSwitchableActivitiesView(replay);
  agentChatPage.chatMessage(59).should("have.text", "Entry 60");
  selectSession("s2");
  agentChatPage.chatMessage(59).should("have.text", "Entry 60");

  // …and a reader who has scrolled back into the second session's history
  agentChatPage.scrollTranscriptToTop();
  agentChatPage.expectDetachedFromNewest();

  // When — they return to the first session
  selectSession("s1");

  // Then — it opens on its newest entry, following it. The scroll position belonged to the session
  // it was measured against, not to the pane.
  agentChatPage.expectFollowingNewest();
  agentChatPage.chatMessage(59).should("be.visible").and("have.text", "Entry 60");
});

it("shows the same tail-first transcript in the agent activity overlay", () => {
  // Given — the same long transcript, reached through the top-bar popover instead of the pane
  const replay = aTailReplayBackend({ transcript: aRecordedTranscript(A_LONG_TRANSCRIPT) });

  // When
  mountActivityOverlay(replay);
  agentActivityPage.open();

  // Then — the overlay opens on the newest page, scrolled to its newest entry
  agentChatPage.chatMessage(0).should("have.text", "Entry 151");
  agentChatPage.expectTranscriptScrollable();
  agentChatPage.chatMessage(NEWEST_IN_A_FULL_PAGE).should("be.visible").and("have.text", "Entry 250");
});
