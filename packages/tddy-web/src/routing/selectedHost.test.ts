/**
 * `resolveSelectedDaemonInstanceId` precedence once the URL joins the chain:
 * URL → sessionStorage → serving daemon → first daemon → none.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md § Host in the URL.
 *
 * The resolution rule moves out of `rpc/selectedDaemon.tsx` into this pure module: importing it
 * from the `.tsx` provider drags React's JSX runtime into a pure unit test, which is why the
 * existing `src/rpc/selectedDaemon.test.ts` cannot run under `bun test` today (and is excluded from
 * every `bun test` path in `package.json`).
 */

import { describe, expect, it } from "bun:test";
import type { DaemonHost } from "../lib/participantRole";
import {
  readStoredSelectedDaemon,
  resolveSelectedDaemonInstanceId,
  writeStoredSelectedDaemon,
} from "./selectedHost";

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

const UDOO: DaemonHost = { instanceId: "udoo", label: "udoo (this daemon)" };
const LAPTOP_B: DaemonHost = { instanceId: "laptop-b", label: "laptop-b" };
const BOTH = [UDOO, LAPTOP_B];

// ---------------------------------------------------------------------------
// Precedence rules 2–5, unchanged by the URL joining the chain. Relocated from
// `src/rpc/selectedDaemon.test.ts`, which could not run: it imported a `.tsx` module.
// ---------------------------------------------------------------------------

describe("resolveSelectedDaemonInstanceId — without a URL host", () => {
  it("defaults to the serving daemon when nothing is stored", () => {
    // Given / When
    const selected = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: null,
    });

    // Then
    expect(selected).toBe("udoo");
  });

  it("prefers the stored selection over the serving daemon when it is still present", () => {
    // Given / When
    const selected = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: "laptop-b",
    });

    // Then
    expect(selected).toBe("laptop-b");
  });

  it("falls back to the serving daemon when the stored selection has left the room", () => {
    // Given — the previously selected peer is no longer in the common room
    const selected = resolveSelectedDaemonInstanceId({
      daemons: [UDOO],
      servingInstanceId: "udoo",
      storedInstanceId: "laptop-b",
    });

    // Then
    expect(selected).toBe("udoo");
  });

  it("falls back to the first available daemon when neither the stored nor serving daemon is present", () => {
    // Given — this web session's own serving daemon has itself dropped off the common room
    const selected = resolveSelectedDaemonInstanceId({
      daemons: [LAPTOP_B],
      servingInstanceId: "udoo",
      storedInstanceId: "some-other-host",
    });

    // Then
    expect(selected).toBe("laptop-b");
  });

  it("returns null when there are no daemons in the room yet", () => {
    // Given / When
    const selected = resolveSelectedDaemonInstanceId({
      daemons: [],
      servingInstanceId: "udoo",
      storedInstanceId: null,
    });

    // Then
    expect(selected).toBeNull();
  });
});

describe("selected-daemon session storage", () => {
  it("round-trips a selection through session storage", () => {
    withMockedSessionStorage(() => {
      // Given / When
      writeStoredSelectedDaemon("laptop-b");

      // Then
      expect(readStoredSelectedDaemon()).toBe("laptop-b");
    });
  });

  it("returns null when nothing has been stored yet", () => {
    withMockedSessionStorage(() => {
      // Given / When / Then — fresh tab (empty sessionStorage)
      expect(readStoredSelectedDaemon()).toBeNull();
    });
  });
});

describe("resolveSelectedDaemonInstanceId — URL precedence", () => {
  it("prefers the URL host over the stored one", () => {
    // When
    const result = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: "udoo",
      urlInstanceId: "laptop-b",
    });

    // Then
    expect(result).toBe("laptop-b");
  });

  it("prefers the URL host over the serving daemon when nothing is stored", () => {
    // When
    const result = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: null,
      urlInstanceId: "laptop-b",
    });

    // Then
    expect(result).toBe("laptop-b");
  });

  it("falls back to the stored host when the URL names a daemon that is not present", () => {
    // When
    const result = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: "laptop-b",
      urlInstanceId: "retired-host",
    });

    // Then
    expect(result).toBe("laptop-b");
  });

  it("falls back to the serving daemon when neither the URL nor the store names a present daemon", () => {
    // When
    const result = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: "retired-store-host",
      urlInstanceId: "retired-url-host",
    });

    // Then
    expect(result).toBe("udoo");
  });

  it("falls back to the first daemon when the serving daemon has left too", () => {
    // When
    const result = resolveSelectedDaemonInstanceId({
      daemons: [LAPTOP_B],
      servingInstanceId: "udoo",
      storedInstanceId: null,
      urlInstanceId: "retired-url-host",
    });

    // Then
    expect(result).toBe("laptop-b");
  });

  it("resolves to none when the room holds no daemons yet", () => {
    // When
    const result = resolveSelectedDaemonInstanceId({
      daemons: [],
      servingInstanceId: "udoo",
      storedInstanceId: "udoo",
      urlInstanceId: "udoo",
    });

    // Then
    expect(result).toBeNull();
  });

  it("keeps the existing precedence when the URL names no host", () => {
    // When
    const result = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: "laptop-b",
      urlInstanceId: null,
    });

    // Then
    expect(result).toBe("laptop-b");
  });
});
