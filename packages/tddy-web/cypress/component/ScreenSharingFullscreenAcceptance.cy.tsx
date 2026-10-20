/**
 * Acceptance tests: the remote desktop viewer takes the whole screen, and keys reach the desktop.
 *
 * Spec: docs/ft/web/screen-sharing-sessions.md (AC-SS-6); packages/tddy-web/docs/remote-desktop-input.md.
 *
 * The Fullscreen and Keyboard Lock APIs are faked on the AUT window (`support/fullscreenBrowser`):
 * a real `requestFullscreen` needs a user activation the runner cannot give, and what the browser
 * then does with reserved keys is not observable from a page at all. So these specs pin what tddy
 * **asks the browser to do** — which element, when to lock, when to let go — and play the
 * browser's half back through `fullscreenchange`.
 */

import React, { useState } from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ScreenSharingOverlay } from "../../src/components/sessions/ScreenSharingOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aBridgeRecordingInput } from "../support/rpc/screenSharingInputBridge";
import { desktopPage } from "../support/pages/screenSharingOverlayPage";
import {
  forgetTheFakeBrowser,
  givenABrowserThatCanLockTheKeyboard,
  givenABrowserWithoutKeyboardLock,
  theBrowserLeavesFullScreenOnItsOwn,
} from "../support/fullscreenBrowser";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THE_BRIDGE = "screenshare-bridge-t-desktop-1";
const THE_TRACK = "screenshare:t-desktop-1";

/** A desktop the operator can close — `onClose` really unmounts the overlay, as both scopes do. */
function ADesktopTheOperatorCanClose() {
  const [open, setOpen] = useState(true);
  if (!open) return null;
  return (
    <ScreenSharingOverlay
      room={null}
      bridgeIdentity={THE_BRIDGE}
      trackName={THE_TRACK}
      width={1920}
      height={1080}
      onClose={() => setOpen(false)}
    />
  );
}

function givenAnOpenDesktop(): void {
  mountWithRpc(<ADesktopTheOperatorCanClose />, aBridgeRecordingInput().servedBy(anInMemoryRpcBackend()));
  desktopPage.root().should("exist");
}

/** Assert the stub aliased `alias` ran exactly once, with `this` bound to `expected`. */
function expectCalledOnceOn(alias: string, expected: () => Cypress.Chainable<JQuery<HTMLElement>>) {
  expected().then(($el) => {
    cy.get(alias).should((subject) => {
      const stub = subject as unknown as sinon.SinonStub;
      expect(stub, alias).to.have.been.calledOnce;
      expect(stub.firstCall.thisValue, `${alias} target`).to.equal($el[0]);
    });
  });
}

beforeEach(() => {
  cy.viewport(1200, 800);
  forgetTheFakeBrowser();
});

// ---------------------------------------------------------------------------

describe("Remote desktop — full screen", () => {
  it("hands the desktop viewer to the fullscreen api and not the page", () => {
    // Given an open desktop in a browser that can go full screen
    givenABrowserThatCanLockTheKeyboard();
    givenAnOpenDesktop();

    // When the operator asks for full screen
    desktopPage.toggleFullScreen();

    // Then the overlay itself was handed over — not `document.documentElement`, which would take
    // the whole app full screen with the desktop still a box inside it
    expectCalledOnceOn("@requestFullscreen", desktopPage.root);
  });

  it("offers to exit full screen while full screen", () => {
    // Given a desktop that has gone full screen
    givenABrowserThatCanLockTheKeyboard();
    givenAnOpenDesktop();
    desktopPage.toggleFullScreen();

    // Then the toggle now offers the way back
    desktopPage.expectOffersToExitFullScreen();

    // When the operator takes it
    desktopPage.toggleFullScreen();

    // Then the document left full screen, and the toggle offers to go back in
    cy.get("@exitFullscreen").should("have.been.calledOnce");
    desktopPage.expectOffersToEnterFullScreen();
  });

  it("locks the keyboard on entering full screen where the browser can", () => {
    // Given an open desktop in a browser with Keyboard Lock
    givenABrowserThatCanLockTheKeyboard();
    givenAnOpenDesktop();

    // When the desktop goes full screen
    desktopPage.toggleFullScreen();

    // Then every key was locked — `lock()` with no key list — so Escape, Alt+Tab and the rest
    // reach the remote machine instead of the browser
    cy.get("@keyboardLock").should("have.been.calledOnceWithExactly");
  });

  it("unlocks the keyboard when the browser leaves full screen", () => {
    // Given a full-screen desktop with the keyboard locked
    givenABrowserThatCanLockTheKeyboard();
    givenAnOpenDesktop();
    desktopPage.toggleFullScreen();
    cy.get("@keyboardLock").should("have.been.calledOnce");

    // When the browser leaves full screen by itself, without the page asking
    theBrowserLeavesFullScreenOnItsOwn();

    // Then the lock is released — a lock outliving full screen is one the operator cannot see
    cy.get("@keyboardUnlock").should("have.been.calledOnce");
    desktopPage.expectOffersToEnterFullScreen();
  });

  it("says escape leaves full screen where the keyboard cannot be locked", () => {
    // Given an open desktop in a browser with no Keyboard Lock
    givenABrowserWithoutKeyboardLock();
    givenAnOpenDesktop();

    // When the desktop goes full screen
    desktopPage.toggleFullScreen();

    // Then the operator is told Escape will be taken by the browser, not sent to the desktop
    desktopPage.escapeLeavesFullScreenHint().should("be.visible").invoke("text").should("match", /esc/i);
  });

  it("leaves full screen when the desktop is closed", () => {
    // Given a full-screen desktop with the keyboard locked
    givenABrowserThatCanLockTheKeyboard();
    givenAnOpenDesktop();
    desktopPage.toggleFullScreen();

    // When the operator closes the desktop
    desktopPage.close().click();

    // Then the document is not left full screen on an empty page, and the lock is let go
    desktopPage.root().should("not.exist");
    cy.get("@exitFullscreen").should("have.been.calledOnce");
    cy.get("@keyboardUnlock").should("have.been.calledOnce");
  });
});
