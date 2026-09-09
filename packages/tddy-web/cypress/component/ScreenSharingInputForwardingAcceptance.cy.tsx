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
 * `XK_Control_L` — the keysym a held Ctrl is forwarded as.
 *
 * Spelled out here rather than imported from the production mapping: a test that asked the code
 * under test what the right answer is would pass whatever that code decided to send.
 */
const CONTROL_KEYSYM = 0xffe3;

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

  it("tells the desktop where the pointer is before it tells it a button went down", () => {
    // Given an open desktop
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);

    // When the operator moves onto a spot and presses there
    desktopPage.movePointerTo({ x: 240, y: 135 });
    desktopPage.pressLeftButtonAt({ x: 240, y: 135 });

    // Then both reached the desktop, and the move reached it first. Movement is coalesced to one
    // event per animation frame; a press that let a still-pending move overtake it would have the
    // desktop click wherever the pointer was *before* the operator aimed.
    bridge.expectPointerLandings(
      { x: 480, y: 270, buttonMask: BUTTON.none },
      { x: 480, y: 270, buttonMask: BUTTON.left },
    );
  });

  it("clears the button mask when the operator lets go outside the picture", () => {
    // Given an open desktop
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);

    // When the operator presses inside the picture and lets go in the letterbox around it — which
    // is what dragging past the edge of a picture that keeps its aspect ratio amounts to
    desktopPage.pressLeftButtonAt(THE_MIDDLE_OF_THE_WINDOW);
    desktopPage.releaseLeftButtonOutsideThePicture();

    // Then the desktop was told the button came up. Without that it goes on dragging: every later
    // move is a selection, every later click is a drop.
    //
    // Reported at the last point the pointer was actually seen inside the picture, because scaling
    // a point outside the element produces a framebuffer coordinate off the end of the desktop —
    // and `button_mask` is what this event is for, not the position.
    bridge.expectPointerLandings(
      { ...THE_MIDDLE_OF_THE_DESKTOP, buttonMask: BUTTON.left },
      { ...THE_MIDDLE_OF_THE_DESKTOP, buttonMask: BUTTON.none },
    );
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

  it("forwards nothing at all for a key that has no keysym", () => {
    // Given an open desktop
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);

    // When the operator hits a key the mapping has nothing for, and then one it does
    desktopPage.tapKey("MediaPlayPause");
    desktopPage.tapKey("a");

    // Then only the second one reached the desktop, and the first left no trace. Both halves are
    // load-bearing: on its own, "the bridge received no MediaPlayPause" is satisfied by a client
    // that forwards nothing whatsoever, and the exact list rules out the other failure — a client
    // sending `keysym ?? 0`, which would put two phantom key events in front of the `a`.
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
// AC-IF-7 — the stream lives exactly as long as the overlay, and leaves nothing held
// ---------------------------------------------------------------------------

describe("What the desktop is left holding when the overlay goes away", () => {
  it("lets go of a modifier the operator was still holding when the desktop closed", () => {
    // Given an open desktop with a modifier held down and never released — which is the ordinary
    // way out of here, since the advertised chord is Ctrl+Alt+Esc and the Escape closes the overlay
    // before either modifier has come up
    const bridge = givenADesktopOpenIn(A_HALF_SIZE_WINDOW);
    desktopPage.pressKey("Control");
    bridge.expectKeyStrokes({ keysym: CONTROL_KEYSYM, pressed: true });

    // When the desktop closes with it still down
    desktopPage.close().click();
    desktopPage.root().should("not.exist");

    // Then the bridge was told the key came up. Nothing else can tell it: the listeners are gone,
    // so the real `keyup` is never forwarded, and a remote machine left holding Ctrl reads every
    // subsequent keystroke as a Ctrl chord.
    //
    // This also pins *where* the release is sent from. The outbound queue is closed in the stream
    // effect's cleanup, and React runs cleanups in the order their effects were defined — so a
    // release flushed from the capture effect's cleanup, which runs second, would be enqueued onto
    // a closed queue and silently dropped, and this assertion would see only the press.
    bridge.expectKeyStrokes(
      { keysym: CONTROL_KEYSYM, pressed: true },
      { keysym: CONTROL_KEYSYM, pressed: false },
    );
  });
});

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
