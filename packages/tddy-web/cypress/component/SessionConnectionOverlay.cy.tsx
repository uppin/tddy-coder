/**
 * Component tests: a session's runtime panes must be covered by a connection overlay until the
 * session's LiveKit room is actually connected — and, if the connection fails, the overlay must say
 * so rather than leaving the operator staring at a terminal that will never receive input.
 *
 * The drawer keeps a session's terminal mounted while its LiveKit room is still handshaking (token
 * request + room join take a few seconds), so without an overlay the panes look ready to use before
 * they are. This mirrors the PR-Stack chat connecting overlay, but for a session runtime's panes.
 *
 * `SessionConnectionOverlay` is pure presentation, driven by a single `status` prop
 * (`LiveKitChromeStatus` = "connecting" | "connected" | "error"), so the three states are
 * deterministic to pin without a live LiveKit handshake.
 *
 * Feature: `docs/ft/web/session-drawer.md` (session connection state).
 */

import React from "react";
import { SessionConnectionOverlay } from "../../src/components/sessions/SessionConnectionOverlay";
import { sessionConnectionOverlayPage } from "../support/pages/sessionConnectionOverlayPage";

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("covers the session's panes with a connecting overlay while the LiveKit room is still connecting", () => {
  // Given — the session's LiveKit room is still establishing its connection
  cy.mount(<SessionConnectionOverlay status="connecting" />);

  // Then — an overlay covers the panes and tells the operator the session is connecting
  sessionConnectionOverlayPage.overlay().should("exist").and("contain.text", "Connecting");
});

it("shows no overlay once the session's LiveKit room has connected", () => {
  // Given — the room has finished connecting; the panes are ready to use
  cy.mount(<SessionConnectionOverlay status="connected" />);

  // Then — nothing covers the panes
  sessionConnectionOverlayPage.overlay().should("not.exist");
});

it("shows an error in the overlay when the LiveKit connection fails", () => {
  // Given — the session's LiveKit connection failed instead of completing
  cy.mount(<SessionConnectionOverlay status="error" />);

  // Then — the overlay stays up and surfaces the failure rather than a silently dead terminal
  sessionConnectionOverlayPage.overlay().should("exist");
  sessionConnectionOverlayPage.error().should("exist").and("contain.text", "Connection failed");
});
