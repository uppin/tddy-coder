/**
 * Cypress component tests: SessionMainPane — traffic strip integration
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip — acceptance criteria)
 *
 * Tests that `SessionMainPane` renders the `SessionTrafficStrip` in the correct
 * states and that the strip wires up to the traffic meter and ping.
 *
 * ⚠️ These tests fail until:
 *   1. `SessionTrafficStrip.tsx` is created.
 *   2. `SessionMainPane.tsx` is updated to render the strip in the
 *      `connected-livekit` attachment state.
 *   3. The `TrafficMeterRegistry` context is wired up.
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

function closedInspectorState() {
  return { open: false, expanded: false };
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
  // -------------------------------------------------------------------------
  // AC1 (PRD): Strip is visible when connected-livekit
  // -------------------------------------------------------------------------

  it("renders the traffic strip when the session is connected via LiveKit", () => {
    // Given / When
    mountMainPane(LIVEKIT_ATTACHMENT);

    // Then
    byTestId(TEST_IDS.sessionTrafficStrip).should("exist");
  });

  // -------------------------------------------------------------------------
  // AC2 (PRD): Strip is absent when not connected-livekit
  // -------------------------------------------------------------------------

  it("does NOT render the traffic strip when the session is connected via gRPC only", () => {
    mountMainPane(GRPC_ATTACHMENT);
    byTestId(TEST_IDS.sessionTrafficStrip).should("not.exist");
  });

  it("does NOT render the traffic strip when no attachment is active (idle)", () => {
    mountMainPane(IDLE_ATTACHMENT);
    byTestId(TEST_IDS.sessionTrafficStrip).should("not.exist");
  });

  // -------------------------------------------------------------------------
  // AC3 (PRD): Strip starts at zero bytes
  // -------------------------------------------------------------------------

  it("shows 0 B bytes in and out initially when the session first connects", () => {
    mountMainPane(LIVEKIT_ATTACHMENT);

    byTestId(TEST_IDS.sessionTrafficBytesIn).should("contain.text", "0 B");
    byTestId(TEST_IDS.sessionTrafficBytesOut).should("contain.text", "0 B");
  });

  it("shows 0 B/s rates initially (no traffic yet)", () => {
    mountMainPane(LIVEKIT_ATTACHMENT);

    byTestId(TEST_IDS.sessionTrafficRateIn).should("contain.text", "0 B/s");
    byTestId(TEST_IDS.sessionTrafficRateOut).should("contain.text", "0 B/s");
  });

  // -------------------------------------------------------------------------
  // AC6 (PRD): Ping shows — when RTT is unavailable
  // -------------------------------------------------------------------------

  it("shows — ping initially (Room not yet connected for RTT stats)", () => {
    mountMainPane(LIVEKIT_ATTACHMENT);

    byTestId(TEST_IDS.sessionTrafficPing).should("contain.text", "—");
  });

  // -------------------------------------------------------------------------
  // Layout: strip sits above the inspector toggle row
  // -------------------------------------------------------------------------

  it("the traffic strip appears before the inspector toggle in DOM order", () => {
    mountMainPane(LIVEKIT_ATTACHMENT);

    byTestId(TEST_IDS.sessionsDetailPane).within(() => {
      cy.get(`[data-testid="${TEST_IDS.sessionTrafficStrip}"], [data-testid="${TEST_IDS.sessionsInspectorToggle}"]`)
        .then(($els) => {
          // The strip must come before the inspector toggle in the DOM
          expect($els[0].getAttribute("data-testid")).to.equal(TEST_IDS.sessionTrafficStrip);
        });
    });
  });
});
