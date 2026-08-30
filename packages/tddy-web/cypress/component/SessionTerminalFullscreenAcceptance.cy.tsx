/**
 * Cypress component acceptance: full screen for a session terminal pane.
 *
 * PRD: `docs/ft/web/session-terminal-tabs.md` § Full screen
 * Changeset: `session-terminal-fullscreen`
 *
 * Driven over the deterministic gRPC path (as `SessionTerminalTabsAcceptance`): `connectSession`
 * returns an empty `livekitRoom`, so the session attaches as `connected-grpc` and every terminal RPC
 * flows over the daemon client into the in-memory backend.
 *
 * The Fullscreen API is stubbed rather than exercised: Cypress' AUT iframe is not guaranteed to
 * carry `allowfullscreen`, and a real `requestFullscreen` needs user activation the runner does not
 * reliably supply. What is pinned here is the contract this feature owns — WHICH element is handed
 * to the API, that the toggle flips to "exit", that the panes survive the transition, and that the
 * mode has a way back. The vendor-prefix fan-out below it is already covered by
 * `src/lib/browserFullscreen.test.ts`.
 *
 * The floating exit control is clicked with `{ force: true }`: this harness mounts components with
 * no stylesheet (`cypress/support/component.ts` imports none), so Tailwind's `absolute` / `z-20`
 * never apply and every pane lays out in flow instead of stacking. That puts the terminal canvas
 * over the button in the test DOM only — in the app the button is `absolute … z-20`, above the
 * terminal's own inline `z-index: 2` live pane.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
} from "../support/rpc/connectionServiceBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";

// ---------------------------------------------------------------------------
// Fixture — a single active local session attached over gRPC
// ---------------------------------------------------------------------------

const SESSION = {
  sessionId: "term-fs-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-08-30T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha",
  pid: 90101,
  isActive: true,
  projectId: "proj-term-fs",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

/** A connected-grpc backend (empty `livekitRoom`) with an optional set of pre-existing bash tabs. */
function aGrpcBackend(
  terminals: Array<{ terminalId: string }> = [],
): ConnectionServiceBackend {
  return aConnectionServiceBackend({
    sessions: [SESSION],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    terminals,
  });
}

/** Attach the session over gRPC and wait for its terminal tab bar to render. */
function attachSession(backend: ConnectionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
  tabs.tabs().should("exist");
}

/**
 * Stub the Fullscreen API on the AUT window and drive `document.fullscreenElement` from the stub, so
 * the component's `fullscreenchange` listener sees the state the browser would have produced.
 * Must run AFTER mount — Cypress stubs window objects post-mount.
 */
function stubFullscreenApi() {
  cy.window().then((win) => {
    const setActive = (el: Element | null) => {
      Object.defineProperty(win.document, "fullscreenElement", {
        configurable: true,
        get: () => el,
      });
      win.document.dispatchEvent(new win.Event("fullscreenchange"));
    };

    cy.stub(win.Element.prototype, "requestFullscreen")
      .as("requestFullscreen")
      .callsFake(function (this: Element) {
        setActive(this);
        return Promise.resolve();
      });

    cy.stub(win.document, "exitFullscreen")
      .as("exitFullscreen")
      .callsFake(() => {
        setActive(null);
        return Promise.resolve();
      });

    setActive(null);
  });
}

// ---------------------------------------------------------------------------

describe("Session terminal panes — full screen", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("offers a full-screen control on the tab strip of a connected session", () => {
    // Given a connected session
    attachSession(aGrpcBackend());

    // Then the tab strip carries a full-screen toggle, in its "enter" state
    tabs.fullscreenToggle()
      .should("exist")
      .and("have.attr", "aria-label", "Enter full screen")
      .and("have.attr", "aria-pressed", "false");

    // ... and nothing is claiming to be full screen yet
    tabs.fullscreenExit().should("not.exist");
  });

  it("hands the active pane's stack — not the tab strip — to the Fullscreen API", () => {
    // Given a connected session with the Agent pane active
    attachSession(aGrpcBackend());
    stubFullscreenApi();

    // When the user asks for full screen
    tabs.fullscreenToggle().click();

    // Then requestFullscreen was called on this runtime's pane stack, so the terminal fills the
    // screen and the tab strip (its sibling) is left behind.
    cy.get("@requestFullscreen").should("have.been.calledOnce");
    tabs.paneStack(SESSION.sessionId).then(($stack) => {
      cy.get("@requestFullscreen").should((subject) => {
        const stub = subject as unknown as sinon.SinonStub;
        expect(stub.firstCall.thisValue).to.equal($stack[0]);
      });
    });
  });

  it("flips the control to 'exit' and surfaces an in-pane way back", () => {
    // Given a session that has entered full screen
    attachSession(aGrpcBackend());
    stubFullscreenApi();
    tabs.fullscreenToggle().click();

    // Then the strip's toggle reads as pressed ...
    tabs.fullscreenToggle()
      .should("have.attr", "aria-label", "Exit full screen")
      .and("have.attr", "aria-pressed", "true");

    // ... and — because the strip itself is outside the fullscreen element — the pane draws its own
    // exit control, which returns to the inline layout.
    tabs.fullscreenExit().should("exist").click({ force: true });

    cy.get("@exitFullscreen").should("have.been.calledOnce");
    tabs.fullscreenExit().should("not.exist");
    tabs.fullscreenToggle().should("have.attr", "aria-pressed", "false");
  });

  it("keeps every terminal of the session mounted across the transition", () => {
    // Given a connected session with a bash terminal open alongside the Agent
    attachSession(aGrpcBackend([{ terminalId: "bash-1" }]));
    tabs.pane("bash-1").should("exist");
    stubFullscreenApi();

    // When it goes full screen and comes back
    tabs.fullscreenToggle().click();
    tabs.pane("main").should("exist");
    tabs.pane("bash-1").should("exist");
    tabs.fullscreenExit().click({ force: true });

    // Then both terminals are still mounted — full screen is a view mode, so no stream is torn down
    tabs.pane("main").should("exist");
    tabs.pane("bash-1").should("exist");
    tabs.agentTab().should("have.attr", "aria-selected", "true");
  });

  it("full-screens whichever pane is active, not always the Agent", () => {
    // Given a connected session switched to its bash terminal
    attachSession(aGrpcBackend([{ terminalId: "bash-1" }]));
    tabs.tab("bash-1").click().should("have.attr", "aria-selected", "true");
    stubFullscreenApi();

    // When the user asks for full screen
    tabs.fullscreenToggle().click();

    // Then the same pane stack is handed over (only one pane is ever visible, so the visible pane is
    // the bash one) and the bash tab stays the selected one.
    cy.get("@requestFullscreen").should("have.been.calledOnce");
    tabs.tab("bash-1").should("have.attr", "aria-selected", "true");
    tabs.pane("bash-1").should("exist");
  });
});
