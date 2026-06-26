/**
 * Acceptance tests: VncOverlay component.
 *
 * PRD: docs/ft/web/vnc-sessions.md (AC-VNC-5).
 *
 * Mounts VncOverlay directly (no RPCs needed) to verify the overlay UI and
 * dismiss mechanics. room=null is valid — overlay must render without a live
 * LiveKit room; video track attachment is tested in integration only.
 */

import React from "react";
import { VncOverlay } from "../../src/components/sessions/VncOverlay";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Shared props
// ---------------------------------------------------------------------------

const OVERLAY_PROPS = {
  room: null,
  bridgeIdentity: "vnc-test-session-aabbccdd-t-001",
  trackName: "vnc:t-001",
  width: 1920,
  height: 1080,
};

// ---------------------------------------------------------------------------
// AC-VNC-5-a: Overlay renders required elements
// ---------------------------------------------------------------------------

it("renders the overlay root, video element, and close button", () => {
  cy.mount(<VncOverlay {...OVERLAY_PROPS} onClose={() => {}} />);

  // Then
  page.vncOverlay().should("exist").and("be.visible");
  page.vncOverlayVideo().should("exist");
  page.vncOverlayClose().should("exist").and("be.visible");
});

// ---------------------------------------------------------------------------
// AC-VNC-5-b: Close button dismisses the overlay
// ---------------------------------------------------------------------------

it("calls onClose when the close button is clicked", () => {
  // Given
  const onClose = cy.stub().as("onClose");

  // When
  cy.mount(<VncOverlay {...OVERLAY_PROPS} onClose={onClose} />);
  page.vncOverlayClose().click();

  // Then
  cy.get("@onClose").should("have.been.calledOnce");
});

// ---------------------------------------------------------------------------
// AC-VNC-5-c: Escape key dismisses the overlay
// ---------------------------------------------------------------------------

it("calls onClose when the Escape key is pressed", () => {
  // Given
  const onClose = cy.stub().as("onClose");

  // When
  cy.mount(<VncOverlay {...OVERLAY_PROPS} onClose={onClose} />);
  page.vncOverlay().should("exist");
  cy.get("body").trigger("keydown", { key: "Escape", bubbles: true });

  // Then
  cy.get("@onClose").should("have.been.calledOnce");
});

// ---------------------------------------------------------------------------
// AC-VNC-5-d: Clicking the backdrop dismisses the overlay
// ---------------------------------------------------------------------------

it("calls onClose when the backdrop is clicked outside the dialog", () => {
  // Given
  const onClose = cy.stub().as("onClose");

  // When
  cy.mount(<VncOverlay {...OVERLAY_PROPS} onClose={onClose} />);
  page.vncOverlay().then(($el) => {
    $el[0].dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));
  });

  // Then
  cy.get("@onClose").should("have.been.calledOnce");
});
