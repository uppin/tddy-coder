/**
 * Cypress component acceptance: a terminal's OSC 52 copy reaches the viewer's clipboard.
 *
 * Any program in a session terminal that prints an OSC 52 clipboard write — tmux copy-mode, Neovim,
 * Claude Code — puts its text on the clipboard of the machine the viewer is on. Reads are refused
 * and never answered. Both feed paths are covered: the agent terminal (`GhosttyTerminalSession` on
 * a feed) and a bash tab (`GrpcSessionTerminal` over the terminal RPC stream).
 *
 * Reference: packages/tddy-web/docs/terminal-osc52-clipboard.md (`#session-tabs 4/7`)
 *
 * `navigator.clipboard.writeText` is stubbed: the stub is the viewer's clipboard.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient, type Client } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  TerminalSessionService,
  type SessionTerminalInput,
  SessionTerminalOutputSchema,
} from "tddy-client-web/gen/terminal_session_pb";
import { GrpcSessionTerminal } from "../../src/GrpcSessionTerminal";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { aGhosttyTerminal } from "../support/drivers/ghosttyTerminalDriver";
import { aGhosttyTerminalSession } from "../support/drivers/ghosttyTerminalSessionDriver";
import { byTestId, TEST_IDS } from "../support/testIds";

const ESC = "\x1b";
const BEL = "\x07";

function osc52Write(text: string): string {
  return `${ESC}]52;c;${btoa(text)}${BEL}`;
}

const OSC52_READ = `${ESC}]52;c;?${BEL}`;

/** Stub the viewer's clipboard before anything is mounted. */
function stubTheClipboard() {
  cy.window().then((win) => {
    cy.stub(win.navigator.clipboard, "writeText").as("clipboardWrite").resolves(undefined);
  });
}

// ---------------------------------------------------------------------------
// A bash tab: `GrpcSessionTerminal` over an in-memory daemon whose stream opens with `opening`
// ---------------------------------------------------------------------------

const SESSION_ID = "osc52-session-aa11";

function aDaemonWhoseTerminalPrints(opening: string) {
  const inputs: SessionTerminalInput[] = [];
  const backend = anInMemoryRpcBackend().implement(TerminalSessionService, {
    async *streamSessionTerminalIO(requests: AsyncIterable<SessionTerminalInput>) {
      const reader = requests[Symbol.asyncIterator]();
      const first = await reader.next();
      if (first.done) return;
      void (async () => {
        for (let next = await reader.next(); !next.done; next = await reader.next()) {
          inputs.push(next.value);
        }
      })();
      yield create(SessionTerminalOutputSchema, {
        data: new TextEncoder().encode(opening),
        snapshot: true,
        endOffset: BigInt(opening.length),
        atOldest: true,
        sessionId: SESSION_ID,
        terminalId: "bash-1",
      });
      await new Promise<never>(() => undefined);
    },
  });
  const client: Client<typeof TerminalSessionService> = createClient(
    TerminalSessionService,
    backend.transport(),
  );
  return { client, inputs };
}

function mountABashTab(client: Client<typeof TerminalSessionService>) {
  cy.mount(
    <div style={{ width: 800, height: 400, position: "relative" }}>
      <UploadProgressProvider>
        <GrpcSessionTerminal
          sessionId={SESSION_ID}
          sessionToken="osc52-token"
          client={client}
          connected={null}
          terminalId="bash-1"
        />
      </UploadProgressProvider>
    </div>,
  );
}

// ---------------------------------------------------------------------------

describe("A terminal's OSC 52 copy reaches the viewer's clipboard", () => {
  beforeEach(() => {
    cy.viewport(1000, 600);
    stubTheClipboard();
  });

  it("an OSC 52 write on the agent terminal copies its text to the clipboard", () => {
    // Given the agent terminal
    const terminal = aGhosttyTerminalSession().mount();

    // When its program prints a clipboard write
    terminal.deliver(`copying${osc52Write("from-the-agent")}done\r\n`);

    // Then the viewer's clipboard holds the text
    cy.get("@clipboardWrite").should("have.been.calledOnceWith", "from-the-agent");
  });

  it("an OSC 52 write on a bash tab copies its text to the clipboard", () => {
    // Given a bash tab whose program prints a clipboard write (tmux copy-mode, say)
    const daemon = aDaemonWhoseTerminalPrints(`$ ${osc52Write("from-tmux")}\r\n`);

    // When it is mounted
    mountABashTab(daemon.client);

    // Then the viewer's clipboard holds the text
    cy.get("@clipboardWrite").should("have.been.calledOnceWith", "from-tmux");
  });

  it("an OSC 52 read writes nothing and sends no input", () => {
    // Given the agent terminal
    const terminal = aGhosttyTerminalSession().mount();

    // When its program asks to read the clipboard, then prints a marker
    terminal.deliver(`${OSC52_READ}after-the-read\r\n`);
    terminal.expectTerminalShows("after-the-read");

    // Then nothing was copied and nothing carrying OSC 52 was sent back
    cy.get("@clipboardWrite").should("not.have.been.called");
    cy.wrap(null).should(() => {
      const sent = terminal.sent.map((chunk) => new TextDecoder().decode(chunk));
      expect(sent.filter((chunk) => chunk.includes("]52;"))).to.deep.equal([]);
    });
  });

  it("the output around an OSC 52 sequence still renders", () => {
    // Given the agent terminal
    const terminal = aGhosttyTerminalSession().mount();

    // When a clipboard write arrives between two words
    terminal.deliver(`before-the-copy${osc52Write("x")}after-the-copy\r\n`);

    // Then both words are painted
    byTestId(TEST_IDS.terminalBufferText, { timeout: 20000 })
      .should("contain.text", "before-the-copy")
      .and("contain.text", "after-the-copy");
    cy.get("@clipboardWrite").should("have.been.calledOnceWith", "x");
  });

  it("an OSC 52 write inside the initial content copies exactly once", () => {
    // Given initial content that carries a clipboard write
    const terminal = aGhosttyTerminal({
      initialContent: `before${osc52Write("from-initial-content")}after-initial-copy\r\n`,
      withHandleCapture: true,
    });

    // When the terminal is mounted and has painted that content
    terminal.mount().expectExists().expectBufferHoldsTimes("after-initial-copy", 1);

    // Then the viewer's clipboard was written once
    cy.get("@clipboardWrite").should("have.been.calledOnceWith", "from-initial-content");
  });
});
