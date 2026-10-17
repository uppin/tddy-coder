/**
 * An isolated in-memory `sessionStorage` for `bun test` suites, which run outside a browser.
 *
 * For bun:test only — never import from Cypress tests.
 */

/**
 * Provides an isolated in-memory sessionStorage for the duration of `fn`, restoring the previous one
 * afterwards. Synchronous only: an async `fn` would outlive the swap, so a thenable result throws.
 */
export function withMockedSessionStorage(fn: () => void): void {
  const store = new Map<string, string>();
  const mock: Storage = {
    get length() {
      return store.size;
    },
    clear() {
      store.clear();
    },
    getItem(k: string) {
      return store.get(k) ?? null;
    },
    key(i: number) {
      return [...store.keys()][i] ?? null;
    },
    removeItem(k: string) {
      store.delete(k);
    },
    setItem(k: string, v: string) {
      store.set(k, v);
    },
  };
  const prev = globalThis.sessionStorage;
  globalThis.sessionStorage = mock;
  try {
    const result: unknown = fn();
    if (typeof (result as { then?: unknown } | null | undefined)?.then === "function") {
      throw new Error(
        "withMockedSessionStorage: fn returned a thenable; the mock is synchronous-only and would be restored before it settles",
      );
    }
  } finally {
    globalThis.sessionStorage = prev;
  }
}
