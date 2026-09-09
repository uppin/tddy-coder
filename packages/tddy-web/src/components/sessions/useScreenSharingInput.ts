/**
 * The browser end of `ScreenSharingInputService` — what turns a watched desktop into a usable one.
 *
 * The bridge half already exists (`packages/tddy-screenshare/src/bridge.rs` serves the service and
 * injects each event into the remote desktop); this hook is the client that has never existed. It
 * opens one `StreamInput` stream at the bridge participant, forwards the operator's pointer and
 * keyboard onto it, and closes it when the desktop does.
 *
 * Four things are deliberate and easy to undo by accident:
 *
 * - **Scaling reads the picture's size on every event.** A factor computed once is wrong the moment
 *   the window is resized, and the operator's clicks land somewhere they did not aim.
 * - **Escape is forwarded, not caught.** Every full-screen application on the far side needs it.
 *   `Ctrl+Alt+Esc` is the one chord kept back, so there is still a way out from the keyboard.
 * - **The stream is opened eagerly**, with one empty event. A stream nothing has been written to has
 *   not reached the bridge at all — see the note where that event is enqueued.
 * - **Everything still held is let go of on teardown.** A modifier or a mouse button the operator
 *   was holding when the overlay went away stays down on the remote machine otherwise, and every
 *   later keystroke there arrives as `Ctrl+Alt+key`.
 *
 * Transport selection mirrors `useSessionUsage`: a client is built when there is a genuinely
 * connected `Room`, or when the RPC provider was configured with an explicit LiveKit transport
 * factory (those are free to ignore `room` and route somewhere else entirely).
 *
 * PRD: `docs/ft/web/1-WIP/PRD-2026-09-07-input-forwarding.md`
 */

import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import { AsyncQueue } from "tddy-livekit-web";
import {
  useLiveKitTransportFactory,
  useLiveKitTransportFactoryIsOverridden,
} from "../../rpc/transportProvider";
import {
  ScreenSharingInputEventSchema,
  ScreenSharingInputService,
  type ScreenSharingInputEvent,
} from "../../gen/screen_sharing_input_pb";
import {
  framebufferPointFor,
  keysymFor,
  rfbButtonMaskFor,
  type Point,
} from "./screenSharingInput";

export interface ScreenSharingInputOptions {
  /** The rendered picture. Pointer positions are read against this element's current box. */
  readonly picture: RefObject<HTMLElement | null>;
  /** The room the bridge is in; `null` until it is connected. */
  readonly room: Room | null;
  /** The LiveKit participant the input stream is addressed at — this desktop's own bridge. */
  readonly bridgeIdentity: string;
  /** The remote framebuffer's size, which is what the events are expressed in. */
  readonly framebufferWidth: number;
  readonly framebufferHeight: number;
  /** Called when the operator presses the one chord the overlay keeps for itself. */
  readonly onCloseChord: () => void;
}

/** What this hook can tell the overlay about the input stream. It carries no way to send anything. */
export interface ScreenSharingInputStatus {
  /**
   * True once there is no input stream to send on — this desktop can be watched but not touched.
   * The picture is unaffected; only the operator's expectations need correcting.
   *
   * What actually gets here: the peer does not serve `ScreenSharingInputService` at all (nothing is
   * listening on that participant, so opening the stream fails), or a stream that *was* open ended
   * on its own — the bridge restarted, or its remote desktop went away.
   *
   * What does **not** get here, contrary to what an earlier version of this comment claimed: a
   * desktop that cannot actually accept injection. `stream_input` always answers `Ok`
   * (`packages/tddy-screenshare/src/bridge.rs`), and a bridge that cannot inject drops the commands
   * without telling anyone. A view-only server is silently ignored, not reported.
   */
  readonly inputUnavailable: boolean;
}

/** Where a pointer landed on the remote framebuffer, and which buttons were down when it did. */
interface PointerLanding {
  readonly x: number;
  readonly y: number;
  readonly buttonMask: number;
}

/** The chord the overlay keeps: it closes the desktop, and is the only key that never reaches it. */
function isTheClosingChord(event: KeyboardEvent): boolean {
  return event.key === "Escape" && event.ctrlKey && event.altKey;
}

export function useScreenSharingInput({
  picture,
  room,
  bridgeIdentity,
  framebufferWidth,
  framebufferHeight,
  onCloseChord,
}: ScreenSharingInputOptions): ScreenSharingInputStatus {
  const liveKitFactory = useLiveKitTransportFactory();
  const factoryIsOverridden = useLiveKitTransportFactoryIsOverridden();
  const canBuildClient = room !== null || factoryIsOverridden;

  // One client per bridge — a re-render is not a reconnect, and a different bridge is a different
  // desktop. `null` when there is no room yet and no configured factory to route through instead.
  const client = useMemo(() => {
    if (!canBuildClient) return null;
    return createClient(ScreenSharingInputService, liveKitFactory(room as Room, bridgeIdentity));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [liveKitFactory, canBuildClient, room, bridgeIdentity]);

  const [inputUnavailable, setInputUnavailable] = useState(false);
  const outboundRef = useRef<AsyncQueue<ScreenSharingInputEvent> | null>(null);

  // What the remote desktop currently believes is held down, and the last place the pointer was
  // actually seen inside the picture.
  //
  // These live out here, above both effects, on purpose: the release that has to happen when the
  // overlay goes away belongs to the *stream* effect's cleanup (see the note there), while the
  // presses that fill them in belong to the capture effect's listeners.
  const heldKeysyms = useRef<Set<number>>(new Set());
  const heldButtonMask = useRef(0);
  const lastPointInPicture = useRef<Point | null>(null);

  // Held in a ref rather than read from the capture effect's dependencies: both call sites build a
  // new `onCloseChord` on every render, and re-running that effect rebinds five listeners and
  // cancels a pending pointer move that then never gets sent.
  const onCloseChordRef = useRef(onCloseChord);
  useEffect(() => {
    onCloseChordRef.current = onCloseChord;
  }, [onCloseChord]);

  const sendPointer = useCallback((landing: PointerLanding) => {
    // What the desktop has been told is held is exactly what was last sent to it, so it is recorded
    // here rather than at each call site — the teardown release depends on this being the truth.
    heldButtonMask.current = landing.buttonMask;
    outboundRef.current?.enqueue(
      create(ScreenSharingInputEventSchema, { event: { case: "pointer", value: landing } }),
    );
  }, []);

  const sendKey = useCallback((keysym: number, pressed: boolean) => {
    if (pressed) heldKeysyms.current.add(keysym);
    else heldKeysyms.current.delete(keysym);
    outboundRef.current?.enqueue(
      create(ScreenSharingInputEventSchema, { event: { case: "key", value: { keysym, pressed } } }),
    );
  }, []);

  /**
   * Let go, on the remote desktop, of everything the operator is still holding here.
   *
   * A keysym is released where it was pressed from; a button is released at the last point inside
   * the picture, because a point outside it scales to a framebuffer coordinate off the end of the
   * desktop and `uint32` has nowhere to put a negative one.
   */
  const releaseEverythingHeld = useCallback(() => {
    const stillHeld = [...heldKeysyms.current];
    heldKeysyms.current.clear();
    for (const keysym of stillHeld) sendKey(keysym, false);

    const at = lastPointInPicture.current;
    if (heldButtonMask.current !== 0 && at) sendPointer({ ...at, buttonMask: 0 });
    heldButtonMask.current = 0;
  }, [sendKey, sendPointer]);

  // The outbound stream: open for exactly as long as this hook is mounted.
  useEffect(() => {
    setInputUnavailable(false);
    if (!client) {
      outboundRef.current = null;
      return;
    }

    const outbound = new AsyncQueue<ScreenSharingInputEvent>();
    outboundRef.current = outbound;
    let cancelled = false;

    // Open the stream eagerly. The LiveKit transport publishes the `call` frame — the one that makes
    // the peer dispatch the method — only with the first enqueued request message, so a stream
    // nothing has been written to has not reached the bridge at all, and the desktop would stay dead
    // until the operator's first mouse move. An event with no `event` case set is a safe opener: the
    // bridge skips it (`packages/tddy-screenshare/src/bridge.rs`, `None => continue`).
    outbound.enqueue(create(ScreenSharingInputEventSchema, {}));

    void (async () => {
      try {
        for await (const ack of client.streamInput(outbound)) {
          // The bridge acks every event it queues for injection (`bridge.rs`), so these arrive
          // steadily; one says only that it is still there, which is not something this hook has
          // anything to do about.
          void ack;
          if (cancelled) break;
        }
        // The response stream ended by itself. Nothing will be injected any more, so stop
        // publishing into a call nobody is answering and tell the operator — a live picture with a
        // dead keyboard and no notice is the worst of the three states.
        if (!cancelled) {
          outbound.close();
          setInputUnavailable(true);
        }
      } catch (streamError) {
        if (!cancelled) {
          console.debug("[useScreenSharingInput] input stream unavailable", streamError);
          // `AsyncQueue` is unbounded and the transport's publish is fire-and-forget: without this
          // close, every later pointer move is still encoded and published, for ever, for a stream
          // that is not there.
          outbound.close();
          setInputUnavailable(true);
        }
      }
    })();

    return () => {
      cancelled = true;
      // Before the queue closes, and in *this* effect rather than the capture one: React runs effect
      // cleanups in the order their effects were defined, so this cleanup runs first and anything
      // the capture cleanup enqueued would land in an already-closed queue and be dropped.
      releaseEverythingHeld();
      // Closing the queue ends the request stream, which is how the bridge learns the desktop is
      // no longer being watched. A stream left open is a bridge left draining input for nobody.
      outbound.close();
      if (outboundRef.current === outbound) {
        outboundRef.current = null;
      }
    };
  }, [client, releaseEverythingHeld]);

  // Capture: the operator's pointer on the picture, and their keyboard anywhere while it is open.
  useEffect(() => {
    const element = picture.current;
    if (!element) return;

    /** The picture's box, or `null` while it has no size to scale into. */
    const measuredBox = (): DOMRect | null => {
      const box = element.getBoundingClientRect();
      // A picture with no size yet has no coordinate space to scale into — the framebuffer point
      // would be `NaN`, which is not a number a `uint32` field can carry.
      if (box.width === 0 || box.height === 0) return null;
      return box;
    };

    const isOverThePicture = (box: DOMRect, mouseEvent: MouseEvent): boolean =>
      mouseEvent.clientX >= box.left &&
      mouseEvent.clientX <= box.right &&
      mouseEvent.clientY >= box.top &&
      mouseEvent.clientY <= box.bottom;

    /** Where this event landed, remembered as the last point the pointer was seen at. */
    const landingOf = (mouseEvent: MouseEvent, box: DOMRect): PointerLanding => {
      const point = framebufferPointFor(
        { x: mouseEvent.clientX - box.left, y: mouseEvent.clientY - box.top },
        { width: box.width, height: box.height },
        { width: framebufferWidth, height: framebufferHeight },
      );
      lastPointInPicture.current = point;
      return { ...point, buttonMask: rfbButtonMaskFor(mouseEvent.buttons) };
    };

    let pendingMove: PointerLanding | null = null;
    // `0` doubles as "nothing pending" rather than carrying a second flag alongside the handle:
    // `requestAnimationFrame` never returns 0, so a live handle is always truthy.
    let pendingFrame = 0;

    const flushPendingMove = () => {
      pendingFrame = 0;
      if (!pendingMove) return;
      sendPointer(pendingMove);
      pendingMove = null;
    };

    /** Send a landing that must not be coalesced, after whatever move is still waiting for a frame. */
    const sendAfterAnyPendingMove = (landing: PointerLanding) => {
      // A desktop told the button went down before it was told the pointer arrived acts on the
      // wrong place.
      if (pendingFrame) {
        cancelAnimationFrame(pendingFrame);
        flushPendingMove();
      }
      sendPointer(landing);
    };

    const onMouseMove = (mouseEvent: MouseEvent) => {
      const box = measuredBox();
      if (!box) return;
      // Bound to the picture, so a drag that leaves it simply stops reporting: scaling a point
      // outside the element would send framebuffer coordinates off the end of the desktop.
      const landing = landingOf(mouseEvent, box);
      // A browser reports motion far faster than a data channel can carry it or a remote desktop can
      // act on it, and the bridge injects every event it has been queued — it drains the whole
      // backlog before each frame (`bridge.rs`), so a move sent is a move paid for. One per frame is
      // all the picture can show anyway.
      pendingMove = landing;
      pendingFrame ||= requestAnimationFrame(flushPendingMove);
    };

    const onMouseDown = (mouseEvent: MouseEvent) => {
      const box = measuredBox();
      if (!box) return;
      sendAfterAnyPendingMove(landingOf(mouseEvent, box));
    };

    // On the window, not the picture. The picture keeps its aspect ratio inside a full-screen
    // container, so there is letterbox around it: press inside, drag out, release there, and the
    // picture never sees the `mouseup` — `button_mask` never clears and the desktop drags for ever.
    const onMouseUp = (mouseEvent: MouseEvent) => {
      const box = measuredBox();
      if (box && isOverThePicture(box, mouseEvent)) {
        sendAfterAnyPendingMove(landingOf(mouseEvent, box));
        return;
      }
      // Outside the picture. The mask has to clear, but the coordinate cannot come from this event,
      // so the release is reported where the pointer was last actually seen. Only when something is
      // held: an ordinary click elsewhere in the page is not this desktop's business.
      const at = lastPointInPicture.current;
      if (heldButtonMask.current === 0 || !at) return;
      sendAfterAnyPendingMove({ ...at, buttonMask: rfbButtonMaskFor(mouseEvent.buttons) });
    };

    const onContextMenu = (mouseEvent: MouseEvent) => {
      // The right-button press has already been forwarded and the remote desktop is opening its own
      // menu; the browser's would land on top of it, over the very thing the operator asked for.
      mouseEvent.preventDefault();
    };

    const onKey = (keyEvent: KeyboardEvent, pressed: boolean) => {
      if (isTheClosingChord(keyEvent)) {
        keyEvent.preventDefault();
        if (pressed) onCloseChordRef.current();
        return;
      }
      const keysym = keysymFor(keyEvent.key);
      // No keysym means there is nothing to send — so there is nothing to swallow either, and the
      // browser keeps the key rather than having it silently disappear into a desktop it never
      // reached.
      if (keysym === null) return;
      keyEvent.preventDefault();
      sendKey(keysym, pressed);
    };

    const onKeyDown = (keyEvent: KeyboardEvent) => onKey(keyEvent, true);
    const onKeyUp = (keyEvent: KeyboardEvent) => onKey(keyEvent, false);

    // Tabbing away mid-keypress: the `keyup` happens in a window that is not this one, so the key
    // would stay held on the remote machine until the desktop is closed.
    const onWindowBlur = () => releaseEverythingHeld();

    element.addEventListener("mousemove", onMouseMove);
    element.addEventListener("mousedown", onMouseDown);
    element.addEventListener("contextmenu", onContextMenu);
    window.addEventListener("mouseup", onMouseUp);
    window.addEventListener("blur", onWindowBlur);
    // On the document, not the picture: a `<video>` takes no focus, so keys an operator types while
    // the overlay is up arrive nowhere else.
    document.addEventListener("keydown", onKeyDown);
    document.addEventListener("keyup", onKeyUp);

    return () => {
      element.removeEventListener("mousemove", onMouseMove);
      element.removeEventListener("mousedown", onMouseDown);
      element.removeEventListener("contextmenu", onContextMenu);
      window.removeEventListener("mouseup", onMouseUp);
      window.removeEventListener("blur", onWindowBlur);
      document.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("keyup", onKeyUp);
      if (pendingFrame) cancelAnimationFrame(pendingFrame);
    };
  }, [picture, framebufferWidth, framebufferHeight, releaseEverythingHeld, sendKey, sendPointer]);

  return { inputUnavailable };
}
