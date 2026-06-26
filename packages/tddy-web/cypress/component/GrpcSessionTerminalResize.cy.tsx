/**
 * Acceptance tests for terminal dimension broadcasting in GrpcSessionTerminal.
 *
 * Bug: PTY spawns at hardcoded 220×24 (claude_cli_session.rs DEFAULT_TERM_COLS/ROWS).
 * The frontend fires an OSC resize after the terminal mounts, but by then the daemon has
 * already started replaying buffered output drawn at 220 cols — producing garbled output
 * in narrower containers.
 *
 * Fix: GrpcSessionTerminal must pass the container's measured cols×rows in the
 * StreamTerminalOutput request so the daemon can resize the PTY *before* replay starts.
 *
 * These tests define the expected behavior:
 * - StreamTerminalOutput request includes initial_cols > 0 and initial_rows > 0
 * - The dimensions reflect the actual container size, not the hardcoded 220×24 PTY default
 */

import React, { useMemo } from "react";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  ConnectionService,
  SendTerminalInputResponseSchema,
  StreamTerminalOutputRequestSchema,
} from "../../src/gen/connection_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { decodeConnectStreamRequestBody, toArrayBuffer } from "../support/rpc/protoRpc";

// ---------------------------------------------------------------------------
// Test harness — provides a real HTTP client Cypress can intercept
// ---------------------------------------------------------------------------

function Harness({ containerWidth = 800, containerHeight = 400 }: { containerWidth?: number; containerHeight?: number }) {
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
    <div style={{ width: containerWidth, height: containerHeight, position: "relative" }}>
      <GrpcSessionTerminal
        sessionId="resize-test-session-aabbcc"
        sessionToken="test-token"
        client={client}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const OK_SEND_TERMINAL_INPUT_BODY = toArrayBuffer(
  toBinary(SendTerminalInputResponseSchema, create(SendTerminalInputResponseSchema, {})),
);

/** Intercept StreamTerminalOutput; accept and return an empty (no-data) stream. */
function interceptStreamTerminalOutput() {
  cy.intercept("POST", "**/rpc/connection.ConnectionService/StreamTerminalOutput", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: new ArrayBuffer(0) });
  }).as("streamTerminalOutput");
}

/** Intercept SendTerminalInput (OSC resize sequences, keystrokes). */
function interceptSendTerminalInput() {
  cy.intercept("POST", "**/rpc/connection.ConnectionService/SendTerminalInput", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: OK_SEND_TERMINAL_INPUT_BODY });
  }).as("sendTerminalInput");
}

// ---------------------------------------------------------------------------
// AC1–AC3: StreamTerminalOutput request must include initial terminal dimensions
// ---------------------------------------------------------------------------

describe("GrpcSessionTerminalResize — initial dimensions in StreamTerminalOutput", () => {
  beforeEach(() => {
    interceptStreamTerminalOutput();
    interceptSendTerminalInput();
  });

  // -------------------------------------------------------------------------
  // AC1 — StreamTerminalOutput request includes initial_cols > 0
  // -------------------------------------------------------------------------

  it("includes initial_cols > 0 in the StreamTerminalOutput request on mount", () => {
    // Given — 800px wide container
    cy.mount(<Harness containerWidth={800} containerHeight={400} />);

    // When — component mounts and opens the output stream
    cy.wait("@streamTerminalOutput").then((interception) => {
      const decoded = fromBinary(
        StreamTerminalOutputRequestSchema,
        decodeConnectStreamRequestBody(interception.request.body),
      );

      expect((decoded as any).initialCols).to.be.greaterThan(0);
    });
  });

  // -------------------------------------------------------------------------
  // AC2 — StreamTerminalOutput request includes initial_rows > 0
  // -------------------------------------------------------------------------

  it("includes initial_rows > 0 in the StreamTerminalOutput request on mount", () => {
    cy.mount(<Harness containerWidth={800} containerHeight={400} />);

    cy.wait("@streamTerminalOutput").then((interception) => {
      const decoded = fromBinary(
        StreamTerminalOutputRequestSchema,
        decodeConnectStreamRequestBody(interception.request.body),
      );

      expect((decoded as any).initialRows).to.be.greaterThan(0);
    });
  });

  // -------------------------------------------------------------------------
  // AC3 — Dimensions reflect the actual container, not the hardcoded PTY 220×24
  // -------------------------------------------------------------------------

  it("initial_cols reflects the container width and is not the hardcoded PTY default (220)", () => {
    // Given — 800px wide container. At ~8px/col: ≈100 cols; at ~10px/col: ≈80 cols.
    // In any case, it should not equal 220 (the PTY's hardcoded value).
    cy.mount(<Harness containerWidth={800} containerHeight={400} />);

    cy.wait("@streamTerminalOutput").then((interception) => {
      const decoded = fromBinary(
        StreamTerminalOutputRequestSchema,
        decodeConnectStreamRequestBody(interception.request.body),
      );

      const cols = (decoded as any).initialCols as number | undefined;

      expect(cols).to.be.a("number", "initial_cols must be a number, not undefined");
      expect(cols).to.be.greaterThan(40, "cols should be a reasonable terminal width");
      expect(cols).to.be.lessThan(220, "cols should be narrower than the hardcoded PTY default");
    });
  });
});
