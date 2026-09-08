/**
 * The browser end of `ScreenSharingInputService` — what turns a watched desktop into a usable one.
 *
 * The bridge half already exists (`packages/tddy-screenshare/src/bridge.rs` serves the service and
 * injects each event into the remote desktop); this hook is the client that has never existed. It
 * opens one `StreamInput` stream at the bridge participant, forwards the operator's pointer and
 * keyboard onto it, and closes it when the desktop does.
 *
 * Two things are deliberate and easy to undo by accident:
 *
 * - **Scaling reads the picture's size on every event.** A factor computed once is wrong the moment
 *   the window is resized, and the operator's clicks land somewhere they did not aim.
 * - **Escape is forwarded, not caught.** Every full-screen application on the far side needs it.
 *   `Ctrl+Alt+Esc` is the one chord kept back, so there is still a way out from the keyboard.
 *
 * Transport selection mirrors `useSessionUsage`: a client is built when there is a genuinely
 * connected `Room`, or when the RPC provider was configured with an explicit LiveKit transport
 * factory (those are free to ignore `room` and route somewhere else entirely).
 *
 * PRD: `docs/ft/web/1-WIP/PRD-2026-09-07-input-forwarding.md`
 */

import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
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
import { framebufferPointFor, keysymFor, rfbButtonMaskFor } from "./screenSharingInput";

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

export interface ScreenSharingInputChannel {
  /**
   * True once the bridge has refused the input stream — this desktop can be watched but not
   * touched. The picture is unaffected; only the operator's expectations need correcting.
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
}: ScreenSharingInputOptions): ScreenSharingInputChannel {
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

    // Iterating the bridge's replies is what opens the stream and what surfaces a refusal — the
    // proto lets the bridge answer nothing, so this loop normally waits and yields nothing at all.
    void (async () => {
      try {
        for await (const ack of client.streamInput(outbound)) {
          // `ScreenSharingInputAck` has no fields — one arriving says only that the bridge is still
          // there, which is not something this hook has anything to do about.
          void ack;
          if (cancelled) break;
        }
      } catch (streamError) {
        // Refused: the desktop is watchable but not touchable (a view-only server, a bridge built
        // without the injection path). Not a reason to take the picture away.
        if (!cancelled) {
          console.debug("[useScreenSharingInput] input stream refused", streamError);
          setInputUnavailable(true);
        }
      }
    })();

    return () => {
      cancelled = true;
      // Closing the queue ends the request stream, which is how the bridge learns the desktop is
      // no longer being watched. A stream left open is a bridge left draining input for nobody.
      outbound.close();
      if (outboundRef.current === outbound) {
        outboundRef.current = null;
      }
    };
  }, [client]);

  // Capture: the operator's pointer on the picture, and their keyboard anywhere while it is open.
  useEffect(() => {
    const element = picture.current;
    if (!element) return;

    const sendPointer = (landing: PointerLanding) => {
      outboundRef.current?.enqueue(
        create(ScreenSharingInputEventSchema, { event: { case: "pointer", value: landing } }),
      );
    };

    const sendKey = (keysym: number, pressed: boolean) => {
      outboundRef.current?.enqueue(
        create(ScreenSharingInputEventSchema, { event: { case: "key", value: { keysym, pressed } } }),
      );
    };

    const landingOf = (mouseEvent: MouseEvent): PointerLanding | null => {
      const box = element.getBoundingClientRect();
      // A picture with no size yet has no coordinate space to scale into — the framebuffer point
      // would be `NaN`, which is not a number a `uint32` field can carry.
      if (box.width === 0 || box.height === 0) return null;
      const point = framebufferPointFor(
        { x: mouseEvent.clientX - box.left, y: mouseEvent.clientY - box.top },
        { width: box.width, height: box.height },
        { width: framebufferWidth, height: framebufferHeight },
      );
      return { ...point, buttonMask: rfbButtonMaskFor(mouseEvent.buttons) };
    };

    let pendingMove: PointerLanding | null = null;
    let pendingFrame = 0;

    const flushPendingMove = () => {
      pendingFrame = 0;
      if (!pendingMove) return;
      sendPointer(pendingMove);
      pendingMove = null;
    };

    const onMouseMove = (mouseEvent: MouseEvent) => {
      const landing = landingOf(mouseEvent);
      if (!landing) return;
      // A browser reports motion far faster than a remote desktop consumes it, and the bridge
      // discards the backlog anyway; one move per frame is all the picture can show.
      pendingMove = landing;
      pendingFrame ||= requestAnimationFrame(flushPendingMove);
    };

    const onMouseButton = (mouseEvent: MouseEvent) => {
      const landing = landingOf(mouseEvent);
      if (!landing) return;
      // Never coalesced, and never overtaken by a move still waiting for its frame: a desktop told
      // the button went down before it was told the pointer arrived acts on the wrong place.
      if (pendingFrame) {
        cancelAnimationFrame(pendingFrame);
        flushPendingMove();
      }
      sendPointer(landing);
    };

    const onKey = (keyEvent: KeyboardEvent, pressed: boolean) => {
      if (isTheClosingChord(keyEvent)) {
        keyEvent.preventDefault();
        if (pressed) onCloseChord();
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

    element.addEventListener("mousemove", onMouseMove);
    element.addEventListener("mousedown", onMouseButton);
    element.addEventListener("mouseup", onMouseButton);
    // On the document, not the picture: a `<video>` takes no focus, so keys an operator types while
    // the overlay is up arrive nowhere else.
    document.addEventListener("keydown", onKeyDown);
    document.addEventListener("keyup", onKeyUp);

    return () => {
      element.removeEventListener("mousemove", onMouseMove);
      element.removeEventListener("mousedown", onMouseButton);
      element.removeEventListener("mouseup", onMouseButton);
      document.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("keyup", onKeyUp);
      if (pendingFrame) cancelAnimationFrame(pendingFrame);
    };
  }, [picture, framebufferWidth, framebufferHeight, onCloseChord]);

  return { inputUnavailable };
}
