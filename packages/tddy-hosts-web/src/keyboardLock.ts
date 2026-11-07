/**
 * The Keyboard Lock API (`navigator.keyboard.lock`), where the browser has one.
 *
 * Only honoured while the document is full screen: it lets keys the browser would otherwise keep
 * for itself (Escape, Alt+Tab, Cmd+W …) reach the page — here, the remote desktop.
 */

/** The part of `navigator.keyboard` this module uses; absent from older TypeScript DOM typings. */
interface KeyboardLockApi {
  lock(keyCodes?: string[]): Promise<void>;
  unlock(): void;
}

function keyboardLockApi(): KeyboardLockApi | undefined {
  const keyboard = (navigator as Navigator & { keyboard?: KeyboardLockApi }).keyboard;
  if (keyboard === undefined || keyboard === null) return undefined;
  if (typeof keyboard.lock !== "function" || typeof keyboard.unlock !== "function") return undefined;
  return keyboard;
}

/** Whether this browser exposes `navigator.keyboard.lock`. */
export function isKeyboardLockSupported(): boolean {
  return keyboardLockApi() !== undefined;
}

/** Lock every key (no key list), so the remote desktop receives all of them. */
export async function lockKeyboard(): Promise<void> {
  const keyboard = keyboardLockApi();
  if (!keyboard) {
    throw new Error("Keyboard Lock API is not supported in this browser");
  }
  await keyboard.lock();
}

/** Release a lock taken by `lockKeyboard`. */
export function unlockKeyboard(): void {
  keyboardLockApi()?.unlock();
}
