import { describe, expect, it } from "bun:test";
import { watchDevicePixelRatio } from "./terminalDprWatch";

/**
 * A matchMedia double whose queries are keyed by their text, so a test can fire
 * the change listener of the exact query the watcher armed — and verify the
 * watcher re-armed a *different* query after the DPR moved.
 */
class FakeMediaQuery {
  listeners: (() => void)[] = [];
  constructor(public matches: boolean) {}
  addEventListener(_type: string, listener: () => void) {
    this.listeners.push(listener);
  }
  removeEventListener(_type: string, listener: () => void) {
    this.listeners = this.listeners.filter((l) => l !== listener);
  }
  change(matches: boolean) {
    this.matches = matches;
    for (const listener of [...this.listeners]) listener();
  }
}

function fakeWindow(initialDpr: number) {
  const queries = new Map<string, FakeMediaQuery>();
  const win = {
    devicePixelRatio: initialDpr,
    matchMedia(query: string) {
      let q = queries.get(query);
      if (!q) {
        q = new FakeMediaQuery(true);
        queries.set(query, q);
      }
      return q;
    },
  };
  return { win, queries };
}

describe("watchDevicePixelRatio", () => {
  it("arms a media query at the current devicePixelRatio", () => {
    // When
    const { win, queries } = fakeWindow(1);
    watchDevicePixelRatio(win as unknown as Window, () => {});
    // Then
    expect([...queries.keys()]).toEqual(["(resolution: 1dppx)"]);
  });

  it("reports the new DPR when the armed query fires, and re-arms at that value", () => {
    // Given
    const { win, queries } = fakeWindow(1);
    const seen: number[] = [];
    watchDevicePixelRatio(win as unknown as Window, (dpr) => seen.push(dpr));
    // When the display scale changes to 2, the watcher's query stops matching
    const first = queries.get("(resolution: 1dppx)")!;
    win.devicePixelRatio = 2;
    first.change(false);
    // Then the change is reported and a fresh query is armed at the new DPR
    expect(seen).toEqual([2]);
    expect(queries.has("(resolution: 2dppx)")).toBe(true);
  });

  it("keeps watching across several DPR changes", () => {
    // Given
    const { win, queries } = fakeWindow(1);
    const seen: number[] = [];
    watchDevicePixelRatio(win as unknown as Window, (dpr) => seen.push(dpr));
    // When
    for (const to of [2, 1, 3]) {
      const from = win.devicePixelRatio;
      win.devicePixelRatio = to;
      queries.get(`(resolution: ${from}dppx)`)!.change(false);
    }
    // Then
    expect(seen).toEqual([2, 1, 3]);
  });

  it("fires nothing after dispose", () => {
    // Given
    const { win, queries } = fakeWindow(1);
    const seen: number[] = [];
    const dispose = watchDevicePixelRatio(win as unknown as Window, (dpr) =>
      seen.push(dpr),
    );
    // When
    dispose();
    win.devicePixelRatio = 2;
    queries.get("(resolution: 1dppx)")!.change(false);
    // Then
    expect(seen).toEqual([]);
  });

  it("is a no-op where matchMedia is unavailable", () => {
    // When
    const win = { devicePixelRatio: 2 };
    const dispose = watchDevicePixelRatio(win as unknown as Window, () => {
      throw new Error("must not be called");
    });
    // Then
    expect(() => dispose()).not.toThrow();
  });
});
