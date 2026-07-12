/**
 * Acceptance tests: single-screen terminal control mutex — the "Claim terminal" overlay.
 *
 * PRD: docs/ft/daemon/terminal-sessions.md (control section) and
 *      docs/ft/web/session-drawer.md (Claim terminal CTA section).
 *
 * The overlay is pure presentation (`TerminalControlOverlay`); the lease state and steal-claim
 * action are owned per-session by `SessionRuntime` via `useTerminalControl` (see
 * `SessionParticipantRpcRouting.cy.tsx` for the end-to-end claim-routing contract). These tests
 * exercise the overlay's presentation contract directly.
 */

import React from "react";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import { TerminalControlOverlay } from "../../src/components/sessions/TerminalControlOverlay";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import type { SessionEntry } from "../../src/gen/connection_pb";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "control-test-session-1";

const aSelectedSession: Partial<SessionEntry> = {
  sessionId: SESSION_ID,
  isActive: true,
  status: "active",
  repoPath: "/home/user/my-project",
};

const aConnectedGrpcAttachment: SessionAttachmentState = {
  status: "connected-grpc",
  sessionId: SESSION_ID,
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

it("shows the Claim terminal overlay when this screen does not hold the control lease", () => {
  // Given — another screen holds control
  const onClaim = cy.stub();

  // When
  cy.mount(
    <TerminalControlOverlay isController={false} holderScreenId="other-screen-abc" onClaim={onClaim} />,
  );

  // Then
  page.terminalControlOverlay().should("be.visible");
  page.terminalClaimBtn().should("be.visible").and("contain.text", "Claim terminal");
  page.terminalControlHolder().should("contain.text", "other-screen-abc");
});

// ---------------------------------------------------------------------------
// AC2: Clicking "Claim terminal" calls `onClaim`.
// ---------------------------------------------------------------------------

it("calls onClaim when the Claim terminal button is clicked", () => {
  // Given
  const onClaim = cy.stub().as("claimStub");
  cy.mount(
    <TerminalControlOverlay isController={false} holderScreenId="other-screen-xyz" onClaim={onClaim} />,
  );

  // When
  page.terminalClaimBtn().click();

  // Then
  cy.get("@claimStub").should("have.been.calledOnce");
});

// ---------------------------------------------------------------------------
// AC3: When this screen IS the controller, the overlay is not present.
// ---------------------------------------------------------------------------

it("does not render the Claim terminal overlay when this screen holds the control lease", () => {
  // Given — this screen is the controller
  cy.mount(
    <TerminalControlOverlay isController={true} holderScreenId="this-screen-id" onClaim={cy.stub()} />,
  );

  // Then
  page.terminalControlOverlay().should("not.exist");
  page.terminalClaimBtn().should("not.exist");
});

// ---------------------------------------------------------------------------
// AC4: When no runtime is attached yet (session selected but not connected), no overlay renders.
// ---------------------------------------------------------------------------

it("does not render the Claim terminal overlay when no runtime is attached yet", () => {
  // Given — session selected and connected, but the runtime layer has not registered a runtime yet
  cy.mount(
    <SessionMainPane
      {...noopHandlers}
      selectedSession={aSelectedSession as SessionEntry}
      attachment={aConnectedGrpcAttachment}
      runtimes={[]}
      focusedRuntimeId={null}
    />,
  );

  // Then — the transition placeholder container exists, but no control overlay is rendered
  page.detailTerminalContainer().should("exist");
  page.terminalControlOverlay().should("not.exist");
});
