import { describe, it, expect, beforeEach } from "bun:test";
import { getScreenId } from "./screenId";

// Bun's test environment is Node-like (no browser globals). Provide a minimal
// in-memory sessionStorage so the implementation's persistence path is exercised.
const store: Record<string, string> = {};
const mockSessionStorage = {
  getItem: (key: string) => store[key] ?? null,
  setItem: (key: string, value: string) => {
    store[key] = value;
  },
  removeItem: (key: string) => {
    delete store[key];
  },
  clear: () => {
    for (const k of Object.keys(store)) delete store[k];
  },
};
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(globalThis as any).sessionStorage = mockSessionStorage;

describe("getScreenId", () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  it("returns a non-empty string", () => {
    // Given / When
    const id = getScreenId();

    // Then
    expect(id).toBeTruthy();
    expect(id.length).toBeGreaterThan(0);
  });

  it("returns the same id on repeated calls within the same tab", () => {
    // Given
    const first = getScreenId();

    // When
    const second = getScreenId();

    // Then
    expect(second).toBe(first);
  });

  it("returns ids starting with 'screen-'", () => {
    // Given / When
    const id = getScreenId();

    // Then
    expect(id.startsWith("screen-")).toBe(true);
  });

  it("persists the id in sessionStorage under a stable key", () => {
    // Given
    const id = getScreenId();

    // When — simulate a 'remount' by calling again
    sessionStorage.setItem("tddy.screenId", id);
    const second = getScreenId();

    // Then — same id is returned, no new one generated
    expect(second).toBe(id);
  });
});
