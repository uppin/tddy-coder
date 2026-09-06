/**
 * Acceptance test: GrpcSessionTerminal overlay double-buffer history wiring.
 *
 * PRD: docs/ft/web/terminal-replay-lazy-scroll.md
 *
 * `GrpcSessionTerminal` captures the `endOffset` anchor from the initial `StreamTerminalOutput`
 * frame, builds a forward `historyFetcher`, and passes it (plus full frames carrying the offset
 * metadata) to `GhosttyTerminalSession`. When the user activates "Load earlier output",
 * `GetTerminalHistory` is called forward from offset 0 bounded by the anchor; a second call chains
 * forward from the previous chunk's `end_offset` until `at_end`. The shared component overlays the
 * page terminal behind the live one, shows a loading indicator while filling, then swaps it to the
 * foreground once `at_end` is reached. This test verifies the wiring (RPC offsets + buffer content);
 * the paging UX is covered by TerminalHistoryPaging.cy.tsx.
 */

import React, { useMemo } from "react";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  ConnectionService,
  GetTerminalHistoryRequestSchema,
  SendTerminalInputResponseSchema,
  SessionTerminalOutputSchema,
  TerminalHistoryChunkSchema,
} from "../../src/gen/connection_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import {
  decodeConnectStreamRequestBody,
  encodeConnectStreamFrames,
  toArrayBuffer,
} from "../support/rpc/protoRpc";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "history-test-session-aa11";
const SESSION_TOKEN = "test-token-history";
const ANCHOR = 1000n;

const OK_SEND_INPUT = toArrayBuffer(
  toBinary(SendTerminalInputResponseSchema, create(SendTerminalInputResponseSchema, {})),
);

/** The initial `StreamTerminalOutput` replay frame: last-screen bytes tagged with `endOffset`, and
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
// Backend doubles
// ---------------------------------------------------------------------------

function interceptReplayStreamOutput() {
  const body = encodeConnectStreamFrames([toBinary(SessionTerminalOutputSchema, REPLAY_FRAME)]);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/StreamTerminalOutput", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/connect+proto" }, body });
  }).as("streamTerminalOutput");
}

/** A swappable forward-chunk responder so the test can serve first-then-final chunks in order. */
function interceptGetTerminalHistory() {
  const chunks = [toBinary(TerminalHistoryChunkSchema, FIRST_CHUNK), toBinary(TerminalHistoryChunkSchema, FINAL_CHUNK)];
  let call = 0;
  cy.intercept("POST", "**/rpc/connection.ConnectionService/GetTerminalHistory", (req) => {
    const body = encodeConnectStreamFrames([chunks[call] ?? chunks[chunks.length - 1]]);
    call += 1;
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/connect+proto" }, body });
  }).as("getTerminalHistory");
}

function interceptSendTerminalInput() {
  cy.intercept("POST", "**/rpc/connection.ConnectionService/SendTerminalInput", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: OK_SEND_INPUT });
  }).as("sendTerminalInput");
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

function Harness() {
  const transport = useMemo(
    () =>
      createConnectTransport({
        baseUrl: `${window.location.origin}/rpc`,
        useBinaryFormat: true,
      }),
    [],
  );
  const client = useMemo(() => createClient(ConnectionService, transport), [transport]);

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
  beforeEach(() => {
    interceptReplayStreamOutput();
    interceptGetTerminalHistory();
    interceptSendTerminalInput();
  });

  it("forwards a forward GetTerminalHistory (from_offset=0, until_offset=anchor) when the user loads earlier output", () => {
    // Given — the terminal mounts and the initial replay frame (endOffset=1000) arrives
    cy.mount(<Harness />);
    cy.wait("@streamTerminalOutput");

    // Then — the affordance is visible once the anchor has been captured
    byTestId(TEST_IDS.loadEarlierHistory, { timeout: 10000 }).should("exist").and("be.visible");

    // When — the user activates "Load earlier output"
    byTestId(TEST_IDS.loadEarlierHistory).click();

    // Then — GetTerminalHistory is called forward from offset 0 bounded by the anchor
    cy.wait("@getTerminalHistory").then((interception) => {
      const decoded = fromBinary(
        GetTerminalHistoryRequestSchema,
        decodeConnectStreamRequestBody(interception.request.body),
      );
      expect(decoded.fromOffset, "first fetch fromOffset").to.equal(0n);
      expect(decoded.untilOffset, "first fetch untilOffset").to.equal(ANCHOR);
    });

    // And — the older-history terminal received the first chunk's bytes
    byTestId(TEST_IDS.terminalOlderBufferText, { timeout: 10000 }).should(($el) => {
      expect($el[0].textContent ?? "", "older buffer holds first chunk").to.contain("older-1");
    });

    // When — the fill chains: a second GetTerminalHistory is issued forward from 600
    cy.wait("@getTerminalHistory").then((interception) => {
      const decoded = fromBinary(
        GetTerminalHistoryRequestSchema,
        decodeConnectStreamRequestBody(interception.request.body),
      );
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
