import { describe, expect, test } from "bun:test";
import { presenceIdentityForUser } from "../lib/presenceIdentity";

function mockSessionStorage(): Storage {
  const store = new Map<string, string>();
  return {
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
  } as Storage;
}

describe("presence identity for LiveKit common room", () => {
  test("repeated calls in the same tab reuse the same identity (sessionStorage)", () => {
    const prev = globalThis.sessionStorage;
    globalThis.sessionStorage = mockSessionStorage();
    try {
      const a = presenceIdentityForUser("testuser");
      const b = presenceIdentityForUser("testuser");
      expect(a).toBe(b);
      expect(a.startsWith("web-testuser-")).toBe(true);
    } finally {
      globalThis.sessionStorage = prev;
    }
  });

  test("different GitHub logins get different identities in the same tab", () => {
    const prev = globalThis.sessionStorage;
    globalThis.sessionStorage = mockSessionStorage();
    try {
      const alice = presenceIdentityForUser("alice");
      const bob = presenceIdentityForUser("bob");
      expect(alice).not.toBe(bob);
    } finally {
      globalThis.sessionStorage = prev;
    }
  });

  test("identity includes a time-derived segment for debugging", () => {
    const prev = globalThis.sessionStorage;
    globalThis.sessionStorage = mockSessionStorage();
    try {
      const identity = presenceIdentityForUser("myuser");
      expect(identity).toMatch(/\d{10,}/);
    } finally {
      globalThis.sessionStorage = prev;
    }
  });
});
