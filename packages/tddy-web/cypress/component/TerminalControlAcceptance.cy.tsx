/**
 * Acceptance tests: single-screen terminal control mutex.
 *
 * PRD: docs/ft/daemon/terminal-sessions.md (control section) and
 *      docs/ft/web/session-drawer.md (Claim terminal CTA section).
 *
 * These tests exercise `SessionMainPane` with explicit `terminalControl` prop values to verify
 * the overlay CTA behavior independently of the full `SessionsDrawerScreen` setup. All tests
 * Exercises `SessionMainPane` with explicit `terminalControl` prop values.
 */

import React from "react";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import type { SessionEntry } from "../../src/gen/connection_pb";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";
import { TEST_IDS, byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "control-test-session-1";

const aConnectedGrpcAttachment: SessionAttachmentState = {
  status: "connected-grpc",
  sessionId: SESSION_ID,
};

const aSelectedSession: Partial<SessionEntry> = {
  sessionId: SESSION_ID,
  isActive: true,
  status: "active",
  repoPath: "/home/user/my-project",
};

const noopHandlers = {
  inspectorState: "closed" as const,
  onToggleInspector: () => undefined,
  onInspectorClose: () => undefined,
  onInspectorExpand: () => undefined,
  onInspectorRestore: () => undefined,
  onResume: () => undefined,
  onDelete: () => undefined,
  onTerminate: () => undefined,
};

// ---------------------------------------------------------------------------
// AC1: When this screen is not the controller, the "Claim terminal" overlay is visible.
// ---------------------------------------------------------------------------

it("shows Claim terminal CTA overlay when this screen does not hold the control lease", () => {
  // Given — another screen holds control
  const onClaim = cy.stub();

  // When
  cy.mount(
    <SessionMainPane
      {...noopHandlers}
      selectedSession={aSelectedSession as SessionEntry}
      attachment={aConnectedGrpcAttachment}
      terminalControl={{
        isController: false,
        holderScreenId: "other-screen-abc",
        onClaim,
      }}
    />,
  );

  // Then
  page.detailTerminalContainer().should("exist");
  page.terminalControlOverlay().should("be.visible");
  page.terminalClaimBtn().should("be.visible").and("contain.text", "Claim terminal");
  page.terminalControlHolder().should("contain.text", "other-screen-abc");
});

// ---------------------------------------------------------------------------
// AC2: Clicking "Claim terminal" calls `onClaim` and the overlay resolves.
// ---------------------------------------------------------------------------

it("clicking Claim terminal calls onClaim callback", () => {
  // Given
  const onClaim = cy.stub().as("claimStub");

  cy.mount(
    <SessionMainPane
      {...noopHandlers}
      selectedSession={aSelectedSession as SessionEntry}
      attachment={aConnectedGrpcAttachment}
      terminalControl={{
        isController: false,
        holderScreenId: "other-screen-xyz",
        onClaim,
      }}
    />,
  );

  // When
  page.terminalClaimBtn().click();

  // Then
  cy.get("@claimStub").should("have.been.calledOnce");
});

// ---------------------------------------------------------------------------
// AC3: When this screen IS the controller, the overlay is not present.
// ---------------------------------------------------------------------------

it("does not show the Claim terminal overlay when this screen holds the control lease", () => {
  // Given
  cy.mount(
    <SessionMainPane
      {...noopHandlers}
      selectedSession={aSelectedSession as SessionEntry}
      attachment={aConnectedGrpcAttachment}
      terminalControl={{
        isController: true,
        holderScreenId: "this-screen-id",
        onClaim: cy.stub(),
      }}
    />,
  );

  // Then
  byTestId(TEST_IDS.terminalControlOverlay).should("not.exist");
  byTestId(TEST_IDS.terminalClaimBtn).should("not.exist");
});

// ---------------------------------------------------------------------------
// AC4: When no terminalControl prop is provided (e.g. session not yet connected),
//      no overlay is rendered.
// ---------------------------------------------------------------------------

it("does not show the overlay when terminalControl prop is absent", () => {
  // Given — session selected but not yet connected
  cy.mount(
    <SessionMainPane
      {...noopHandlers}
      selectedSession={aSelectedSession as SessionEntry}
      attachment={aConnectedGrpcAttachment}
    />,
  );

  // Then
  byTestId(TEST_IDS.terminalControlOverlay).should("not.exist");
});
