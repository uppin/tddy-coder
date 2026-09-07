/**
 * Acceptance tests: a remote desktop that accepts input.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-07-input-forwarding.md — AC-IF-1, AC-IF-2, AC-IF-3, AC-IF-6,
 * AC-IF-7, AC-IF-8, AC-IF-9. The two scopes that mount this overlay are covered by
 * `ScreenSharingInputScopesAcceptance` (AC-IF-4, AC-IF-5).
 *
 * ⚠ Every assertion here is about a `ScreenSharingInputEvent` **the bridge received**, never about
 * a handler having run. The bridge half of this feature is built and shipped
 * (`packages/tddy-screenshare/src/bridge.rs` serves `ScreenSharingInputService` and injects each
 * event); the only thing that has never existed is a browser client for it. So the one question
 * worth asking of the browser is what came out the other end of the stream — a spec that watched
 * `onMouseDown` fire would go green against a page that opens no stream at all.
 */

import React, { useState } from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ScreenSharingOverlay } from "../../src/components/sessions/ScreenSharingOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aBridgeRecordingInput,
  aBridgeRefusingInput,
  BUTTON,
  KEYSYM,
  type AFakeBridge,
} from "../support/rpc/screenSharingInputBridge";
import { desktopPage, type Size } from "../support/pages/screenSharingOverlayPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** The bridge participant a start reply names — the identity the input stream is addressed at. */
const THE_BRIDGE = "screenshare-bridge-t-desktop-1";
const THE_TRACK = "screenshare:t-desktop-1";

/** The desktop on the other end: an ordinary full-HD screen. */
const THE_DESKTOP: Size = { width: 1920, height: 1080 };

/** The window it is being watched in — half size, so a raw browser coordinate is visibly wrong. */
const A_HALF_SIZE_WINDOW: Size = { width: 960, height: 540 };

/** Half-size window ⇒ the middle of the picture is the middle of the desktop. */
const THE_MIDDLE_OF_THE_WINDOW = { x: 480, y: 270 };
const THE_MIDDLE_OF_THE_DESKTOP = { x: 960, y: 540 };

/**
 * A desktop the operator can close — an overlay that goes away, as both real scopes' do.
 *
 * `onClose` has to actually unmount something, or "the chord closed the overlay" and "the stream
 * did not outlive the overlay" are both unassertable.
 */
function ADesktopTheOperatorCanClose({ desktop }: { desktop: Size }) {
  const [open, setOpen] = useState(true);
  if (!open) return null;
  return (
    <ScreenSharingOverlay
      room={null}
      bridgeIdentity={THE_BRIDGE}
      trackName={THE_TRACK}
      width={desktop.width}
      height={desktop.height}
      onClose={() => setOpen(false)}
    />
  );
}

/** An open desktop, watched in `window`, whose bridge records the input it is sent. */
function givenADesktopOpenIn(window: Size, desktop: Size = THE_DESKTOP): AFakeBridge {
  const bridge = aBridgeRecordingInput();
  mountWithRpc(
    <ADesktopTheOperatorCanClose desktop={desktop} />,
    bridge.servedBy(anInMemoryRpcBackend()),
  );
  desktopPage.renderedAt(window);
  return bridge;
}

beforeEach(() => {
  cy.viewport(1200, 800);
});

// ---------------------------------------------------------------------------
// AC-IF-1 — the pointer reaches the desktop
// ---------------------------------------------------------------------------

describe("Pointer forwarding", () => {
  it("sends a button press and its release to the desktop", () => {
    // Given an open desktop
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);

    // When the operator clicks in the middle of it
    desktopPage.pressLeftButtonAt(THE_MIDDLE_OF_THE_WINDOW);
    desktopPage.releaseLeftButtonAt(THE_MIDDLE_OF_THE_WINDOW);

    // Then the bridge was told the button went down and then came up again. Both halves matter: a
    // client that sent only the press leaves the remote desktop dragging for ever.
    bridge.expectPointerLandings(
      { ...THE_MIDDLE_OF_THE_DESKTOP, buttonMask: BUTTON.left },
      { ...THE_MIDDLE_OF_THE_DESKTOP, buttonMask: BUTTON.none },
    );
  });

  it("sends pointer movement with no button held", () => {
    // Given an open desktop
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);

    // When the operator moves the pointer over it without pressing anything
    desktopPage.movePointerTo({ x: 240, y: 135 });

    // Then the desktop's own pointer moved there, and nothing was reported as held down
    bridge.expectPointerLandings({ x: 480, y: 270, buttonMask: BUTTON.none });
  });

  it("distinguishes the right button from the left", () => {
    // Given an open desktop
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);

    // When the operator presses the right button — the one that opens a context menu over there
    desktopPage.pressRightButtonAt(THE_MIDDLE_OF_THE_WINDOW);

    // Then the mask says which button, rather than "a button": a client hard-coding the left one
    // gives an operator no way to right-click anything on the remote machine.
    bridge.expectPointerLandings({ ...THE_MIDDLE_OF_THE_DESKTOP, buttonMask: BUTTON.right });
  });
});

// ---------------------------------------------------------------------------
// AC-IF-3 — a click lands where the operator aimed
// ---------------------------------------------------------------------------

describe("Pointer coordinates", () => {
  it("lands the click where the operator aimed at either window size", () => {
    // Given a full-HD desktop being watched at half size
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);

    // When the operator presses at the middle of the picture
    desktopPage.pressLeftButtonAt(THE_MIDDLE_OF_THE_WINDOW);

    // …and then the window becomes a quarter size, and they press at the same spot in the picture
    desktopPage.renderedAt({ width: 480, height: 270 });
    desktopPage.pressLeftButtonAt(THE_MIDDLE_OF_THE_WINDOW);

    // Then the desktop was pointed at in its own pixels both times, at two different places: the
    // same offset is the middle of a half-size window and the far corner of a quarter-size one. A
    // client forwarding raw browser coordinates sends 480,270 twice and fails on the first; one
    // scaling by a factor fixed at mount sends 960,540 twice and fails on the second.
    bridge.expectPointerLandings(
      { x: 960, y: 540, buttonMask: BUTTON.left },
      { x: 1920, y: 1080, buttonMask: BUTTON.left },
    );
  });
});

// ---------------------------------------------------------------------------
// AC-IF-2 — the keyboard reaches the desktop
// ---------------------------------------------------------------------------

describe("Keyboard forwarding", () => {
  it("sends a printable key down and up", () => {
    // Given an open desktop
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);

    // When the operator types a letter
    desktopPage.tapKey("a");

    // Then the desktop saw that key pressed and released, as its own keysym
    bridge.expectKeyStrokes(
      { keysym: KEYSYM.a, pressed: true },
      { keysym: KEYSYM.a, pressed: false },
    );
  });

  it("sends the special keys a desktop is unusable without", () => {
    // Given an open desktop
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);

    // When the operator uses the keys that are not characters
    desktopPage.tapKey("Enter");
    desktopPage.tapKey("Tab");
    desktopPage.tapKey("ArrowUp");
    desktopPage.tapKey("F1");

    // Then each reached the desktop as its own keysym rather than being dropped for having no
    // character to send
    bridge.expectKeyStrokes(
      { keysym: KEYSYM.Enter, pressed: true },
      { keysym: KEYSYM.Enter, pressed: false },
      { keysym: KEYSYM.Tab, pressed: true },
      { keysym: KEYSYM.Tab, pressed: false },
      { keysym: KEYSYM.ArrowUp, pressed: true },
      { keysym: KEYSYM.ArrowUp, pressed: false },
      { keysym: KEYSYM.F1, pressed: true },
      { keysym: KEYSYM.F1, pressed: false },
    );
  });
});

// ---------------------------------------------------------------------------
// AC-IF-9 / AC-IF-6 — the one chord the overlay keeps
// ---------------------------------------------------------------------------

describe("Escape and the chord that closes the desktop", () => {
  it("sends Escape to the desktop instead of closing the overlay", () => {
    // Given an open desktop
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);

    // When the operator presses Escape — leaving vim's insert mode, dismissing a dialog over there
    desktopPage.tapKey("Escape");

    // Then the desktop got it
    bridge.expectKeyStrokes(
      { keysym: KEYSYM.Escape, pressed: true },
      { keysym: KEYSYM.Escape, pressed: false },
    );

    // …and the overlay is still open. Escape used to dismiss it, which made every full-screen
    // application on the remote machine unreachable.
    desktopPage.root().should("exist");
  });

  it("closes on Ctrl+Alt+Esc without sending it to the desktop", () => {
    // Given an open desktop that is forwarding what the operator types — asserted, not assumed,
    // because "nothing was forwarded" would otherwise satisfy this test's absence claim on its own
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);
    desktopPage.tapKey("a");
    bridge.expectKeyStrokes(
      { keysym: KEYSYM.a, pressed: true },
      { keysym: KEYSYM.a, pressed: false },
    );

    // When the operator presses the chord the overlay reserves for itself
    desktopPage.tapKey("Escape", { ctrlKey: true, altKey: true });

    // Then the desktop closed
    desktopPage.root().should("not.exist");

    // …and no Escape was ever sent to the machine on the other side, which would have dismissed
    // whatever the operator had open there on the way out
    bridge.expectNoKeyStroke(
      KEYSYM.Escape,
      "the chord the overlay keeps must not also reach the desktop",
    );
  });

  it("names the chord that closes it", () => {
    // Given an open desktop
    givenADesktopOpenIn(A_HALF_SIZE_WINDOW);

    // Then the way out is on screen: every other key goes to the remote machine, so an operator who
    // has not memorised this one has only the mouse left
    desktopPage.expectNamesTheClosingChord();
  });
});

// ---------------------------------------------------------------------------
// AC-IF-7 — the stream lives exactly as long as the overlay
// ---------------------------------------------------------------------------

describe("The input stream's lifetime", () => {
  it("opens one stream for an open desktop and ends it when the desktop closes", () => {
    // Given an open desktop
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);

    // Then exactly one input stream is open against the bridge — one desktop is not two streams,
    // and a re-render is not a reconnect
    bridge.expectOneOpenStream();

    // When the operator closes the desktop
    desktopPage.close().click();
    desktopPage.root().should("not.exist");

    // Then the stream ended with it. A stream left open is a bridge left draining input for a
    // window nobody is looking at.
    bridge.expectTheStreamClosed();
  });
});

// ---------------------------------------------------------------------------
// AC-IF-8 — a desktop that can be watched but not touched
// ---------------------------------------------------------------------------

describe("A desktop whose input stream cannot open", () => {
  it("keeps the picture and says input is unavailable", () => {
    // Given a desktop whose bridge takes input, and is taking it
    const working = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);
    desktopPage.tapKey("a");
    working.expectKeyStrokes(
      { keysym: KEYSYM.a, pressed: true },
      { keysym: KEYSYM.a, pressed: false },
    );

    // …which says nothing about input being unavailable, or the notice below would mean nothing
    desktopPage.inputUnavailableNotice().should("not.exist");

    // When the same overlay is opened against a bridge that refuses the input stream
    mountWithRpc(
      <ADesktopTheOperatorCanClose desktop={THE_DESKTOP} />,
      aBridgeRefusingInput().servedBy(anInMemoryRpcBackend()),
    );

    // Then the operator is told, rather than left clicking into a surface that ignores them
    desktopPage.inputUnavailableNotice().should("exist");

    // …and the desktop is still there to watch, which is most of why it was opened
    desktopPage.picture().should("exist");
  });
});
