/**
 * Acceptance: a session terminal that (re)connects asks for a snapshot of the current screen and
 * shows it at once — no resize, no waiting for the application to repaint.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-10-25-terminal-snapshot-host.md
 *
 * Driven through the real `GrpcSessionTerminal` (the host-served Agent and bash panes) over an
 * in-memory `TerminalSessionService` whose bidi `streamSessionTerminalIO` records every message of
 * every open and lets the test push the frames the daemon would send. Three ways a terminal
 * (re)connects are covered: the first mount, a remount after the runtime was evicted (a brand-new
 * ghostty instance that saw nothing), and a transport blip that the terminal survives.
 */

import React, { useState } from "react";
import { create } from "@bufbuild/protobuf";
import { createClient, type Client } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  SessionTerminalOutputSchema,
  StreamReplayMode,
  TerminalSessionService,
  type SessionTerminalInput,
  type SessionTerminalOutput,
} from "../../src/gen/terminal_session_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { byTestId, TEST_IDS } from "../support/testIds";

const SESSION_ID = "01900000-0000-7000-8000-00000000snap";
const SESSION_TOKEN = "snapshot-test-token";
const CONTROL_TOKEN = "lease-1";
const REMOUNT_BTN = "snapshot-test-remount";
const BLIP_BTN = "snapshot-test-blip";
const RESTORE_BTN = "snapshot-test-restore";

type TerminalClient = Client<typeof TerminalSessionService>;

const bytes = (text: string) => new TextEncoder().encode(text);

// ---------------------------------------------------------------------------
// A daemon that serves the bidi terminal stream, in memory
// ---------------------------------------------------------------------------

/** One open of `streamSessionTerminalIO`: what the client sent on it, and a way to answer. */
interface OpenStream {
  readonly messages: SessionTerminalInput[];
  push(frame: SessionTerminalOutput): void;
}

function aBidiTerminalDaemon() {
  const opens: OpenStream[] = [];
  const backend = anInMemoryRpcBackend().implement(TerminalSessionService, {
    async *streamSessionTerminalIO(requests: AsyncIterable<SessionTerminalInput>) {
      const queued: SessionTerminalOutput[] = [];
      let wake: (() => void) | null = null;
      const open: OpenStream = {
        messages: [],
        push(frame) {
          queued.push(frame);
          wake?.();
        },
      };
      opens.push(open);
      void (async () => {
        for await (const message of requests) open.messages.push(message);
      })();
      for (;;) {
        while (queued.length > 0) yield queued.shift() as SessionTerminalOutput;
        await new Promise<void>((resolve) => {
          wake = resolve;
        });
        wake = null;
      }
    },
  });
  const client: TerminalClient = createClient(TerminalSessionService, backend.transport());
  return { client, opens };
}

/** A snapshot frame: one repaint, beginning with RIS, at sequence point `endOffset`. */
function aSnapshot(screen: string, endOffset: bigint): SessionTerminalOutput {
  return create(SessionTerminalOutputSchema, {
    data: bytes(`\x1bc${screen}`),
    snapshot: true,
    endOffset,
    snapshotRows: 24,
    snapshotCols: 80,
    sessionId: SESSION_ID,
    terminalId: "main",
  });
}

/** A live delta, stamped with the offsets it occupies in the terminal's output. */
function aDelta(text: string, startOffset: bigint): SessionTerminalOutput {
  const data = bytes(text);
  return create(SessionTerminalOutputSchema, {
    data,
    startOffset,
    endOffset: startOffset + BigInt(data.length),
    sessionId: SESSION_ID,
    terminalId: "main",
  });
}

// ---------------------------------------------------------------------------
// Fluent driver
// ---------------------------------------------------------------------------

function aSessionTerminal() {
  const daemon = aBidiTerminalDaemon();

  function Harness() {
    const [generation, setGeneration] = useState(0);
    const [client, setClient] = useState<TerminalClient | null>(daemon.client);
    return (
      <div style={{ width: 800, height: 400, position: "relative" }}>
        <button type="button" data-testid={REMOUNT_BTN} onClick={() => setGeneration((g) => g + 1)}>
          remount
        </button>
        <button type="button" data-testid={BLIP_BTN} onClick={() => setClient(null)}>
          blip
        </button>
        <button type="button" data-testid={RESTORE_BTN} onClick={() => setClient(daemon.client)}>
          restore
        </button>
        <UploadProgressProvider>
          <GrpcSessionTerminal
            key={generation}
            sessionId={SESSION_ID}
            sessionToken={SESSION_TOKEN}
            client={client}
            connected={{ sessionId: SESSION_ID, controlToken: CONTROL_TOKEN }}
          />
        </UploadProgressProvider>
      </div>
    );
  }

  /** Open number `n` (0-based) of the bidi stream, once its opening message has arrived. */
  const openNumbered = (n: number) =>
    cy
      .wrap(daemon, { timeout: 10000 })
      .should((d) => {
        expect(
          d.opens[n]?.messages.length ?? 0,
          `open #${n + 1} of the bidi terminal stream has its opening message`,
        ).to.be.greaterThan(0);
      })
      .then((d) => d.opens[n]);

  const driver = {
    mount() {
      cy.viewport(1280, 800);
      cy.mount(<Harness />);
      byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should(
        "have.attr",
        "data-terminal-ready",
        "true",
      );
      return driver;
    },
    /** Open number `n` (1-based) asked for a snapshot of the current screen, at a measured grid. */
    expectOpenAsksForASnapshot(n: number) {
      openNumbered(n - 1).then((open) => {
        const opening = open.messages[0];
        expect(opening.mode, `open #${n} asks for a snapshot`).to.equal(StreamReplayMode.SNAPSHOT);
        expect(opening.initialCols, "the grid is measured before the open").to.be.greaterThan(0);
        expect(opening.initialRows, "the grid is measured before the open").to.be.greaterThan(0);
      });
      return driver;
    },
    daemonSends(n: number, frame: SessionTerminalOutput) {
      openNumbered(n - 1).then((open) => open.push(frame));
      return driver;
    },
    /** The live terminal's parsed screen (its readable mirror; the canvas itself is WebGL). */
    expectScreenShows(text: string) {
      byTestId(TEST_IDS.terminalBufferText, { timeout: 10000 }).should("contain.text", text);
      return driver;
    },
    expectScreenDoesNotShow(text: string) {
      byTestId(TEST_IDS.terminalBufferText).should("not.contain.text", text);
      return driver;
    },
    evictAndRemount() {
      byTestId(REMOUNT_BTN).click();
      byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should(
        "have.attr",
        "data-terminal-ready",
        "true",
      );
      return driver;
    },
    blipTheTransport() {
      byTestId(BLIP_BTN).click();
      return driver;
    },
    restoreTheTransport() {
      byTestId(RESTORE_BTN).click();
      return driver;
    },
    type(text: string) {
      byTestId(TEST_IDS.ghosttyTerminal).click().type(text);
      return driver;
    },
    /** Every keystroke travelled on open `n` itself, each message carrying the running byte total. */
    expectKeystrokesOnTheSameStream(n: number, typed: string) {
      cy.wrap(null).should(() => {
        const open = daemon.opens[n - 1];
        const sent = open.messages
          .slice(1)
          .map((m) => new TextDecoder().decode(m.data))
          .join("");
        expect(sent, `keystrokes on open #${n}`).to.contain(typed);
        let total = 0;
        for (const message of open.messages.slice(1)) {
          total += message.data.length;
          expect(Number(message.inputOffset), "a cumulative input offset").to.equal(total);
        }
      });
      cy.then(() => expect(daemon.opens, "no second stream for input").to.have.lengthOf(n));
      return driver;
    },
  };
  return driver;
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("Session terminal — snapshot on (re)connect", () => {
  it("asks for a snapshot on every open and shows the screen without a resize", () => {
    aSessionTerminal()
      // Given — a session terminal opening for the first time
      .mount()
      // Then — it asks the daemon for the current screen
      .expectOpenAsksForASnapshot(1)
      // When — the daemon answers with a snapshot of screen A
      .daemonSends(1, aSnapshot("\x1b[1;1HSCREEN-A header\x1b[12;5Hbody of A", 120n))
      // Then — screen A is on display at once
      .expectScreenShows("SCREEN-A header")
      // When — the runtime is evicted and the terminal remounts: a fresh VT that saw nothing
      .evictAndRemount()
      // Then — the new open asks for a snapshot too, never a byte resume
      .expectOpenAsksForASnapshot(2)
      // When — the daemon answers with the screen as it is now
      .daemonSends(2, aSnapshot("\x1b[1;1HSCREEN-B header\x1b[12;5Hbody of B", 480n))
      // Then — screen B shows, from the snapshot alone
      .expectScreenShows("SCREEN-B header")
      .expectScreenShows("body of B")
      .expectScreenDoesNotShow("SCREEN-A")
      // When — the application draws one more change after the snapshot's sequence point
      .daemonSends(2, aDelta("\x1b[13;5Hdelta after B", 480n))
      // Then — it lands on top of the snapshot
      .expectScreenShows("delta after B");
  });

  it("asks for a snapshot again when the transport comes back after a blip", () => {
    aSessionTerminal()
      // Given — a terminal showing screen A
      .mount()
      .daemonSends(1, aSnapshot("\x1b[1;1HSCREEN-A", 64n))
      .expectScreenShows("SCREEN-A")
      // When — the transport drops and returns
      .blipTheTransport()
      .restoreTheTransport()
      // Then — the reconnect asks for a snapshot, and shows what it is answered with
      .expectOpenAsksForASnapshot(2)
      .daemonSends(2, aSnapshot("\x1b[1;1HSCREEN-C after the blip", 900n))
      .expectScreenShows("SCREEN-C after the blip");
  });

  it("sends keystrokes on the same bidi stream with cumulative input offsets", () => {
    aSessionTerminal()
      // Given — an open terminal showing a prompt
      .mount()
      .daemonSends(1, aSnapshot("\x1b[1;1H$ ", 10n))
      .expectScreenShows("$")
      // When — the user types
      .type("ls -la")
      // Then — every keystroke went up the stream the output comes down, offsets cumulative
      .expectKeystrokesOnTheSameStream(1, "ls -la");
  });
});
