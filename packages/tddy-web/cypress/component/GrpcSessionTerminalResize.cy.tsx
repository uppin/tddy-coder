/**
 * Acceptance tests for terminal dimension broadcasting in GrpcSessionTerminal.
 *
 * Bug: PTY spawns at hardcoded 220×24 (claude_cli_session.rs DEFAULT_TERM_COLS/ROWS).
 * The frontend fires an OSC resize after the terminal mounts, but by then the daemon has
 * already started replaying buffered output drawn at 220 cols — producing garbled output
 * in narrower containers.
 *
 * Fix: GrpcSessionTerminal must pass the container's measured cols×rows in the opening message of
 * the bidi `StreamSessionTerminalIO` call (the one that asks for `mode: SNAPSHOT`) so the daemon can
 * resize the PTY *before* it serializes the screen.
 *
 * These tests define the expected behavior:
 * - the opening message asks for a SNAPSHOT and carries initial_cols > 0 and initial_rows > 0
 * - The dimensions reflect the actual container size, not the hardcoded 220×24 PTY default
 *
 * The browser's connect-web transport cannot carry a bidi call over HTTP, so the daemon is an
 * in-memory `TerminalSessionService` that records the opening message of every open.
 */

import React from "react";
import { createClient, type Client } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  StreamReplayMode,
  TerminalSessionService,
  type SessionTerminalInput,
} from "../../src/gen/terminal_session_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";

type TerminalClient = Client<typeof TerminalSessionService>;

// ---------------------------------------------------------------------------
// Backend double — a daemon whose bidi terminal stream records each open's first message
// ---------------------------------------------------------------------------

function aRecordingTerminalDaemon() {
  const openingMessages: SessionTerminalInput[] = [];
  const backend = anInMemoryRpcBackend().implement(TerminalSessionService, {
    async *streamSessionTerminalIO(requests: AsyncIterable<SessionTerminalInput>) {
      const reader = requests[Symbol.asyncIterator]();
      const first = await reader.next();
      if (first.done) return;
      openingMessages.push(first.value);
      // Drain later messages (the resize OSC, keystrokes) and stay open: an empty, silent stream.
      void (async () => {
        for (let next = await reader.next(); !next.done; next = await reader.next()) {
          /* accepted */
        }
      })();
      await new Promise<never>(() => undefined);
    },
  });
  const client: TerminalClient = createClient(TerminalSessionService, backend.transport());
  return { client, openingMessages };
}

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

function Harness({
  client,
  containerWidth = 800,
  containerHeight = 400,
}: {
  client: TerminalClient;
  containerWidth?: number;
  containerHeight?: number;
}) {
  return (
    <div style={{ width: containerWidth, height: containerHeight, position: "relative" }}>
      <UploadProgressProvider>
        <GrpcSessionTerminal
          sessionId="resize-test-session-aabbcc"
          sessionToken="test-token"
          client={client}
          connected={null}
        />
      </UploadProgressProvider>
    </div>
  );
}

/** Yields the opening message of the first open once it has arrived. */
function waitForOpeningMessage(daemon: ReturnType<typeof aRecordingTerminalDaemon>) {
  return cy
    .wrap(daemon, { timeout: 10000 })
    .should((d) => {
      expect(d.openingMessages.length, "the bidi terminal stream was opened").to.be.greaterThan(0);
    })
    .then((d) => d.openingMessages[0]);
}

// ---------------------------------------------------------------------------
// AC1–AC3: the opening message must include initial terminal dimensions
// ---------------------------------------------------------------------------

describe("GrpcSessionTerminalResize — initial dimensions in the opening StreamSessionTerminalIO message", () => {
  // -------------------------------------------------------------------------
  // AC1 — opening message asks for a snapshot and includes initial_cols > 0
  // -------------------------------------------------------------------------

  it("includes initial_cols > 0 in the opening SNAPSHOT message on mount", () => {
    // Given — 800px wide container
    const daemon = aRecordingTerminalDaemon();
    cy.mount(<Harness client={daemon.client} containerWidth={800} containerHeight={400} />);

    // When — component mounts and opens the terminal stream
    waitForOpeningMessage(daemon).then((opening) => {
      expect(opening.mode, "the open asks for a snapshot").to.equal(StreamReplayMode.SNAPSHOT);
      expect(opening.initialCols).to.be.greaterThan(0);
    });
  });

  // -------------------------------------------------------------------------
  // AC2 — opening message includes initial_rows > 0
  // -------------------------------------------------------------------------

  it("includes initial_rows > 0 in the opening SNAPSHOT message on mount", () => {
    const daemon = aRecordingTerminalDaemon();
    cy.mount(<Harness client={daemon.client} containerWidth={800} containerHeight={400} />);

    waitForOpeningMessage(daemon).then((opening) => {
      expect(opening.mode, "the open asks for a snapshot").to.equal(StreamReplayMode.SNAPSHOT);
      expect(opening.initialRows).to.be.greaterThan(0);
    });
  });

  // -------------------------------------------------------------------------
  // AC3 — Dimensions reflect the actual container, not the hardcoded PTY 220×24
  // -------------------------------------------------------------------------

  it("initial_cols reflects the container width and is not the hardcoded PTY default (220)", () => {
    // Given — 800px wide container. At ~8px/col: ≈100 cols; at ~10px/col: ≈80 cols.
    // In any case, it should not equal 220 (the PTY's hardcoded value).
    const daemon = aRecordingTerminalDaemon();
    cy.mount(<Harness client={daemon.client} containerWidth={800} containerHeight={400} />);

    waitForOpeningMessage(daemon).then((opening) => {
      const cols = opening.initialCols as number | undefined;

      expect(cols).to.be.a("number", "initial_cols must be a number, not undefined");
      expect(cols).to.be.greaterThan(40, "cols should be a reasonable terminal width");
      expect(cols).to.be.lessThan(220, "cols should be narrower than the hardcoded PTY default");
    });
  });
});
