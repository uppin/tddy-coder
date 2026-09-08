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
  return {
    x: Math.round((pointerInElement.x / renderedSize.width) * framebufferSize.width),
    y: Math.round((pointerInElement.y / renderedSize.height) * framebufferSize.height),
  };
}

/**
 * The X11 keysyms of the keys whose `KeyboardEvent.key` is a name rather than the character it
 * types. Each modifier is the left-hand one: a browser reports which side was pressed in `location`,
 * not in `key`, and a desktop only needs the modifier held — not which of the two produced it.
 */
const KEYSYM_BY_KEY_NAME: Readonly<Record<string, number>> = {
  Enter: 0xff0d, // XK_Return
  Escape: 0xff1b, // XK_Escape
  Backspace: 0xff08, // XK_BackSpace
  Delete: 0xffff, // XK_Delete
  Tab: 0xff09, // XK_Tab
  ArrowLeft: 0xff51, // XK_Left
  ArrowUp: 0xff52, // XK_Up
  ArrowRight: 0xff53, // XK_Right
  ArrowDown: 0xff54, // XK_Down
  Home: 0xff50, // XK_Home
  End: 0xff57, // XK_End
  PageUp: 0xff55, // XK_Page_Up
  PageDown: 0xff56, // XK_Page_Down
  Insert: 0xff63, // XK_Insert
  F1: 0xffbe, // XK_F1 — F1…F12 are consecutive
  F2: 0xffbf,
  F3: 0xffc0,
  F4: 0xffc1,
  F5: 0xffc2,
  F6: 0xffc3,
  F7: 0xffc4,
  F8: 0xffc5,
  F9: 0xffc6,
  F10: 0xffc7,
  F11: 0xffc8,
  F12: 0xffc9,
  Shift: 0xffe1, // XK_Shift_L
  Control: 0xffe3, // XK_Control_L
  Alt: 0xffe9, // XK_Alt_L
  Meta: 0xffeb, // XK_Super_L
  CapsLock: 0xffe5, // XK_Caps_Lock
};

/** The highest code point X11 gives a keysym equal to the code point itself (Latin-1). */
const LAST_LATIN_1_CODE_POINT = 0xff;

/** X11's encoding of any other Unicode code point as a keysym: `0x01000000 | codePoint`. */
const UNICODE_KEYSYM_BASE = 0x01000000;

/**
 * The neutral keysym `ScreenSharingKeyEvent.keysym` carries for a browser `KeyboardEvent.key`.
 *
 * `null` for a key with no keysym at all (media keys, `F13`+, browser-specific keys) — those are
 * dropped rather than sent as some stand-in the remote desktop would act on.
 */
export function keysymFor(key: string): number | null {
  // `hasOwn` rather than `in`: `key` is whatever string the browser reported, and `constructor` or
  // `toString` would otherwise find something on the prototype chain that is not a keysym.
  if (Object.hasOwn(KEYSYM_BY_KEY_NAME, key)) {
    return KEYSYM_BY_KEY_NAME[key];
  }

  // Anything else `key` reports as a single character is the character it types, and the character
  // is what the desktop should receive — `A` and `a` are two keysyms, not one plus a shift flag.
  const codePoint = key.codePointAt(0);
  const isASingleCharacter = codePoint !== undefined && String.fromCodePoint(codePoint) === key;
  if (isASingleCharacter) {
    return codePoint <= LAST_LATIN_1_CODE_POINT ? codePoint : UNICODE_KEYSYM_BASE | codePoint;
  }

  return null;
}

/**
 * The RFB button-mask bit each `MouseEvent.buttons` bit stands for, indexed by the browser's bit.
 *
 * The two orders disagree in the middle: a browser numbers its buttons the way it enumerates them
 * (primary, secondary, auxiliary), RFB the way they sit on a mouse (left, middle, right). Mapping
 * bit for bit — which is what treating one mask as the other amounts to — swaps the middle button
 * for the right one, so every context menu the operator asks for arrives as a paste.
 */
const RFB_MASK_BY_MOUSE_BUTTONS_BIT = [
  0b001, // bit 0, primary   → RFB left
  0b100, // bit 1, secondary → RFB right
  0b010, // bit 2, auxiliary → RFB middle
];

/**
 * The RFB button mask for the set of buttons a `MouseEvent` reports as held (`MouseEvent.buttons`).
 *
 * The set, not the button that triggered the event: `ScreenSharingPointerEvent.button_mask` says
 * what is down *now*, so a release is the remaining buttons rather than the one let go of, and a
 * drag carries its held button along with every move. Browser-only buttons (back, forward) have no
 * RFB bit and are left out rather than folded onto one that means something else.
 */
export function rfbButtonMaskFor(heldButtons: number): number {
  return RFB_MASK_BY_MOUSE_BUTTONS_BIT.reduce(
    (mask, rfbBit, browserBit) => (heldButtons & (1 << browserBit) ? mask | rfbBit : mask),
    0,
  );
}
