/**
 * Acceptance test: the sessions drawer supports bulk selection + delete of sessions
 * (ported from the old ConnectionScreen). Selecting rows and choosing "delete selected"
 * deletes exactly the chosen sessions.
 *
 * PRD: docs/ft/web/session-drawer.md
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { byTestId, TEST_IDS, sessionRowSelect } from "../support/testIds";

const SESSION_A = {
  sessionId: "bulk-aaaaaaaa-0000-0000-0000-000000000000",
  createdAt: "2026-07-23T12:00:00Z",
  status: "exited",
  repoPath: "/home/dev/a",
  pid: 0,
  isActive: false,
  projectId: "proj-bulk-1",
  workflowGoal: "Session A",
  pendingElicitation: false,
};

const SESSION_B = {
  sessionId: "bulk-bbbbbbbb-0000-0000-0000-000000000000",
  createdAt: "2026-07-23T11:00:00Z",
  status: "exited",
  repoPath: "/home/dev/b",
  pid: 0,
  isActive: false,
  projectId: "proj-bulk-1",
  workflowGoal: "Session B",
  pendingElicitation: false,
};

const SESSION_C = {
  sessionId: "bulk-cccccccc-0000-0000-0000-000000000000",
  createdAt: "2026-07-23T10:00:00Z",
  status: "exited",
  repoPath: "/home/dev/c",
  pid: 0,
  isActive: false,
  projectId: "proj-bulk-1",
  workflowGoal: "Session C",
  pendingElicitation: false,
};

describe("Sessions drawer — bulk delete", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("deletes exactly the selected sessions and leaves the rest", () => {
    // Given — three sessions in the drawer
    const backend = aConnectionServiceBackend({ sessions: [SESSION_A, SESSION_B, SESSION_C] });
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen onNavigate={cy.stub()} />),
      backend,
    );

    // When — activate selection mode (checkboxes are hidden until the bottom minibar turns it on),
    // select A and C, then delete selected
    byTestId(sessionRowSelect(SESSION_A.sessionId)).should("not.exist");
    byTestId(TEST_IDS.sessionsDrawerSelectMode).click();
    byTestId(sessionRowSelect(SESSION_A.sessionId)).check();
    byTestId(sessionRowSelect(SESSION_C.sessionId)).check();
    byTestId(TEST_IDS.sessionsDrawerBulkDelete).click();

    // Then — A and C were deleted; B was not
    cy.wrap(null).should(() => {
      expect(backend.deletedSessionIds).to.deep.equal([
        SESSION_A.sessionId,
        SESSION_C.sessionId,
      ]);
    });
  });

  it("select-all from the minibar ticks every session, then deletes them all", () => {
    // Given — three sessions in the drawer
    const backend = aConnectionServiceBackend({ sessions: [SESSION_A, SESSION_B, SESSION_C] });
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen onNavigate={cy.stub()} />),
      backend,
    );

    // When — activate selection mode, use "Select all", then delete
    byTestId(TEST_IDS.sessionsDrawerSelectMode).click();
    byTestId(TEST_IDS.sessionsDrawerSelectAll).click();
    byTestId(sessionRowSelect(SESSION_A.sessionId)).should("be.checked");
    byTestId(sessionRowSelect(SESSION_B.sessionId)).should("be.checked");
    byTestId(sessionRowSelect(SESSION_C.sessionId)).should("be.checked");
    byTestId(TEST_IDS.sessionsDrawerBulkDelete).click();

    // Then — all three were deleted (in list order)
    cy.wrap(null).should(() => {
      expect(backend.deletedSessionIds).to.deep.equal([
        SESSION_A.sessionId,
        SESSION_B.sessionId,
        SESSION_C.sessionId,
      ]);
    });
  });
});
