/**
 * Acceptance test: a #/sessions/:id deep link that matches no known session (after the list
 * loads) shows a "session not found" state with a Home link, instead of silently no-opping.
 *
 * PRD: docs/ft/web/session-drawer.md
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { byTestId, TEST_IDS } from "../support/testIds";

const KNOWN_SESSION = {
  sessionId: "known-aaaaaaaa-0000-0000-0000-000000000000",
  createdAt: "2026-07-23T12:00:00Z",
  status: "exited",
  repoPath: "/home/dev/known",
  pid: 0,
  isActive: false,
  projectId: "proj-known-1",
  workflowGoal: "Known session",
  pendingElicitation: false,
};

describe("Sessions drawer — unknown deep link", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("shows a not-found message with a Home link for an unknown session id", () => {
    // Given — a deep link to a session id that is not in the loaded list
    window.location.hash = "/sessions/does-not-exist-999";
    const backend = aConnectionServiceBackend({ sessions: [KNOWN_SESSION] });

    // When
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen onNavigate={cy.stub()} />),
      backend,
    );

    // Then — the not-found state and its Home link are shown
    byTestId(TEST_IDS.terminalRouteUnknownSession).should("be.visible");
    byTestId(TEST_IDS.terminalRouteUnknownSessionHome).should("be.visible").click();

    // And — after choosing Home, the not-found state is dismissed
    byTestId(TEST_IDS.terminalRouteUnknownSession).should("not.exist");
  });
});
