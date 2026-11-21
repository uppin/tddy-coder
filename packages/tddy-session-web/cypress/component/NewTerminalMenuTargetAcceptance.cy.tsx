/**
 * Cypress component acceptance: a target session's "+" menu offers its shell "On target" — on the
 * session's target, first and so the default — and "On this host"; a session that runs on this host
 * offers no such choice.
 *
 * PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-21-target-shell-tab.md` (`#session-liveness` 6/8)
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

const SESSION_ID = "target-tabs-aaaaaaaa-0000-0000-0000-000000000001";

/** A session whose agent runs on the ssh target `buildbox`. */
function aTargetSession(): Partial<SessionEntry> {
  return {
    ...aHostSession(),
    repoPath: `/home/dev/repo/.worktrees/${SESSION_ID}`,
    target: { kind: { case: "ssh", value: { configHost: "buildbox" } } },
  } as Partial<SessionEntry>;
}

/** A session whose agent runs on the daemon's host. */
function aHostSession(): Partial<SessionEntry> {
  return {
    sessionId: SESSION_ID,
    createdAt: "2026-11-21T09:00:00Z",
    status: "active",
    repoPath: "/home/dev/feature-on-host",
    pid: 90012,
    isActive: true,
    projectId: "proj-target-tabs",
    daemonInstanceId: "local",
    pendingElicitation: false,
    sessionType: "claude-cli",
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

describe("NewTerminalMenu — shell tabs on a target", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("offers On target (default) and On this host for a target session", () => {
    // Given a connected session whose agent runs on a target
    const backend = aBackendServing(aTargetSession());
    attachSession(backend);

    // When the "+" menu is opened
    tabs.newTab().click();

    // Then "On target" is the menu's first group, "On this host" the next
    tabs.newTabMenuGroups().should(($groups) => {
      expect(menuGroupLabels($groups).slice(0, 2)).to.deep.equal(["On target", "On this host"]);
    });

    // When the shell is opened from each
    tabs.newTabOnTargetOption("bash").click();
    tabs.newTab().click();
    tabs.newTabOnThisHostOption("bash").click();

    // Then the daemon is asked for a shell on the target, then for a shell on this host
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(startedTerminals(b)).to.deep.equal([
        { kind: "", wrappers: [{ kind: "target" }] },
        { kind: "", wrappers: [] },
      ]);
    });
  });

  it("offers no target choice for a local session", () => {
    // Given a connected session that runs on the daemon's host
    const backend = aBackendServing(aHostSession());
    attachSession(backend);

    // When the "+" menu is opened
    tabs.newTab().click();

    // Then neither choice is offered, and its shell opens unwrapped
    tabs.newTabOption("bash").should("exist");
    tabs.newTabOnTargetOption("bash").should("not.exist");
    tabs.newTabOnThisHostOption("bash").should("not.exist");
    tabs.newTabOption("bash").click();
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(startedTerminals(b)).to.deep.equal([{ kind: "", wrappers: [] }]);
    });
  });
});
