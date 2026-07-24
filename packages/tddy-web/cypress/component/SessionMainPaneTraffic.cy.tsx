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
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { hostStatsFooterPage } from "../support/pages/hostStatsFooterPage";
import { aSessionTrafficBar } from "../support/drivers/sessionTrafficBarDriver";

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
  identity: "browser-traffic-test-aaaa-0000-0000-0000-000000000001",
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

// ---------------------------------------------------------------------------
// Multi-session aggregate — the screen-level readout reflects the terminal (data-plane) traffic of
// EVERY mounted runtime (focused AND backgrounded), not just the focused session's room meter.
//
// Changeset: `statusbar-session-traffic`
// PRD: `docs/ft/web/host-stats-footer.md`
// ---------------------------------------------------------------------------

const AGG_SESSION_A = {
  sessionId: "traffic-agg-aaaa-0000-0000-0000-00000000000a",
  createdAt: "2026-07-24T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/agg-a",
  pid: 81001,
  isActive: true,
  projectId: "proj-agg",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

const AGG_SESSION_B = {
  ...AGG_SESSION_A,
  sessionId: "traffic-agg-bbbb-0000-0000-0000-00000000000b",
  repoPath: "/home/dev/agg-b",
  pid: 81002,
};

function aTwoSessionBackend() {
  return aConnectionServiceBackend({
    sessions: [AGG_SESSION_A, AGG_SESSION_B],
    connectSession: (sessionId: string) => ({
      livekitRoom: `room-${sessionId}`,
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: `daemon-local-${sessionId}`,
    }),
  });
}

describe("SessionMainPaneTraffic — screen wires every mounted runtime into the one footer readout", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("keeps the first session mounted (backgrounded) beneath a single footer readout after focus moves", () => {
    // Given — a screen with two active sessions
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen />, [{ instanceId: "local", label: "local" }]),
      aTwoSessionBackend(),
    );

    // When — the first session is attached, then focus moves to the second
    sessionsDrawerPage.drawerItem(AGG_SESSION_A.sessionId).click();
    sessionsDrawerPage.runtimeTerminal(AGG_SESSION_A.sessionId).should("exist");
    sessionsDrawerPage.drawerItem(AGG_SESSION_B.sessionId).click();
    sessionsDrawerPage.runtimeTerminal(AGG_SESSION_B.sessionId).should("exist");

    // Then — the first session stays mounted in the background, and there is exactly one footer
    // readout fed by the shared runtime registry (not a per-session or focused-only strip).
    sessionsDrawerPage.runtimeTerminal(AGG_SESSION_A.sessionId).should("exist");
    hostStatsFooterPage.trafficStripInFooter().should("have.length", 1);
  });
});

describe("SessionMainPaneTraffic — aggregate byte totals across focused and backgrounded sessions", () => {
  it("sums inbound and outbound bytes from both the focused and backgrounded runtimes", () => {
    // Given — a focused session and a backgrounded one, both mounted
    aSessionTrafficBar()
      .withAttachedSession("focused", { focused: true })
      .withAttachedSession("background")
      .mount()
      // When — traffic flows on both, in both directions
      .receiveBytes("focused", { bytesIn: 1_000 })
      .receiveBytes("focused", { bytesOut: 400 })
      .receiveBytes("background", { bytesIn: 2_000 })
      .receiveBytes("background", { bytesOut: 600 })
      // Then — the readout is the aggregate across BOTH runtimes, per direction
      .expectBytesIn("3.0 kB")
      .expectBytesOut("1.0 kB");
  });

  it("keeps a backgrounded session's traffic in the total after focus moved to another session", () => {
    // Given — two mounted sessions with the second focused (the first is backgrounded)
    aSessionTrafficBar()
      .withAttachedSession("background")
      .withAttachedSession("focused", { focused: true })
      .mount()
      .receiveBytes("focused", { bytesIn: 1_000 })
      // When — the BACKGROUNDED session receives more terminal output (it keeps streaming)
      .receiveBytes("background", { bytesIn: 3_000 })
      // Then — the backgrounded session's bytes are included, not dropped
      .expectBytesIn("4.0 kB");
  });
});
