/**
 * A bridge that records the input it was actually sent — the double every input-forwarding spec
 * asserts against.
 *
 * The real bridge (`packages/tddy-screenshare/src/bridge.rs`) serves `ScreenSharingInputService`
 * over its LiveKit data channel and injects each event into the remote desktop. Nothing in a
 * browser test can watch a desktop move, so the last observable point on this side of the wire is
 * the stream itself: what arrived on it, in what order, and whether it was open.
 *
 * ⚠ This is the whole reason the double exists. A spec asserting that `onMouseDown` fired, or that
 * some handler was called, proves nothing about whether a single byte left the page — which is
 * exactly the failure this stack has already produced seven times. Every assertion here is about a
 * `ScreenSharingInputEvent` the bridge received.
 */

import { ConnectError, Code } from "@connectrpc/connect";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ScreenSharingInputService,
  type ScreenSharingInputEvent,
} from "../../../src/gen/screen_sharing_input_pb";

// ---------------------------------------------------------------------------
// What a spec talks about
// ---------------------------------------------------------------------------

/** Where a pointer landed on the framebuffer, and which buttons were down when it did. */
export interface PointerLanding {
  readonly x: number;
  readonly y: number;
  readonly buttonMask: number;
}

/** A key the desktop was told about, and whether it went down or came up. */
export interface KeyStroke {
  readonly keysym: number;
  readonly pressed: boolean;
}

/** The X11 keysyms the specs name, so a test body reads a key rather than a hex literal. */
export const KEYSYM = {
  a: 0x61,
  Enter: 0xff0d,
  Escape: 0xff1b,
  Tab: 0xff09,
  ArrowUp: 0xff52,
  F1: 0xffbe,
} as const;

/** RFB button-mask bits, as `ScreenSharingPointerEvent.button_mask` carries them. */
export const BUTTON = {
  none: 0,
  left: 1,
  middle: 2,
  right: 4,
} as const;

// ---------------------------------------------------------------------------
// The double
// ---------------------------------------------------------------------------

export interface AFakeBridge {
  /** Serve this bridge's `ScreenSharingInputService` from `backend`, and return that backend. */
  servedBy(backend: InMemoryRpcBackend): InMemoryRpcBackend;

  /** Every pointer event the bridge received, in arrival order. */
  pointerLandings(): PointerLanding[];
  /** Every key event the bridge received, in arrival order. */
  keyStrokes(): KeyStroke[];

  /**
   * Assert the pointer landings the bridge received are exactly these, in this order.
   *
   * Exact rather than "contains": a client that also sent the operator's raw window coordinates
   * alongside the scaled ones would satisfy a containment check while putting the pointer in two
   * places on the desktop.
   */
  expectPointerLandings(...expected: PointerLanding[]): void;

  /** Assert the key strokes the bridge received are exactly these, in this order. */
  expectKeyStrokes(...expected: KeyStroke[]): void;

  /**
   * Assert no key with this keysym ever reached the bridge.
   *
   * Never use alone: an absence holds trivially for a client that forwards nothing at all. Pair it
   * with an {@link expectKeyStrokes} in the same test that proves this client does forward.
   */
  expectNoKeyStroke(keysym: number, because: string): void;

  /** Assert exactly one input stream was opened, and that it is still open. */
  expectOneOpenStream(): void;

  /** Assert the one stream that was opened has since ended. */
  expectTheStreamClosed(): void;
}

/**
 * A bridge that accepts an input stream and records everything sent on it.
 *
 * Records are plain arrays read through `cy.wrap(...).should(...)`, so an assertion retries while
 * the event makes its way across the transport rather than reading once and racing it.
 */
export function aBridgeRecordingInput(): AFakeBridge {
  const received: ScreenSharingInputEvent[] = [];
  let opened = 0;
  let closed = 0;

  async function* streamInput(requests: AsyncIterable<ScreenSharingInputEvent>) {
    opened += 1;
    try {
      for await (const event of requests) {
        received.push(event);
      }
    } finally {
      // Reached however the browser lets go — closing its outbound queue, aborting, or simply
      // dropping the response iterable when the overlay unmounts.
      closed += 1;
    }
    // The proto's ack is optional ("the server side drains input and may send nothing"), and this
    // double sends none: a bridge that answered every event would test the client's ack handling
    // instead of what it sent.
  }

  const pointerLandings = (): PointerLanding[] =>
    received
      .filter((e) => e.event.case === "pointer")
      .map((e) => {
        const p = e.event.value as { x: number; y: number; buttonMask: number };
        return { x: p.x, y: p.y, buttonMask: p.buttonMask };
      });

  const keyStrokes = (): KeyStroke[] =>
    received
      .filter((e) => e.event.case === "key")
      .map((e) => {
        const k = e.event.value as { keysym: number; pressed: boolean };
        return { keysym: k.keysym, pressed: k.pressed };
      });

  const bridge: AFakeBridge = {
    servedBy: (backend) => backend.implement(ScreenSharingInputService, { streamInput }),
    pointerLandings,
    keyStrokes,

    expectPointerLandings: (...expected) => {
      cy.wrap(null, { log: false }).should(() => {
        expect(pointerLandings(), "the pointer events the bridge received").to.deep.equal(expected);
      });
    },

    expectKeyStrokes: (...expected) => {
      cy.wrap(null, { log: false }).should(() => {
        expect(keyStrokes(), "the key events the bridge received").to.deep.equal(expected);
      });
    },

    expectNoKeyStroke: (keysym, because) => {
      cy.wrap(null, { log: false }).should(() => {
        expect(
          keyStrokes().filter((k) => k.keysym === keysym),
          because,
        ).to.deep.equal([]);
      });
    },

    expectOneOpenStream: () => {
      cy.wrap(null, { log: false }).should(() => {
        expect(opened, "input streams opened against the bridge").to.equal(1);
        expect(closed, "input streams that have already ended").to.equal(0);
      });
    },

    expectTheStreamClosed: () => {
      cy.wrap(null, { log: false }).should(() => {
        expect(opened, "input streams opened against the bridge").to.equal(1);
        expect(closed, "input streams that have ended").to.equal(1);
      });
    },
  };

  return bridge;
}

/**
 * A bridge whose desktop cannot take input at all — the stream is refused the moment it is opened.
 *
 * A real one refuses for reasons the browser cannot fix: an RDP server in view-only mode, a bridge
 * built without the injection path. The overlay's job is to keep showing the picture and say so.
 */
export function aBridgeRefusingInput(): { servedBy(backend: InMemoryRpcBackend): InMemoryRpcBackend } {
  return {
    servedBy: (backend) =>
      backend.implement(ScreenSharingInputService, {
        // eslint-disable-next-line require-yield
        async *streamInput() {
          throw new ConnectError("this desktop does not accept input", Code.Unimplemented);
        },
      }),
  };
}
