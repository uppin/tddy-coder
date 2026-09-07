/**
 * The two pure translations an input-forwarding overlay makes before anything reaches the wire.
 *
 * A browser reports a pointer in the rendered element's own pixels and a key as a `KeyboardEvent.key`
 * string; `ScreenSharingInputEvent` carries framebuffer pixels and a neutral keysym. Both mappings
 * are total functions of their inputs, so they are pinned here as such rather than only through a
 * mounted overlay — a click that lands in the wrong place is a scaling bug, not a UI bug.
 *
 * The superseded `vncInput.ts` (of the retired `VncOverlay` generation) holds the same logic; this
 * module is where the surviving surface keeps it.
 */

/** A size in CSS pixels — the rendered video element, or the desktop's framebuffer. */
export interface Size {
  readonly width: number;
  readonly height: number;
}

/** A point, in whichever space the parameter it is passed as names. */
export interface Point {
  readonly x: number;
  readonly y: number;
}

/**
 * Where a pointer inside the rendered video element lands on the remote framebuffer.
 *
 * `pointerInElement` is measured from the element's top-left corner, so the operator's aim is
 * expressed in the picture they are actually looking at, whatever size the window happens to be.
 * Results are rounded: the framebuffer is addressed in whole pixels.
 */
export function framebufferPointFor(
  pointerInElement: Point,
  renderedSize: Size,
  framebufferSize: Size,
): Point {
  void pointerInElement;
  void renderedSize;
  void framebufferSize;
  throw new Error("framebufferPointFor is not implemented");
}

/**
 * The neutral keysym `ScreenSharingKeyEvent.keysym` carries for a browser `KeyboardEvent.key`.
 *
 * `null` for a key with no keysym at all (media keys, `F13`+, browser-specific keys) — those are
 * dropped rather than sent as some stand-in the remote desktop would act on.
 */
export function keysymFor(key: string): number | null {
  void key;
  throw new Error("keysymFor is not implemented");
}
