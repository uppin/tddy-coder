import { afterEach, describe, expect, it, mock } from "bun:test";
import { isKeyboardLockSupported, lockKeyboard } from "./keyboardLock";

/** A browser `navigator.keyboard` whose `lock` records what it was asked to lock. */
function aKeyboardRecordingLocks() {
  return {
    lock: mock((_keys?: string[]) => Promise.resolve()),
    unlock: mock(() => undefined),
  };
}

function givenTheBrowserHasKeyboard(keyboard: ReturnType<typeof aKeyboardRecordingLocks>): void {
  Object.defineProperty(globalThis.navigator, "keyboard", {
    configurable: true,
    value: keyboard,
  });
}

function givenTheBrowserHasNoKeyboardLock(): void {
  Object.defineProperty(globalThis.navigator, "keyboard", {
    configurable: true,
    value: undefined,
  });
}

afterEach(() => {
  Reflect.deleteProperty(globalThis.navigator, "keyboard");
});

describe("keyboardLock", () => {
  it("reports keyboard lock as unsupported where the browser has none", () => {
    // Given a browser without the Keyboard Lock API
    givenTheBrowserHasNoKeyboardLock();

    // When asked whether the keyboard can be locked
    const supported = isKeyboardLockSupported();

    // Then it says no
    expect(supported).toBe(false);
  });

  it("locks every key", async () => {
    // Given a browser with the Keyboard Lock API
    const keyboard = aKeyboardRecordingLocks();
    givenTheBrowserHasKeyboard(keyboard);

    // When the keyboard is locked
    await lockKeyboard();

    // Then the lock was asked for once, with no key list — which is how "every key" is spelled
    expect(keyboard.lock).toHaveBeenCalledTimes(1);
    expect(keyboard.lock.mock.calls[0]).toEqual([]);
  });
});
