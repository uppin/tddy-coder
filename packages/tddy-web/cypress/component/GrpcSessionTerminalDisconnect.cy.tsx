/**
 * Behaviour spec: the Sessions-screen terminal (GrpcSessionTerminal) must NOT
 * carry its own top bar / Disconnect control. Disconnect happens automatically
 * when the terminal component unmounts (i.e. when the user switches sessions).
 *
 * Expected:
 *   1. GrpcSessionTerminal renders no terminal connection top bar and no
 *      disconnect affordance.
 *   2. GrpcSessionTerminal invokes `onDisconnect` exactly once when it unmounts,
 *      without the user clicking any control.
 *
 * Both fail today: GrpcSessionTerminal always passes `connectionOverlay` (so the
 * status bar with the Disconnect menu renders), and its unmount cleanup only
 * marks the stream closed — it never calls `onDisconnect`.
 */

import React, { useMemo, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { TerminalSessionService } from "../../src/gen/terminal_session_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { aTerminalSessionServiceBackend } from "../support/rpc/terminalSessionServiceBackend";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "disconnect-test-session-9f8e";
const SESSION_TOKEN = "test-session-token-disconnect";

/** data-testid for the harness button that unmounts the terminal (simulates a session switch). */
const SWITCH_AWAY_BTN = "sessions-terminal-switch-away";

// ---------------------------------------------------------------------------
// Backend doubles
// ---------------------------------------------------------------------------

/**
 * A daemon whose bidi `StreamSessionTerminalIO` stays open for the lifetime of the test (the shared
 * in-memory terminal fake never drops a stream unless told to) — so the terminal is "connected" and
 * `onDisconnect` can never fire from the stream ending. The only way it can fire is on unmount.
 * Input on the stream (the initial resize) is always accepted. In memory because connect-web cannot
 * carry a bidi call over HTTP.
 */
function aPendingTerminalDaemon() {
  return aTerminalSessionServiceBackend();
}

// ---------------------------------------------------------------------------
// Fluent driver
// ---------------------------------------------------------------------------

function SessionsTerminalHarness({ onDisconnect }: { onDisconnect: () => void }) {
  const [mounted, setMounted] = useState(true);
  const client = useMemo(
    () => createClient(TerminalSessionService, aPendingTerminalDaemon().backend.transport()),
    [],
  );

  return (
    <div style={{ width: 800, height: 400, position: "relative" }}>
      <button type="button" data-testid={SWITCH_AWAY_BTN} onClick={() => setMounted(false)}>
        switch away
      </button>
      {mounted && (
        <UploadProgressProvider>
          <GrpcSessionTerminal
            sessionId={SESSION_ID}
            sessionToken={SESSION_TOKEN}
            client={client}
            connected={null}
            onDisconnect={onDisconnect}
          />
        </UploadProgressProvider>
      )}
    </div>
  );
}

function aSessionsScreenTerminal() {
  const onDisconnect = cy.stub().as("onDisconnect");

  const driver = {
    mount() {
      cy.mount(<SessionsTerminalHarness onDisconnect={onDisconnect} />);
      return driver;
    },
    expectTerminalVisible() {
      byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");
      return driver;
    },
    expectNoDisconnectTopBar() {
      byTestId(TEST_IDS.terminalConnectionStatusBar).should("not.exist");
      byTestId(TEST_IDS.connectionStatusDot).should("not.exist");
      return driver;
    },
    expectStillConnected() {
      cy.get("@onDisconnect").should("not.have.been.called");
      return driver;
    },
    switchAwayFromSession() {
      byTestId(SWITCH_AWAY_BTN).click();
      return driver;
    },
    expectTerminalGone() {
      byTestId(TEST_IDS.ghosttyTerminal).should("not.exist");
      return driver;
    },
    expectDisconnectedAutomatically() {
      cy.get("@onDisconnect").should("have.been.calledOnce");
      return driver;
    },
  };

  return driver;
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("Sessions-screen terminal — no top bar, auto-disconnect on unmount", () => {
  it("renders no terminal top bar or disconnect control", () => {
    // Given — a connected sessions-screen terminal whose output stream stays open
    // When — it mounts
    aSessionsScreenTerminal()
      .mount()
      .expectTerminalVisible()
      // Then — there is no top bar and no disconnect affordance
      .expectNoDisconnectTopBar();
  });

  it("disconnects automatically when the terminal unmounts as the session switches", () => {
    // Given — a connected sessions-screen terminal whose output stream stays open
    const terminal = aSessionsScreenTerminal().mount().expectTerminalVisible();

    // Given — no disconnect has happened while the terminal is mounted
    terminal.expectStillConnected();

    // When — the session switches and the terminal unmounts
    terminal.switchAwayFromSession().expectTerminalGone();

    // Then — disconnect fired automatically, with no manual control clicked
    terminal.expectDisconnectedAutomatically();
  });
});
