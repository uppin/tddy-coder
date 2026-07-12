/**
 * Unit tests for `SessionRuntimeRegistry` — the per-session runtime store that keeps one
 * mounted terminal per attached session and survives focus switches (explicit-disconnect eviction).
 *
 * Changeset: `2026-07-12-fast-session-change`
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change` (req 2, 3)
 *
 * ⚠️ RED PHASE — fails until `./sessionRuntimeRegistry` exists with the API below.
 */

import { describe, it, expect } from "bun:test";
import {
  SessionRuntimeRegistry,
  type SessionRuntimeState,
} from "./sessionRuntimeRegistry";

function aRuntimeState(sessionId: string): SessionRuntimeState {
  return {
    sessionId,
    attached: true,
    bytesIn: 0,
    bytesOut: 0,
    lastDataReceivedAt: null,
  };
}

describe("SessionRuntimeRegistry", () => {
  it("keeps a backgrounded session's runtime after a focus switch and only evicts on explicit disconnect", () => {
    // Given — two attached sessions with A focused
    const registry = new SessionRuntimeRegistry();
    registry.add("session-a", aRuntimeState("session-a"));
    registry.add("session-b", aRuntimeState("session-b"));
    registry.focus("session-a");
    expect(registry.focusedSessionId).toBe("session-a");

    // When — the user switches focus to B
    registry.focus("session-b");

    // Then — A is still mounted (not evicted) and B is focused
    expect(registry.focusedSessionId).toBe("session-b");
    expect(registry.get("session-a")?.attached).toBe(true);
    expect(registry.get("session-b")?.attached).toBe(true);
    expect(registry.runtimes.map((r) => r.sessionId).sort()).toEqual(["session-a", "session-b"]);

    // When — the user explicitly disconnects A
    registry.disconnect("session-a");

    // Then — only A is evicted; B remains
    expect(registry.get("session-a")).toBeUndefined();
    expect(registry.get("session-b")?.attached).toBe(true);
  });

  it("notifies subscribers when a runtime is added, focused, or disconnected", () => {
    // Given
    const registry = new SessionRuntimeRegistry();
    const events: string[] = [];
    registry.subscribe(() => events.push("notify"));

    // When
    registry.add("session-a", aRuntimeState("session-a"));
    registry.focus("session-a");
    registry.disconnect("session-a");

    // Then — one notification per mutation
    expect(events).toEqual(["notify", "notify", "notify"]);
    expect(registry.runtimes).toHaveLength(0);
  });

  it("updates byte counters and lastDataReceivedAt on a background runtime without refocusing it", () => {
    // Given — A is focused, B is backgrounded
    const registry = new SessionRuntimeRegistry();
    registry.add("session-a", aRuntimeState("session-a"));
    registry.add("session-b", aRuntimeState("session-b"));
    registry.focus("session-a");

    // When — bytes arrive for the backgrounded B
    registry.recordBytes("session-b", { bytesIn: 128, bytesOut: 32, at: 1_700_000_000_000 });

    // Then — B's counters update while focus stays on A
    expect(registry.focusedSessionId).toBe("session-a");
    expect(registry.get("session-b")?.bytesIn).toBe(128);
    expect(registry.get("session-b")?.bytesOut).toBe(32);
    expect(registry.get("session-b")?.lastDataReceivedAt).toBe(1_700_000_000_000);
  });
});
