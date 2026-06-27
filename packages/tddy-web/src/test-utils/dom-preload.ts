/**
 * Minimal DOM API polyfills for bun:test.
 *
 * Bun's test runner runs in a Node-like environment without browser globals.
 * This preload registers the subset of DOM APIs used in bun unit tests so that
 * tests which construct browser events (e.g. `new KeyboardEvent(...)`) work
 * without a full DOM implementation.
 *
 * Listed in `bunfig.toml` under `[test] preload`.
 */

// ---------------------------------------------------------------------------
// KeyboardEvent
// ---------------------------------------------------------------------------

if (typeof globalThis.KeyboardEvent === "undefined") {
  class KeyboardEvent {
    readonly type: string;
    readonly key: string;
    readonly code: string;
    readonly shiftKey: boolean;
    readonly ctrlKey: boolean;
    readonly altKey: boolean;
    readonly metaKey: boolean;

    constructor(type: string, init?: KeyboardEventInit) {
      this.type = type;
      this.key = init?.key ?? "";
      this.code = init?.code ?? "";
      this.shiftKey = init?.shiftKey ?? false;
      this.ctrlKey = init?.ctrlKey ?? false;
      this.altKey = init?.altKey ?? false;
      this.metaKey = init?.metaKey ?? false;
    }
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (globalThis as any).KeyboardEvent = KeyboardEvent;
}
