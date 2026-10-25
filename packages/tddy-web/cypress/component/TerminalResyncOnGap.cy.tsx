/**
 * Acceptance: a session terminal never paints across a gap in its stream — it asks for a fresh
 * snapshot on the same stream instead.
 *
 * Feature: docs/ft/daemon/terminal-sessions.md (Resync at any point)
 *
 * Driven through the real `GrpcSessionTerminal` over an in-memory `TerminalSessionService` whose bidi
 * `streamSessionTerminalIO` records every message of every open and lets the test push frames.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient, type Client } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  SessionTerminalOutputSchema,
  TerminalSessionService,
  type SessionTerminalInput,
  type SessionTerminalOutput,
} from "../../src/gen/terminal_session_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { byTestId, TEST_IDS } from "../support/testIds";

const SESSION_ID = "01900000-0000-7000-8000-000000resync";
const bytes = (text: string) => new TextEncoder().encode(text);

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
  const client: Client<typeof TerminalSessionService> = createClient(TerminalSessionService, backend.transport());
  return { client, opens };
}

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

function aResyncingTerminal() {
  const daemon = aBidiTerminalDaemon();
  const theOpen = () =>
    cy
      .wrap(daemon, { timeout: 10000 })
      .should((d) => {
        expect(d.opens[0]?.messages.length ?? 0, "the bidi terminal stream opened").to.be.greaterThan(0);
      })
      .then((d) => d.opens[0]);

  const driver = {
    mount() {
      cy.viewport(1280, 800);
      cy.mount(
        <div style={{ width: 800, height: 400, position: "relative" }}>
          <UploadProgressProvider>
            <GrpcSessionTerminal
              sessionId={SESSION_ID}
              sessionToken="resync-token"
              client={daemon.client}
              connected={{ sessionId: SESSION_ID, controlToken: "lease-1" }}
            />
          </UploadProgressProvider>
        </div>,
      );
      return driver;
    },
    daemonSends(frame: SessionTerminalOutput) {
      theOpen().then((open) => open.push(frame));
      return driver;
    },
    expectScreenShows(text: string) {
      byTestId(TEST_IDS.terminalBufferText, { timeout: 10000 }).should("contain.text", text);
      return driver;
    },
    expectScreenDoesNotShow(text: string) {
      byTestId(TEST_IDS.terminalBufferText).should("not.contain.text", text);
      return driver;
    },
    /** A snapshot was requested on the stream that is already open — not by reconnecting. */
    expectASnapshotRequestedOnTheSameStream() {
      cy.wrap(daemon, { timeout: 10000 }).should((d) => {
        expect(
          d.opens[0].messages.slice(1).some((m) => m.requestSnapshot),
          "a later message on the open stream asks for a snapshot",
        ).to.equal(true);
        expect(d.opens, "no second open").to.have.lengthOf(1);
      });
      return driver;
    },
  };
  return driver;
}

describe("Session terminal — resync on a gap", () => {
  it("does not paint across a gap and asks for a snapshot on the same stream", () => {
    aResyncingTerminal()
      // Given — a terminal showing screen A and one contiguous delta on top of it
      .mount()
      .daemonSends(aSnapshot("\x1b[1;1HSCREEN-A", 100n))
      .daemonSends(aDelta("\x1b[2;1Hcontiguous", 100n))
      .expectScreenShows("contiguous")
      // When — a delta arrives that does not start where the last one ended
      .daemonSends(aDelta("\x1b[3;1Hafter-a-hole", 500n))
      // Then — it is not painted, and a snapshot is asked for on the open stream
      .expectASnapshotRequestedOnTheSameStream()
      .expectScreenDoesNotShow("after-a-hole")
      // When — the daemon answers with the screen as it is now
      .daemonSends(aSnapshot("\x1b[1;1HSCREEN-B resynced", 640n))
      // Then — the resynced screen shows
      .expectScreenShows("SCREEN-B resynced");
  });
});
