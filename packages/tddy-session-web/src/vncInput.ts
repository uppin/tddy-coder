/**
 * VNC input helpers — coordinate scaling and keyboard keysym mapping.
 *
 * Used by `VncOverlay` to convert browser pointer/keyboard events into the values
 * expected by the VNC RFB protocol and the `VncInputEvent` proto messages.
 */

// ---------------------------------------------------------------------------
// Coordinate scaling
// ---------------------------------------------------------------------------

/**
 * Scale pointer coordinates from the rendered video element space to the
 * VNC framebuffer space.
 *
 * The video element may be a different size than the framebuffer (e.g. the
 * browser window is smaller than the VNC desktop), so pointer positions must be
 * scaled proportionally.
 *
 * Non-integer results are rounded to the nearest integer.
 */
export function scaleCoordinates(
  videoX: number,
  videoY: number,
  videoWidth: number,
  videoHeight: number,
  framebufferWidth: number,
  framebufferHeight: number,
): { x: number; y: number } {
  const x = Math.round((videoX / videoWidth) * framebufferWidth);
  const y = Math.round((videoY / videoHeight) * framebufferHeight);
  return { x, y };
}

// ---------------------------------------------------------------------------
// Keysym mapping
// ---------------------------------------------------------------------------

/**
 * X11 keysym values for special keys that don't map directly from the
 * character code.
 */
const SPECIAL_KEY_KEYSYMS: Record<string, number> = {
  Enter: 0xff0d,      // XK_Return
  Escape: 0xff1b,     // XK_Escape
  Backspace: 0xff08,  // XK_BackSpace
  Delete: 0xffff,     // XK_Delete
  Tab: 0xff09,        // XK_Tab
  ArrowLeft: 0xff51,  // XK_Left
  ArrowUp: 0xff52,    // XK_Up
  ArrowRight: 0xff53, // XK_Right
  ArrowDown: 0xff54,  // XK_Down
  Home: 0xff50,       // XK_Home
  End: 0xff57,        // XK_End
  PageUp: 0xff55,     // XK_Page_Up
  PageDown: 0xff56,   // XK_Page_Down
  Insert: 0xff63,     // XK_Insert
  F1: 0xffbe,         // XK_F1
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
  Shift: 0xffe1,      // XK_Shift_L
  Control: 0xffe3,    // XK_Control_L
  Alt: 0xffe9,        // XK_Alt_L
  Meta: 0xffeb,       // XK_Super_L
  CapsLock: 0xffe5,   // XK_Caps_Lock
};

/**
 * Map a browser `KeyboardEvent` to an X11 keysym value suitable for
 * `VncKeyEvent.keysym`.
 *
 * Returns `null` if the event does not have a known keysym mapping.
 */
export function keyboardEventToKeysym(e: KeyboardEvent): number | null {
  const { key } = e;

  // Check the special-key table first.
  if (key in SPECIAL_KEY_KEYSYMS) {
    return SPECIAL_KEY_KEYSYMS[key];
  }

  // Single printable character — use its Unicode code point directly.
  // X11 keysyms for Unicode characters in the Latin-1 range (0x20–0xff) map
  // directly to the code point. For characters above 0xff, X11 uses 0x01000000
  // | codepoint, but those are uncommon for basic VNC usage.
  if (key.length === 1) {
    const codePoint = key.codePointAt(0);
    if (codePoint !== undefined) {
      return codePoint;
    }
  }

  // Unrecognised key (e.g. media keys, F13+, browser-specific keys).
  return null;
}

// ---------------------------------------------------------------------------
// RFB button mask helpers
// ---------------------------------------------------------------------------

/**
 * Convert a browser `MouseEvent.button` index (0=left, 1=middle, 2=right) to an
 * RFB button mask bit.
 *
 * RFB button mask: bit 0 = left, bit 1 = middle, bit 2 = right, bits 3–7 = extra.
 */
export function mouseButtonToRfbMask(button: number): number {
  // button index maps directly to bit position in the RFB mask.
  return 1 << button;
}

/**
 * Convert a browser `WheelEvent.deltaY` value to an RFB scroll button mask.
 *
 * RFB uses button 4 (bit 4, value 16) for scroll up and button 5 (bit 5, value 32)
 * for scroll down.
 *
 * Returns 0 if deltaY is zero.
 */
export function wheelDeltaToRfbMask(deltaY: number): number {
  if (deltaY < 0) {
    return 16;  // button 4 = scroll up = bit 4 (1 << 4)
  }
  if (deltaY > 0) {
    return 32;  // button 5 = scroll down = bit 5 (1 << 5)
  }
  return 0;
}
