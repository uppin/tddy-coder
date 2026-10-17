/**
 * The PTY follows the screen that drives it, and only that screen.
 *
 * The daemon resizes a session's PTY only for the screen holding its control lease: a screen that
 * merely watches (a phone opening a session the desktop drives) must not reflow it. That puts two
 * duties on the terminal:
 *
 * - it presents the lease it holds when it opens the output stream, so the daemon can honour the
 *   grid it measured when — and only when — this screen is the driver;
 * - it re-sends its grid each time it gains the lease. A grid reported while another screen drove
 *   the session never took effect, and without the re-send the PTY stays at the previous driver's
 *   size: the TUI is laid out for a grid this screen is not showing, and every tap lands on the
 *   wrong cell.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  SendTerminalInputResponseSchema,
  TerminalSessionService,
} from "../../src/gen/terminal_session_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import type { ConnectedSession } from "../../src/components/sessions/useTerminalControl";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";

const SESSION_ID = "handover-session-aaaa-0000-0000-0000-000000000001";
const SESSION_TOKEN = "handover-session-token";
const FIRST_LEASE = "lease-before-the-desktop-took-over";
const SECOND_LEASE = "lease-after-claiming-back";

type TerminalClient = ReturnType<typeof createClient<typeof TerminalSessionService>>;

const RESIZE_OSC = /^\x1b\]resize;\d+;\d+\x07$/;

function aRecordingBackend() {
  const sent: { controlToken: string; text: string }[] = [];
  const opens: { controlToken: string }[] = [];
  const backend = anInMemoryRpcBackend().implement(TerminalSessionService, {
    streamTerminalOutput: async function* (req: { controlToken: string }) {
      opens.push({ controlToken: req.controlToken });
      // Stay open, like a live terminal: an ended stream would tear the pane down.
      await new Promise<void>(() => undefined);
    },
    sendTerminalInput: async (req: { controlToken: string; data: Uint8Array }) => {
      sent.push({ controlToken: req.controlToken, text: new TextDecoder().decode(req.data) });
      return create(SendTerminalInputResponseSchema, {});
    },
  });
  return { backend, sent, opens };
}

function Pane({ client, controlToken }: { client: TerminalClient; controlToken: string }) {
  const connected: ConnectedSession = { sessionId: SESSION_ID, controlToken };
  return (
    <div style={{ width: 800, height: 400, position: "relative" }}>
      <UploadProgressProvider>
        <GrpcSessionTerminal
          sessionId={SESSION_ID}
          sessionToken={SESSION_TOKEN}
          client={client}
          connected={connected}
        />
      </UploadProgressProvider>
    </div>
  );
}

/** A pane whose lease the test can hand over in place — the way a claim reaches a mounted
 *  terminal — rather than by re-mounting it, which would open a second stream. */
function HandoverPane({
  client,
  initialLease,
  onHandover,
}: {
  client: TerminalClient;
  initialLease: string;
  onHandover: (handTo: (lease: string) => void) => void;
}) {
  const [lease, setLease] = React.useState(initialLease);
  React.useEffect(() => onHandover(setLease), [onHandover]);
  return <Pane client={client} controlToken={lease} />;
}

describe("GrpcSessionTerminal — control handover", () => {
  it("presents the lease it holds when it opens the output stream", () => {
    // Given — a screen that holds the session's lease
    const { backend, opens } = aRecordingBackend();
    const client = createClient(TerminalSessionService, backend.transport());

    // When — its terminal opens
    cy.mount(<Pane client={client} controlToken={FIRST_LEASE} />);

    // Then — the open carries the lease, so the daemon honours the grid it measured
    cy.wrap(opens).should((o) => {
      expect(o, "one output stream opened").to.have.length(1);
      expect(o[0].controlToken).to.equal(FIRST_LEASE);
    });
  });

  it("re-sends its grid under the new lease when it claims control back", () => {
    // Given — a screen that drove the session and reported its grid under its first lease
    const { backend, sent, opens } = aRecordingBackend();
    const client = createClient(TerminalSessionService, backend.transport());
    let handTo: (lease: string) => void = () => undefined;
    cy.mount(
      <HandoverPane
        client={client}
        initialLease={FIRST_LEASE}
        onHandover={(setLease) => {
          handTo = setLease;
        }}
      />,
    );
    cy.wrap(sent).should((s) => {
      expect(
        s.some((c) => c.controlToken === FIRST_LEASE && RESIZE_OSC.test(c.text)),
        "grid reported under the first lease",
      ).to.be.true;
    });

    // ...and whose layout has settled, so no further grid report is on its way by itself
    cy.wait(500);
    let sentBeforeHandover = 0;
    cy.then(() => {
      sentBeforeHandover = sent.length;
    });

    // When — another screen took over, and this one claims control back with a fresh lease
    cy.then(() => handTo(SECOND_LEASE));

    // Then — its grid goes out again under the new lease, so the PTY is resized back to it
    cy.wrap(sent).should((s) => {
      expect(
        s
          .slice(sentBeforeHandover)
          .some((c) => c.controlToken === SECOND_LEASE && RESIZE_OSC.test(c.text)),
        "grid re-sent under the new lease",
      ).to.be.true;
      expect(opens, "the handover reuses the open stream").to.have.length(1);
    });
  });
});
