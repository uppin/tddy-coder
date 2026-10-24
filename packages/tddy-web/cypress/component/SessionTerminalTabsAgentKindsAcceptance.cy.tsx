/**
 * Cypress component acceptance: agent terminal tabs — the "+" opens a menu of Bash and every CLI
 * harness, a chosen agent runs in a tab of its own beside the session's main agent, and tabs are
 * labelled by what they run.
 *
 * PRD: `docs/ft/web/session-terminal-tabs.md` (agent terminal tabs, `#warp-agent` 2/2)
 *
 * Driven over the deterministic gRPC path: `connectSession` returns an empty `livekitRoom`, so every
 * terminal RPC flows over the daemon client into the in-memory backend.
 */

import React from "react";
import { Code } from "@connectrpc/connect";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { TerminalSessionService } from "../../src/gen/terminal_session_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aSessionServiceBackend,
  type SessionServiceBackend,
} from "../support/rpc/daemonSessionHostBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionTabsErrorsPage as errors } from "../support/pages/sessionTabsErrorsPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";

// ---------------------------------------------------------------------------
// Fixture — one active session attached over gRPC
// ---------------------------------------------------------------------------

const SESSION_ID = "agent-tabs-aaaaaaaa-0000-0000-0000-000000000001";

/** Every kind the "+" menu offers on a CLI harness session, in menu order. */
const EVERY_KIND = ["bash", "claude-cli", "cursor-cli", "opencode-cli", "muse-cli", "warp-cli"];

function aSession(sessionType: string) {
  return {
    sessionId: SESSION_ID,
    createdAt: "2026-10-24T09:00:00Z",
    status: "active",
    repoPath: "/home/dev/feature-alpha",
    pid: 90001,
    isActive: true,
    projectId: "proj-agent-tabs",
    daemonInstanceId: "local",
    pendingElicitation: false,
    sessionType,
  };
}

/** A host-served `sessionType` session whose started terminals are `term-1`, `term-2`, … */
function aBackendFor(
  sessionType: string,
  terminals: Array<{ terminalId: string; kind: string }> = [],
): SessionServiceBackend {
  return aSessionServiceBackend({
    sessions: [aSession(sessionType)],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    terminals,
    newTerminalId: (index) => `term-${index + 1}`,
  });
}

function attachSession(backend: SessionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(SESSION_ID).click();
  tabs.tabs().should("exist");
}

/** Opens the "+" menu and picks the item for `kind`. */
function openATerminalOf(kind: string) {
  tabs.newTab().click();
  tabs.newTabOption(kind).click();
}

// ---------------------------------------------------------------------------

describe("SessionTerminalTabs — agent terminals beside bash", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("opens a menu of Bash and every CLI harness from '+'", () => {
    // Given
    attachSession(aBackendFor("claude-cli"));

    // When
    tabs.newTab().click();

    // Then
    tabs
      .newTabMenu()
      .find("[data-testid^='sessions-terminal-tab-new-']")
      .then(($items) => {
        const kinds = [...$items].map((item) =>
          item.getAttribute("data-testid")!.replace("sessions-terminal-tab-new-", ""),
        );
        expect(kinds).to.deep.equal(EVERY_KIND);
      });
    tabs.newTabOption("warp-cli").should("have.text", "Warp");
    tabs.newTabOption("bash").should("have.text", "Bash");
  });

  it("starts a Muse Code terminal from the menu and focuses its tab", () => {
    // Given
    const backend = aBackendFor("claude-cli");
    attachSession(backend);

    // When
    openATerminalOf("muse-cli");

    // Then the daemon is asked for a Muse Code terminal ...
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(b.startedTerminalKinds).to.deep.equal(["muse-cli"]);
    });
    // ... whose tab is focused, labelled by what it runs, and streams its own terminal_id
    tabs.tab("term-1").should("have.attr", "aria-selected", "true").and("have.text", "Muse Code 1");
    tabs.newTabMenu().should("not.exist");
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(b.streamedTerminals.map((s) => s.terminalId)).to.include("term-1");
    });
  });

  it("labels tabs by kind with a per-kind ordinal", () => {
    // Given
    attachSession(aBackendFor("claude-cli"));

    // When
    openATerminalOf("bash");
    tabs.tab("term-1").should("exist");
    openATerminalOf("warp-cli");
    tabs.tab("term-2").should("exist");
    openATerminalOf("bash");

    // Then
    tabs.tab("term-1").should("have.text", "Bash 1");
    tabs.tab("term-2").should("have.text", "Warp 1");
    tabs.tab("term-3").should("have.text", "Bash 2");
  });

  it("restores tab labels from the listed terminals' kinds on reattach", () => {
    // Given a session that already runs a Warp terminal and a shell
    const backend = aBackendFor("claude-cli", [
      { terminalId: "019a-warp", kind: "warp-cli" },
      { terminalId: "019a-shell", kind: "bash" },
    ]);

    // When
    attachSession(backend);

    // Then
    tabs.tab("019a-warp").should("have.text", "Warp 1");
    tabs.tab("019a-shell").should("have.text", "Bash 1");
  });

  it("offers only Bash on a tool session", () => {
    // Given — a tool session's terminals are served by its tddy-coder participant, which runs bash only
    attachSession(aBackendFor("tool"));

    // When
    tabs.newTab().click();

    // Then
    tabs.newTabOption("bash").should("exist");
    tabs
      .newTabMenu()
      .find("[data-testid^='sessions-terminal-tab-new-']")
      .should("have.length", 1);
  });

  it("closes the menu on Escape without starting a terminal", () => {
    // Given
    const backend = aBackendFor("claude-cli");
    attachSession(backend);
    tabs.newTab().click();
    tabs.newTabMenu().should("exist");

    // When
    cy.get("body").type("{esc}");

    // Then
    tabs.newTabMenu().should("not.exist");
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(b.startedTerminalKinds).to.deep.equal([]);
    });
  });

  it("shows why an agent terminal could not start and opens no tab", () => {
    // Given
    const backend = aBackendFor("claude-cli");
    backend.failWith(
      TerminalSessionService.method.startTerminalSession,
      Code.InvalidArgument,
      'unknown terminal kind "muse-cli"',
    );
    attachSession(backend);

    // When
    openATerminalOf("muse-cli");

    // Then
    errors.terminalError().should("contain.text", 'unknown terminal kind "muse-cli"');
    tabs.tab("term-1").should("not.exist");
  });
});
