/**
 * Unit tests for the terminal feed a session connection hands its terminal.
 *
 * The asymmetry this node removes: scrollback history existed **only** on the gRPC path. The gRPC
 * terminal had the forward-history loader, the offset tracking and a 50 000-line page terminal; the
 * LiveKit terminal had none of it, so a LiveKit session could not scroll back past what was live.
 * Once the feed carries the history fetcher, the transport stops deciding.
 *
 * Docs: `packages/tddy-web/docs/terminal-session.md`.
 */

import { describe, it, expect } from "bun:test";
import { feedSupportsHistory, type TerminalFeed, type TerminalStream } from "./terminal";

/**
 * An inert stream: it satisfies the shape a feed needs, and does nothing.
 *
 * These tests ask what a feed *offers*, never what flows over it — `feedSupportsHistory` reads the
 * feed's own shape — so neither writing nor delivery is wired up here.
 */
function aTerminalStream(): TerminalStream {
  return {
    send: () => {},
    onMessage: () => {},
    close: () => {},
  };
}

/** What a connection whose host can replay older output offers. */
function aFeedWithHistory(): TerminalFeed {
  return {
    stream: aTerminalStream(),
    history: async () => null,
  };
}

/** What a connection that can only tail the live output offers. */
function aLiveTailOnlyFeed(): TerminalFeed {
  return { stream: aTerminalStream() };
}

describe("a terminal feed", () => {
  it("supports scrollback when the connection offers a history fetcher", () => {
    // Given a feed carrying history — which, after this node, includes a LiveKit-carried session
    const feed = aFeedWithHistory();

    // Then the terminal can page backwards. This is the behaviour a LiveKit session has never had:
    // the LiveKit terminal had no HistoryFetcher, no offset tracking and no page terminal.
    expect(feedSupportsHistory(feed)).toBe(true);
  });

  it("degrades to live tail when the connection offers none", () => {
    // Given a feed that can only tail
    const feed = aLiveTailOnlyFeed();

    // Then the terminal renders live output and does not offer scrollback — no worse than the
    // LiveKit path behaves today, so nothing regresses on a transport that cannot serve history
    expect(feedSupportsHistory(feed)).toBe(false);
  });

  it("does not treat a present-but-undefined history as available", () => {
    // Given a feed built by spreading an object where `history` came through undefined — the shape
    // a provider produces when it builds the feed conditionally
    const feed: TerminalFeed = { stream: aTerminalStream(), history: undefined };

    // Then it is live-tail only. Reading `"history" in feed` instead of the value is the mistake
    // this pins: it would offer a scrollback control that calls undefined.
    expect(feedSupportsHistory(feed)).toBe(false);
  });
});
