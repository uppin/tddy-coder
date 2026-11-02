/**
 * Cypress component acceptance: a stopped session's Agent tab shows what the agent last printed,
 * greyed and read-only, under a toast that offers Resume — and Resume turns it live in place.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-dormant-agent-tab.md (`#session-tabs 6/7`)
 *
 * The daemon is in memory: `GetTerminalHistory` serves the recorded output, `ResumeSession` succeeds
 * or fails per test, and `ListSessions` keeps reporting the session stopped — so a view that goes
 * live after Resume can only have done so from the resume itself, not from a session-list refresh.
 */

import React from "react";
import { Code, ConnectError } from "@connectrpc/connect";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aSessionServiceBackend,
  type SessionServiceBackend,
} from "../support/rpc/daemonSessionHostBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";
import { byTestId, TEST_IDS } from "../support/testIds";

const STOPPED = {
  sessionId: "stopped-agent-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-11-02T09:00:00Z",
  status: "exited",
  repoPath: "/home/dev/finished-work",
  pid: 0,
  isActive: false,
  projectId: "proj-stopped-agent-1",
  daemonInstanceId: "local",
  sessionType: "claude-cli",
  pendingElicitation: false,
};

const RECORDED_OUTPUT = new TextEncoder().encode("Rewrote the tokenizer.\r\n$ ");

const dormantTerminal = (options?: Parameters<typeof cy.get>[1]) =>
  byTestId("dormant-agent-terminal", { timeout: 10000, ...options });
const toast = (options?: Parameters<typeof cy.get>[1]) =>
  byTestId("dormant-agent-toast", { timeout: 10000, ...options });
const toastResume = () => byTestId("dormant-agent-toast-action", { timeout: 10000 });
const toastError = () => byTestId("dormant-agent-toast-error", { timeout: 10000 });

/** A daemon serving the stopped session; `resume` decides what `ResumeSession` does. */
function aStoppedSessionBackend({
  history = "recorded",
  resume = "succeeds",
}: { history?: "recorded" | "none"; resume?: "succeeds" | "fails" } = {}) {
  const resumes: string[] = [];
  const backend = aSessionServiceBackend({
    sessions: [STOPPED],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    terminalHistory:
      history === "recorded"
        ? [{ data: RECORDED_OUTPUT, startOffset: 0n, endOffset: BigInt(RECORDED_OUTPUT.length), atOldest: true, atEnd: true }]
        : undefined,
    terminalHistoryNotFound: history === "none",
    resumeSession: (sessionId) => {
      resumes.push(sessionId);
      if (resume === "fails") {
        throw new ConnectError("the daemon refused the resume", Code.FailedPrecondition);
      }
      return { sessionId, livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" };
    },
  });
  return { backend, resumes };
}

function selectTheStoppedSession(backend: SessionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(STOPPED.sessionId).click();
}

describe("A stopped session's Agent tab", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("a stopped session's Agent tab shows its recorded output in the history colour scheme", () => {
    // Given
    const { backend } = aStoppedSessionBackend();

    // When
    selectTheStoppedSession(backend);

    // Then
    dormantTerminal().should("have.attr", "data-color-scheme", "history");
    dormantTerminal().within(() => {
      byTestId(TEST_IDS.terminalBufferText, { timeout: 20000 }).should("contain.text", "Rewrote the tokenizer.");
    });
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(b.getTerminalHistoryCalls.map((c) => [c.sessionId, c.terminalId])).to.deep.include([
        STOPPED.sessionId,
        "main",
      ]);
    });
  });

  it("typing into a stopped session's Agent tab sends no input", () => {
    // Given
    const { backend } = aStoppedSessionBackend();
    selectTheStoppedSession(backend);
    dormantTerminal().should("exist");

    // When
    dormantTerminal().click().type("ls{enter}");

    // Then
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(b.sentTerminalInput).to.deep.equal([]);
    });
  });

  it("a toast says the session is not running and offers Resume", () => {
    // Given / When
    const { backend } = aStoppedSessionBackend();
    selectTheStoppedSession(backend);

    // Then
    toast().should("be.visible").and("contain.text", "This session is not running");
    toastResume().should("be.visible").and("contain.text", "Resume");
  });

  it("Resume from the toast goes live in the Agent tab without waiting for a session refresh", () => {
    // Given a stopped session whose listing will keep saying it is stopped
    const { backend, resumes } = aStoppedSessionBackend();
    selectTheStoppedSession(backend);

    // When
    toastResume().click();

    // Then it was resumed once, and the Agent tab holds the live terminal
    cy.wrap(resumes).should("deep.equal", [STOPPED.sessionId]);
    dormantTerminal().should("not.exist");
    toast().should("not.exist");
    tabs.paneTerminal("main").should("exist");
  });

  it("a bash tab opened before Resume is the same terminal after it", () => {
    // Given a stopped session with a bash tab opened while it was stopped
    const { backend } = aStoppedSessionBackend();
    selectTheStoppedSession(backend);
    tabs.newTab().click();
    tabs.newTabOption("bash").click();
    tabs.tab("bash-1").should("exist");

    // When
    tabs.agentTab().click();
    toastResume().click();

    // Then the tab is still there and no second terminal was started for it
    dormantTerminal().should("not.exist");
    tabs.tab("bash-1").should("exist");
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(b.startTerminalSessionIds).to.deep.equal([STOPPED.sessionId]);
    });
  });

  it("a failed Resume shows its error in the toast and keeps the greyed view", () => {
    // Given a daemon that refuses the resume
    const { backend } = aStoppedSessionBackend({ resume: "fails" });
    selectTheStoppedSession(backend);

    // When
    toastResume().click();

    // Then
    toastError().should("contain.text", "the daemon refused the resume");
    dormantTerminal().should("have.attr", "data-color-scheme", "history");
  });

  it("a stopped session with no recorded output says so", () => {
    // Given a session whose agent terminal was never recorded
    const { backend } = aStoppedSessionBackend({ history: "none" });

    // When
    selectTheStoppedSession(backend);

    // Then
    byTestId("dormant-agent-terminal-empty", { timeout: 10000 }).should("contain.text", "No recorded output");
    toast().should("be.visible");
  });
});
