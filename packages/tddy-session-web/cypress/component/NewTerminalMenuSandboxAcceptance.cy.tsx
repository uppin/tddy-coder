/**
 * Cypress component acceptance: a sandboxed session's "+" menu offers its shell "In sandbox" — in a
 * jail of its own, first and so the default — and "On host"; a session that is not sandboxed offers
 * no such choice. Its agents are offered "In sandbox" too (`#session-liveness` 7/8).
 *
 * PRD: `docs/ft/daemon/session-liveness.md (A sandboxed session's tabs)` (`#session-liveness` 5/8)
 *
 * Driven over the deterministic gRPC path: `connectSession` returns an empty `livekitRoom`, so every
 * terminal RPC flows over the daemon client into the in-memory backend, which records each request.
 */

import React from "react";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
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

const SESSION_ID = "sandbox-tabs-aaaaaaaa-0000-0000-0000-000000000001";

function aSession(sandbox: boolean): Partial<SessionEntry> {
  return {
    sessionId: SESSION_ID,
    createdAt: "2026-11-21T09:00:00Z",
    status: "active",
    repoPath: "/home/dev/feature-sandboxed",
    pid: 90011,
    isActive: true,
    projectId: "proj-sandbox-tabs",
    daemonInstanceId: "local",
    pendingElicitation: false,
    sessionType: "claude-cli",
    sandbox,
  };
}

/** A host-served session; started terminals are `term-1`, `term-2`, … */
function aBackendServing(session: Partial<SessionEntry>): SessionServiceBackend {
  return aSessionServiceBackend({
    sessions: [session],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    newTerminalId: (index) => `term-${index + 1}`,
  });
}

/** A host-served session whose daemon runs Muse Code; started terminals are `term-1`, `term-2`, … */
function aBackendRunningMuseServing(session: Partial<SessionEntry>): SessionServiceBackend {
  return aSessionServiceBackend({
    sessions: [session],
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

/** The kind and wrapper kinds of every `StartTerminalSession` the backend received, in call order.
 *  The login shell goes on the wire as the empty kind. */
function startedTerminals(backend: SessionServiceBackend) {
  return backend
    .callsTo(TerminalSessionService.method.startTerminalSession)
    .map((request) => ({
      kind: request.kind,
      wrappers: request.wrappers.map((wrapper) => ({ kind: wrapper.kind })),
    }));
}

/** The aria-labels of the open "+" menu's labelled groups, in order. */
function menuGroupLabels($groups: JQuery<HTMLElement>): string[] {
  return $groups.toArray().map((group) => group.getAttribute("aria-label") ?? "");
}

// ---------------------------------------------------------------------------

describe("NewTerminalMenu — tabs in a sandbox", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("offers In sandbox (default) and On host for a sandboxed session", () => {
    // Given a connected sandboxed session
    const backend = aBackendServing(aSession(true));
    attachSession(backend);

    // When the "+" menu is opened
    tabs.newTab().click();

    // Then "In sandbox" is the menu's first group, "On host" the next
    tabs.newTabMenuGroups().should(($groups) => {
      expect(menuGroupLabels($groups).slice(0, 2)).to.deep.equal(["In sandbox", "On host"]);
    });

    // When the shell is opened from each
    tabs.newTabInSandboxOption("bash").click();
    tabs.newTab().click();
    tabs.newTabOnHostOption("bash").click();

    // Then the daemon is asked for a jailed shell, then for a shell on the host
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(startedTerminals(b)).to.deep.equal([
        { kind: "", wrappers: [{ kind: "sandbox" }] },
        { kind: "", wrappers: [] },
      ]);
    });
  });

  it("lists agent kinds under In sandbox for a sandboxed session", () => {
    // Given a connected sandboxed session whose daemon runs Muse Code
    const backend = aBackendRunningMuseServing(aSession(true));
    attachSession(backend);

    // When the "+" menu is opened
    tabs.newTab().click();

    // Then Muse Code is offered both "In sandbox" and "On host"
    tabs.newTabInSandboxOption("muse-cli").should("exist");
    tabs.newTabOnHostOption("muse-cli").should("exist");

    // When it is opened from each
    tabs.newTabInSandboxOption("muse-cli").click();
    tabs.newTab().click();
    tabs.newTabOnHostOption("muse-cli").click();

    // Then the daemon is asked for Muse Code in a jail of its own, then for Muse Code on the host
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(startedTerminals(b)).to.deep.equal([
        { kind: "muse-cli", wrappers: [{ kind: "sandbox" }] },
        { kind: "muse-cli", wrappers: [] },
      ]);
    });
  });

  it("offers no sandbox choice for an unsandboxed session", () => {
    // Given a connected session that runs on the host
    const backend = aBackendServing(aSession(false));
    attachSession(backend);

    // When the "+" menu is opened
    tabs.newTab().click();

    // Then neither choice is offered, and its shell opens unwrapped
    tabs.newTabOption("bash").should("exist");
    tabs.newTabInSandboxOption("bash").should("not.exist");
    tabs.newTabOnHostOption("bash").should("not.exist");
    tabs.newTabOption("bash").click();
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(startedTerminals(b)).to.deep.equal([{ kind: "", wrappers: [] }]);
    });
  });
});
