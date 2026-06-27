/**
 * Cypress component tests: SessionMainPane — traffic strip integration
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip — acceptance criteria)
 *
 * The traffic strip has been moved out of SessionMainPane into SessionsDrawerScreen
 * (as a top-level toolbar). These tests verify that SessionMainPane no longer renders
 * the strip, and that the structural invariants inside the detail pane hold.
 */

import React from "react";
import { byTestId, TEST_IDS } from "../support/testIds";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FAKE_SESSION = {
  sessionId: "traffic-test-aaaa-0000-0000-0000-000000000001",
  createdAt: "2026-06-26T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/my-feature",
  pid: 42001,
  isActive: true,
  projectId: "proj-traffic-1",
  daemonInstanceId: "",
  workflowGoal: "Traffic strip test session",
  pendingElicitation: false,
};

const LIVEKIT_ATTACHMENT: SessionAttachmentState = {
  status: "connected-livekit",
  sessionId: FAKE_SESSION.sessionId,
  livekitRoom: "room-traffic-test-001",
  livekitUrl: "wss://livekit.example.internal",
  livekitServerIdentity: "server",
};

const GRPC_ATTACHMENT: SessionAttachmentState = {
  status: "connected-grpc",
  sessionId: FAKE_SESSION.sessionId,
};

const IDLE_ATTACHMENT: SessionAttachmentState = { status: "idle" };

function closedInspectorState(): import("../../src/components/sessions/SessionInspectorDrawer").InspectorDrawerState {
  return "closed";
}

function mountMainPane(attachment: SessionAttachmentState) {
  cy.mount(
    <SessionMainPane
      selectedSession={FAKE_SESSION as any}
      attachment={attachment}
      inspectorState={closedInspectorState()}
      onToggleInspector={cy.stub()}
      onInspectorClose={cy.stub()}
      onInspectorExpand={cy.stub()}
      onInspectorRestore={cy.stub()}
      onResume={cy.stub()}
      onDelete={cy.stub()}
      onTerminate={cy.stub()}
    />,
  );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("SessionMainPane — traffic strip integration (Cypress)", () => {
  // The strip has moved to SessionsDrawerScreen level; SessionMainPane must NOT render it.

  it("does NOT render the traffic strip inside sessions-detail-pane when connected via LiveKit", () => {
    mountMainPane(LIVEKIT_ATTACHMENT);

    byTestId(TEST_IDS.sessionsDetailPane)
      .find(`[data-testid="${TEST_IDS.sessionTrafficStrip}"]`)
      .should("not.exist");
  });

  it("does NOT render the traffic strip inside sessions-detail-pane when connected via HTTP RPC", () => {
    mountMainPane(GRPC_ATTACHMENT);

    byTestId(TEST_IDS.sessionsDetailPane)
      .find(`[data-testid="${TEST_IDS.sessionTrafficStrip}"]`)
      .should("not.exist");
  });

  it("does NOT render the traffic strip when no attachment is active (idle)", () => {
    mountMainPane(IDLE_ATTACHMENT);
    byTestId(TEST_IDS.sessionTrafficStrip).should("not.exist");
  });

  // -------------------------------------------------------------------------
  // Layout: inspector toggle row is present inside the detail pane
  // -------------------------------------------------------------------------

  it("renders the inspector toggle inside sessions-detail-pane when connected", () => {
    mountMainPane(LIVEKIT_ATTACHMENT);

    byTestId(TEST_IDS.sessionsDetailPane)
      .find(`[data-testid="${TEST_IDS.sessionsInspectorToggle}"]`)
      .should("exist");
  });
});
