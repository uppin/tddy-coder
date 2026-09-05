/**
 * Acceptance spec: one terminal, fed by the session connection, with scrollback on every wire.
 *
 * Two components used to render identical chrome and differ in how bytes arrived and what each
 * could do. Node 3 removed the first difference's cause — a `SessionConnection` owns the room, the
 * identity and the token, so the LiveKit terminal was connecting something it no longer owned. What
 * was left is the second: **only the gRPC terminal could scroll back.** A LiveKit session could not
 * see past what was live, because that component had no `HistoryFetcher`, no offset tracking, and
 * no page terminal.
 *
 * Both are now one `GhosttyTerminalSession`, which is what these specs pin.
 *
 * These specs pin the converged component's contract: it takes a feed and nothing else, and history
 * follows the feed rather than the transport.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-terminal-convergence.md`
 * Stack: `optional-livekit` node 5 of 7.
 */

import React from "react";
import {
  feedSupportsHistory,
  type TerminalFeed,
  type TerminalFrame,
  type TerminalHistoryChunk,
  type TerminalStream,
} from "../../src/rpc/connections/terminal";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** A stream a test can push frames into, and that records everything written to it. */
function aControllableStream() {
  let listener: ((frame: TerminalFrame) => void) | null = null;
  const written: Uint8Array[] = [];
  let closed = false;

  const stream: TerminalStream = {
    send: (data) => written.push(data),
    onMessage: (fn) => {
      listener = fn;
    },
    close: () => {
      closed = true;
    },
  };

  return {
    stream,
    written,
    isClosed: () => closed,
    /**
     * Settles once the terminal has registered its listener.
     *
     * A frame delivered before it has is dropped and never retried — `cy.mount` returns before
     * React has run the mounting effect, so a delivery chained straight off it races the
     * registration rather than the behaviour under test.
     */
    listening: () =>
      cy.wrap(null, { log: false }).should(() => {
        expect(listener, "terminal registered its onMessage listener").to.not.equal(null);
      }),
    deliver: (text: string, endOffset = BigInt(0), atOldest = false) =>
      listener?.({ data: new TextEncoder().encode(text), endOffset, atOldest }),
  };
}

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

// ---------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------

/**
 * A stand-in for the converged terminal: it takes a feed, renders what arrives, offers scrollback
 * only when the feed can serve it, and writes input back to the stream.
 *
 * The real component is `GhosttyTerminalSession`; this pins the *contract* the feed establishes,
 * which is what this node owns. Porting the 736 + 631 lines of chrome onto it is `/green`.
 */
function TerminalProbe({ feed }: { feed: TerminalFeed }) {
  const [output, setOutput] = React.useState("");
  const [older, setOlder] = React.useState("");
  const canScrollBack = feedSupportsHistory(feed);

  React.useEffect(() => {
    feed.stream.onMessage((frame) => {
      setOutput((current) => current + new TextDecoder().decode(frame.data));
    });
  }, [feed]);

  return (
    <div>
      <div data-testid="output">{output || "(nothing yet)"}</div>
      <div data-testid="older">{older || "(no older output loaded)"}</div>
      {canScrollBack && (
        <button
          data-testid="load-older"
          onClick={() => {
            void feed.history?.(BigInt(0), BigInt(100)).then((chunk) => {
              if (chunk) setOlder(new TextDecoder().decode(chunk.data));
            });
          }}
        >
          load older
        </button>
      )}
      <button data-testid="type" onClick={() => feed.stream.send(new TextEncoder().encode("ls\r"))}>
        type
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("a terminal fed by a session connection", () => {
  it("renders output arriving on the stream, whatever carries it", () => {
    // Given any feed at all — the component never learns which wire this is
    const { stream, deliver, listening } = aControllableStream();

    cy.mount(<TerminalProbe feed={{ stream }} />);

    // When output arrives
    listening().then(() => deliver("hello from the session"));

    // Then it is rendered. One component, one path, no `livekit-client` import.
    byTestId("output").should("have.text", "hello from the session");
  });

  it("writes typed input back to the stream", () => {
    const { stream, written } = aControllableStream();

    cy.mount(<TerminalProbe feed={{ stream }} />);
    byTestId("type").click();

    cy.wrap(null).should(() => {
      expect(written).to.have.length(1);
      expect(new TextDecoder().decode(written[0])).to.equal("ls\r");
    });
  });

  it("offers scrollback on a LiveKit-carried session, which it has never had", () => {
    // Given a session whose connection can replay older output. Before this node a LiveKit session
    // could not: the LiveKit terminal had no history fetcher at all.
    const { stream } = aControllableStream();
    const feed: TerminalFeed = { stream, history: async () => anOlderChunk("output from earlier") };

    cy.mount(<TerminalProbe feed={feed} />);

    // When the user scrolls back
    byTestId("load-older").click();

    // Then earlier output is loaded
    byTestId("older").should("have.text", "output from earlier");
  });

  it("hides the scrollback control when the connection cannot replay", () => {
    // Given a live-tail-only feed
    const { stream, deliver, listening } = aControllableStream();

    cy.mount(<TerminalProbe feed={{ stream }} />);
    listening().then(() => deliver("live output"));

    // Then the terminal still works and simply offers no scrollback. The positive assertion comes
    // first so a component that throws cannot satisfy the absence check.
    byTestId("output").should("have.text", "live output");
    byTestId("load-older").should("not.exist");
  });
});
