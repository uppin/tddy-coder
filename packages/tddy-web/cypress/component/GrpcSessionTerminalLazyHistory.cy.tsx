/**
 * Acceptance test: GrpcSessionTerminal overlay double-buffer history wiring.
 *
 * PRD: docs/ft/web/terminal-replay-lazy-scroll.md
 *
 * `GrpcSessionTerminal` captures the `endOffset` anchor from the opening frame of its bidi
 * `StreamSessionTerminalIO` stream, builds a forward `historyFetcher`, and passes it (plus full frames carrying the offset
 * metadata) to `GhosttyTerminalSession`. When the user activates "Load earlier output",
 * `GetTerminalHistory` is called forward from offset 0 bounded by the anchor; a second call chains
 * forward from the previous chunk's `end_offset` until `at_end`. The shared component overlays the
 * page terminal behind the live one, shows a loading indicator while filling, then swaps it to the
 * foreground once `at_end` is reached. This test verifies the wiring (RPC offsets + buffer content);
 * the paging UX is covered by TerminalHistoryPaging.cy.tsx.
 *
 * connect-web cannot carry the bidi terminal stream over HTTP, so the daemon is an in-memory
 * `TerminalSessionService` that records every open and every `GetTerminalHistory` request.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient, type Client } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  TerminalSessionService,
  type GetTerminalHistoryRequest,
  type SessionTerminalInput,
  SessionTerminalOutputSchema,
  TerminalHistoryChunkSchema,
} from "../../src/gen/terminal_session_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { byTestId, TEST_IDS } from "../support/testIds";

type TerminalClient = Client<typeof TerminalSessionService>;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "history-test-session-aa11";
const SESSION_TOKEN = "test-token-history";
const ANCHOR = 1000n;

/** The opening frame of the terminal stream: last-screen bytes tagged with `endOffset`, and
 *  stamped with the session and resolved terminal it came from as the daemon stamps every frame. */
const REPLAY_FRAME = create(SessionTerminalOutputSchema, {
  data: new TextEncoder().encode("term:main\r\n"),
  endOffset: ANCHOR,
  atOldest: false,
  sessionId: SESSION_ID,
  terminalId: "main",
});

/** First forward chunk (0..600), not at_end — the fill continues. */
const FIRST_CHUNK = create(TerminalHistoryChunkSchema, {
  data: new TextEncoder().encode("older-1\r\n"),
  startOffset: 0n,
  endOffset: 600n,
  atOldest: true,
  atEnd: false,
});

/** Final forward chunk (600..1000), at_end — terminates the fill at the anchor. */
const FINAL_CHUNK = create(TerminalHistoryChunkSchema, {
  data: new TextEncoder().encode("older-2\r\n"),
  startOffset: 600n,
  endOffset: ANCHOR,
  atOldest: false,
  atEnd: true,
});

// ---------------------------------------------------------------------------
// Backend double
// ---------------------------------------------------------------------------

/**
 * The daemon, in memory: its bidi terminal stream answers the opening message with the replay
 * frame and stays open; `GetTerminalHistory` serves the first chunk, then the final one, recording
 * each request so the test can assert the anchor chain.
 */
function aHistoryServingDaemon() {
  const opens: SessionTerminalInput[] = [];
  const historyRequests: GetTerminalHistoryRequest[] = [];
  const chunks = [FIRST_CHUNK, FINAL_CHUNK];
  const backend = anInMemoryRpcBackend().implement(TerminalSessionService, {
    async *streamSessionTerminalIO(requests: AsyncIterable<SessionTerminalInput>) {
      const reader = requests[Symbol.asyncIterator]();
      const opening = await reader.next();
      if (opening.done) return;
      opens.push(opening.value);
      void (async () => {
        for (let next = await reader.next(); !next.done; next = await reader.next()) {
          /* input accepted */
        }
      })();
      yield REPLAY_FRAME;
      await new Promise<never>(() => undefined);
    },
    async *getTerminalHistory(req: GetTerminalHistoryRequest) {
      const chunk = chunks[historyRequests.length] ?? chunks[chunks.length - 1];
      historyRequests.push(req);
      yield chunk;
    },
  });
  const client: TerminalClient = createClient(TerminalSessionService, backend.transport());
  return { client, opens, historyRequests };
}

/** Yields the `n`th (1-based) `GetTerminalHistory` request once it has arrived. */
function historyRequestNumbered(daemon: ReturnType<typeof aHistoryServingDaemon>, n: number) {
  return cy
    .wrap(daemon, { timeout: 10000 })
    .should((d) => {
      expect(d.historyRequests.length, `GetTerminalHistory call #${n} was issued`).to.be.at.least(n);
    })
    .then((d) => d.historyRequests[n - 1]);
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

function Harness({ client }: { client: TerminalClient }) {
  return (
    <div style={{ width: 800, height: 400, position: "relative" }}>
      <UploadProgressProvider>
        <GrpcSessionTerminal
          sessionId={SESSION_ID}
          sessionToken={SESSION_TOKEN}
          client={client}
          connected={null}
        />
      </UploadProgressProvider>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("GrpcSessionTerminalLazyHistory — progressive forward-fill wiring", () => {
  it("forwards a forward GetTerminalHistory (from_offset=0, until_offset=anchor) when the user loads earlier output", () => {
    // Given — the terminal mounts and the initial replay frame (endOffset=1000) arrives
    const daemon = aHistoryServingDaemon();
    cy.mount(<Harness client={daemon.client} />);
    cy.wrap(daemon, { timeout: 10000 }).should((d) => {
      expect(d.opens.length, "the terminal stream was opened").to.be.greaterThan(0);
    });

    // Then — the affordance is visible once the anchor has been captured
    byTestId(TEST_IDS.loadEarlierHistory, { timeout: 10000 }).should("exist").and("be.visible");

    // When — the user activates "Load earlier output"
    byTestId(TEST_IDS.loadEarlierHistory).click();

    // Then — GetTerminalHistory is called forward from offset 0 bounded by the anchor
    historyRequestNumbered(daemon, 1).then((decoded) => {
      expect(decoded.fromOffset, "first fetch fromOffset").to.equal(0n);
      expect(decoded.untilOffset, "first fetch untilOffset").to.equal(ANCHOR);
    });

    // And — the older-history terminal received the first chunk's bytes
    byTestId(TEST_IDS.terminalOlderBufferText, { timeout: 10000 }).should(($el) => {
      expect($el[0].textContent ?? "", "older buffer holds first chunk").to.contain("older-1");
    });

    // When — the fill chains: a second GetTerminalHistory is issued forward from 600
    historyRequestNumbered(daemon, 2).then((decoded) => {
      expect(decoded.fromOffset, "second fetch fromOffset").to.equal(600n);
      expect(decoded.untilOffset, "second fetch untilOffset").to.equal(ANCHOR);
    });

    // And — the older-history terminal now holds both chunks in order, and the fill is complete
    byTestId(TEST_IDS.terminalOlderBufferText, { timeout: 10000 }).should(($el) => {
      const text = $el[0].textContent ?? "";
      expect(text, "older buffer holds both chunks").to.contain("older-1");
      expect(text, "older buffer holds both chunks").to.contain("older-2");
    });
  });
});
