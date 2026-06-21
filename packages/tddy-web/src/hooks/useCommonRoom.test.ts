import { describe, expect, it } from "bun:test";
import { presenceIdentityForUser } from "../lib/presenceIdentity";

/** Provides an isolated in-memory sessionStorage for the duration of fn. */
function withMockedSessionStorage(fn: () => void): void {
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
    fn();
  } finally {
    globalThis.sessionStorage = prev;
  }
}

describe("presence identity for LiveKit common room", () => {
  it("repeated calls in the same tab reuse the same identity via sessionStorage", () => {
    withMockedSessionStorage(() => {
      // Given — fresh tab (empty sessionStorage)

      // When
      const a = presenceIdentityForUser("testuser");
      const b = presenceIdentityForUser("testuser");

      // Then — both calls return the same stable identity for this tab
      expect(a).toBe(b);
      expect(a.startsWith("web-testuser-")).toBe(true);
    });
  });

  it("different GitHub logins get different identities in the same tab", () => {
    withMockedSessionStorage(() => {
      // Given — fresh tab

      // When
      const alice = presenceIdentityForUser("alice");
      const bob = presenceIdentityForUser("bob");

      // Then
      expect(alice).not.toBe(bob);
    });
  });

  it("identity includes a time-derived segment for debugging", () => {
    withMockedSessionStorage(() => {
      // Given — fresh tab

      // When
      const identity = presenceIdentityForUser("myuser");

      // Then — time-derived segment is generated at runtime; can only validate it contains a number sequence
      expect(identity).toMatch(/\d{10,}/);
    });
  });
});
