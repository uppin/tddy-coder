/**
 * Cypress component acceptance: Resume brings a session's tabs back under their recorded ids, so the
 * tab the operator was on before the stop is still the selected one after the stopped → live swap.
 *
 * PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-19-resume-restores-tab-roster.md` (`#session-liveness` 4/4),
 * acceptance criterion 7.
 *
 * Driven over the in-memory daemon (`aSessionServiceBackend`): the stopped session's daemon lists the
 * restored Claude tab, `ResumeSession` reports it `RESTORED`, and every `ListSessions` after the
 * resume reports the session live — the daemon's view once its agent runs again.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { RestoredTabOutcome, RestoredTabSchema } from "tddy-client-web/gen/session_pb";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aSessionServiceBackend,
  type SessionServiceBackend,
} from "../support/rpc/daemonSessionHostBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionActivitiesPage } from "../support/pages/sessionActivitiesPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";

// ---------------------------------------------------------------------------
// Fixture — a stopped claude-cli session with a second Claude tab in its roster
// ---------------------------------------------------------------------------

/** The recorded terminal id of the session's second Claude tab. */
const CLAUDE_TAB = "tab-claude-0000-0000-0000-000000000002";

const STOPPED = {
  sessionId: "resume-roster-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-11-19T09:00:00Z",
  status: "exited",
  repoPath: "/home/dev/feature-alpha",
  pid: 0,
  isActive: false,
  projectId: "proj-resume-roster",
  daemonInstanceId: "local",
  sessionType: "claude-cli",
  agent: "claude-cli",
  pendingElicitation: false,
};

/**
 * A daemon whose stopped session lists [`CLAUDE_TAB`], reports it restored by `ResumeSession`, and
 * lists the session live from the resume on. Served host-direct (empty room), like any PTY session.
 */
function aStoppedSessionWhoseClaudeTabIsRestored(
  outcome: RestoredTabOutcome = RestoredTabOutcome.RESTORED,
  reason = "",
): SessionServiceBackend {
  let resumed = false;
  return aSessionServiceBackend({
    listSessionsFactory: () =>
      resumed ? [{ ...STOPPED, status: "active", pid: 4242, isActive: true }] : [STOPPED],
    terminals: [{ terminalId: CLAUDE_TAB, kind: "claude-cli" }],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    resumeSession: (sessionId) => {
      resumed = true;
      return {
        sessionId,
        livekitRoom: "",
        livekitUrl: "",
        livekitServerIdentity: "",
        restoredTabs: [
          create(RestoredTabSchema, {
            terminalId: CLAUDE_TAB,
            outcome,
            reason,
          }),
        ],
      };
    },
  });
}

function selectStoppedSession(backend: SessionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.selectEndedSession(STOPPED.sessionId);
}

// ---------------------------------------------------------------------------

describe("Resume restores the session's tab roster", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("keeps the active tab selected across Resume", () => {
    // Given a stopped session whose second Claude tab is the one the operator is on
    const backend = aStoppedSessionWhoseClaudeTabIsRestored();
    selectStoppedSession(backend);
    tabs.tab(CLAUDE_TAB).click();
    tabs.tab(CLAUDE_TAB).should("have.attr", "aria-selected", "true");

    // When the operator resumes the session
    sessionActivitiesPage.resume(STOPPED.sessionId);

    // Then the live session's strip still has that tab selected, not the Agent tab
    sessionActivitiesPage.resumeBtn(STOPPED.sessionId).should("not.exist");
    tabs.tab(CLAUDE_TAB).should("have.attr", "aria-selected", "true");
    tabs.agentTab().should("have.attr", "aria-selected", "false");
  });

  it("says which tab reopened fresh, and why", () => {
    // Given a stopped session whose tab the daemon can only reopen fresh
    const backend = aStoppedSessionWhoseClaudeTabIsRestored(
      RestoredTabOutcome.FRESH,
      "opencode-cli cannot resume a given conversation, so the tab reopened fresh",
    );
    selectStoppedSession(backend);

    // When the operator resumes the session
    sessionActivitiesPage.resume(STOPPED.sessionId);

    // Then a notice names the tab and the reason
    cy.get("[data-testid=sessions-resume-notice]")
      .should("contain.text", CLAUDE_TAB)
      .and("contain.text", "reopened fresh");
  });
});
