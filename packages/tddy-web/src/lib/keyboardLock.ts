/**
 * The Keyboard Lock API (`navigator.keyboard.lock`), where the browser has one.
 *
 * Only honoured while the document is full screen: it lets keys the browser would otherwise keep
 * for itself (Escape, Alt+Tab, Cmd+W …) reach the page — here, the remote desktop.
 */

/** Whether this browser exposes `navigator.keyboard.lock`. */
export function isKeyboardLockSupported(): boolean {
  throw new Error("TODO(viewer-fullscreen): implement");
}

/** Lock every key (no key list), so the remote desktop receives all of them. */
export async function lockKeyboard(): Promise<void> {
  throw new Error("TODO(viewer-fullscreen): implement");
}

/** Release a lock taken by `lockKeyboard`. */
export function unlockKeyboard(): void {
  throw new Error("TODO(viewer-fullscreen): implement");
}
