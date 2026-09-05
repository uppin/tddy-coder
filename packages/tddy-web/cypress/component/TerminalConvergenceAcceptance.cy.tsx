/**
 * Acceptance spec: one terminal, fed by the session connection, with scrollback on every wire.
 *
 * Two components used to render identical chrome and differ in how bytes arrived and what each
 * could do. Node 3 removed the first difference's cause — a `SessionConnection` owns the room, the
 * identity and the token, so the LiveKit terminal was connecting something it no longer owned. What
 * was left is the second: **only the gRPC terminal could scroll back.** A LiveKit session could not
 * see past what was live, because that component had no `HistoryFetcher`, no offset tracking and no
 * page terminal.
 *
 * These specs mount the real `GhosttyTerminalSession` — through `aGhosttyTerminalSession`, which
 * hands it a feed and nothing else — and assert through its rendered surface: the readable mirror
 * of what the canvas painted, the bytes it writes back, and the scrollback affordance. Delete the
 * component and every one of them fails.
 *
 * Docs: `packages/tddy-web/docs/terminal-session.md`.
 * Stack: `optional-livekit` node 5 of 7.
 */

import type { TerminalHistoryChunk } from "../../src/rpc/connections/terminal";
import { aGhosttyTerminalSession } from "../support/drivers/ghosttyTerminalSessionDriver";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** One page of older output — the whole of it, as a host that can replay would serve it. */
function anOlderChunk(text: string): TerminalHistoryChunk {
  return {
    data: new TextEncoder().encode(text),
    startOffset: BigInt(0),
    endOffset: BigInt(text.length),
    atOldest: true,
    atEnd: true,
  };
}

/** The history a connection that can replay serves: one page of older output, and no more. */
function replaying(text: string) {
  return async () => anOlderChunk(text);
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("a terminal fed by a session connection", () => {
  it("renders output arriving on the feed, whatever carries it", () => {
    // Given the one terminal on a feed — it never learns which wire this is
    const terminal = aGhosttyTerminalSession().mount().expectReady();

    // When output arrives
    terminal.deliver("hello from the session");

    // Then it is painted. One component, one path, no `livekit-client` import.
    terminal.expectTerminalShows("hello from the session");
  });

  it("writes typed input back to the feed", () => {
    // Given a mounted terminal
    const terminal = aGhosttyTerminalSession().mount().expectReady();

    // When the operator types
    terminal.type("ls");

    // Then each keystroke went back down the feed the caller opened
    terminal.expectSentToFeed("l").expectSentToFeed("s");
  });

  it("offers scrollback on a LiveKit-carried session, which it has never had", () => {
    // Given a session whose connection can replay older output. Before this node a LiveKit session
    // could not: the LiveKit terminal had no history fetcher at all.
    const terminal = aGhosttyTerminalSession({ history: replaying("output from earlier") })
      .mount()
      .expectReady();
    terminal.deliver("live output");

    // When the user scrolls back
    terminal.expectScrollbackOffered().loadEarlierOutput();

    // Then earlier output is loaded and shown, and the terminal is on the history page
    terminal.expectEarlierOutputShows("output from earlier").expectViewingEarlierOutput();
  });

  it("hides the scrollback control when the connection cannot replay", () => {
    // Given a live-tail-only feed
    const terminal = aGhosttyTerminalSession().mount().expectReady();

    // When output arrives
    terminal.deliver("live output");

    // Then the terminal still works and simply offers no scrollback. The positive assertion comes
    // first so a component that renders nothing cannot satisfy the absence check.
    terminal.expectTerminalShows("live output").expectNoScrollbackOffered();
  });
});
