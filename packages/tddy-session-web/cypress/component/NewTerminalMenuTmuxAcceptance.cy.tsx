/**
 * Cypress component acceptance: the "+" menu's "in tmux" option starts an agent tab wrapped in tmux,
 * and the tab strip marks it.
 *
 * PRD: `docs/ft/daemon/session-liveness.md` (`#session-liveness` 2/4)
 *
 * Driven over the deterministic gRPC path: `connectSession` returns an empty `livekitRoom`, so every
 * terminal RPC flows over the daemon client into the in-memory backend, which records each request.
 */

import React from "react";
import { TerminalSessionService } from "tddy-client-web/gen/terminal_session_pb";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aSessionServiceBackend,
  type SessionServiceBackend,
} from "../support/rpc/daemonSessionHostBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";

// ---------------------------------------------------------------------------
// Fixture — one active claude-cli session attached over gRPC
// ---------------------------------------------------------------------------

const SESSION_ID = "tmux-tabs-aaaaaaaa-0000-0000-0000-000000000001";

const SESSION = {
  sessionId: SESSION_ID,
  createdAt: "2026-11-19T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha",
  pid: 90001,
  isActive: true,
  projectId: "proj-tmux-tabs",
  daemonInstanceId: "local",
  pendingElicitation: false,
  sessionType: "claude-cli",
};

/** A host-served session whose daemon runs Muse Code; started terminals are `term-1`, `term-2`, … */
function aBackendRunningMuse(): SessionServiceBackend {
  return aSessionServiceBackend({
    sessions: [SESSION],
    agents: [{ id: "muse", label: "Muse Code" }],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    newTerminalId: (index) => `term-${index + 1}`,
  });
}

function attachSession(backend: SessionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(SESSION_ID).click();
  tabs.tabs().should("exist");
}

/** The kind and wrapper kinds of every `StartTerminalSession` the backend received, in call order. */
function startedTerminals(backend: SessionServiceBackend) {
  return backend
    .callsTo(TerminalSessionService.method.startTerminalSession)
    .map((request) => ({
      kind: request.kind,
      wrappers: request.wrappers.map((wrapper) => ({ kind: wrapper.kind })),
    }));
}

// ---------------------------------------------------------------------------

describe("NewTerminalMenu — agent tabs in tmux", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("adds an agent tab in tmux and marks it", () => {
    // Given a connected session whose daemon runs Muse Code
    const backend = aBackendRunningMuse();
    attachSession(backend);

    // When Muse Code is opened in tmux from the "+" menu
    tabs.newTab().click();
    tabs.newTabInTmuxOption("muse-cli").click();

    // Then the daemon is asked for a Muse Code terminal wrapped in tmux ...
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(startedTerminals(b)).to.deep.equal([
        { kind: "muse-cli", wrappers: [{ kind: "tmux" }] },
      ]);
    });
    // ... whose tab is focused and carries the tmux marker, which the Agent tab does not
    tabs.tab("term-1").should("have.attr", "aria-selected", "true");
    tabs.tmuxMarker("term-1").should("exist");
    tabs.tmuxMarker("main").should("not.exist");
  });
});
