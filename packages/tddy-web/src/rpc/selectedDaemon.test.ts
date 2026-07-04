import { describe, expect, it } from "bun:test";
import type { DaemonHost } from "../lib/participantRole";
import {
  readStoredSelectedDaemon,
  writeStoredSelectedDaemon,
  resolveSelectedDaemonInstanceId,
} from "./selectedDaemon";

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
const LAPTOP_B: DaemonHost = { instanceId: "laptop-b", label: "laptop-b (this daemon)" };

describe("resolveSelectedDaemonInstanceId", () => {
  it("defaults to the serving daemon when nothing is stored", () => {
    // Given / When
    const selected = resolveSelectedDaemonInstanceId({
      daemons: [UDOO, LAPTOP_B],
      servingInstanceId: "udoo",
      storedInstanceId: null,
    });

    // Then
    expect(selected).toBe("udoo");
  });

  it("prefers the stored selection over the serving daemon when it is still present", () => {
    // Given / When
    const selected = resolveSelectedDaemonInstanceId({
      daemons: [UDOO, LAPTOP_B],
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
