/**
 * Unit tests for readRoomRtt and useLiveKitPing.
 *
 * `readRoomRtt` inspects the WebRTC peer-connection getStats() output to
 * return the current round-trip time in ms.  Since this accesses livekit-client
 * internals, it is isolated behind a thin wrapper that is tested with fake stats.
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip)
 */

import { describe, it, expect } from "bun:test";

import { readRoomRtt } from "./livekitPing";

// ---------------------------------------------------------------------------
// Helpers: fake RTCStatsReport entries
// ---------------------------------------------------------------------------

/**
 * Build a minimal fake Room whose engine.pcManager.subscriber.pc has
 * a getStats() that returns the given RTCStats entries.
 */
function makeFakeRoomWithStats(stats: RTCStats[]): unknown {
  const statsMap = new Map<string, RTCStats>(stats.map((s) => [s.id, s]));
  const fakePC = {
    getStats: async () => statsMap.values(),
  };
  return {
    engine: {
      pcManager: {
        subscriber: {
          pc: fakePC,
        },
        publisher: {
          pc: fakePC,
        },
      },
    },
  };
}

/**
 * A candidate-pair stats entry that signals a succeeded pair with a known RTT.
 */
function succeededCandidatePair(rttSeconds: number): RTCStats {
  return {
    id: "RTCIceCandidatePair_test",
    type: "candidate-pair",
    timestamp: Date.now(),
    // @ts-expect-error — RTCStats is narrowly typed; we add the fields we care about
    state: "succeeded",
    currentRoundTripTime: rttSeconds,
    nominated: true,
  };
}

/**
 * A candidate-pair stats entry that is not succeeded (should be ignored).
 */
function nonSucceededCandidatePair(): RTCStats {
  return {
    id: "RTCIceCandidatePair_failed",
    type: "candidate-pair",
    timestamp: Date.now(),
    // @ts-expect-error
    state: "failed",
    currentRoundTripTime: 0.001,
  };
}

// ---------------------------------------------------------------------------
// readRoomRtt
// ---------------------------------------------------------------------------

describe("readRoomRtt", () => {
  it("returns RTT in ms rounded from the succeeded candidate-pair", async () => {
    const room = makeFakeRoomWithStats([succeededCandidatePair(0.042)]);

    const rtt = await readRoomRtt(room as any);

    // 0.042 s × 1000 = 42 ms
    expect(rtt).toBe(42);
  });

  it("returns null when there is no succeeded candidate-pair entry", async () => {
    const room = makeFakeRoomWithStats([nonSucceededCandidatePair()]);

    const rtt = await readRoomRtt(room as any);

    expect(rtt).toBeNull();
  });

  it("returns null when there are no stats entries at all", async () => {
    const room = makeFakeRoomWithStats([]);

    const rtt = await readRoomRtt(room as any);

    expect(rtt).toBeNull();
  });

  it("returns null when the room has no engine/pcManager (not yet connected)", async () => {
    const rtt = await readRoomRtt({} as any);

    expect(rtt).toBeNull();
  });

  it("returns null when currentRoundTripTime is missing from the candidate-pair", async () => {
    const stats: RTCStats[] = [
      {
        id: "pair-no-rtt",
        type: "candidate-pair",
        timestamp: Date.now(),
        // @ts-expect-error
        state: "succeeded",
        // currentRoundTripTime deliberately absent
      },
    ];
    const room = makeFakeRoomWithStats(stats);

    const rtt = await readRoomRtt(room as any);

    expect(rtt).toBeNull();
  });

  it("ignores non-candidate-pair stat entries (e.g. inbound-rtp, codec)", async () => {
    const stats: RTCStats[] = [
      { id: "codec-1", type: "codec", timestamp: Date.now() },
      {
        id: "pair-1",
        type: "candidate-pair",
        timestamp: Date.now(),
        // @ts-expect-error
        state: "succeeded",
        currentRoundTripTime: 0.015,
      },
    ];
    const room = makeFakeRoomWithStats(stats);

    const rtt = await readRoomRtt(room as any);

    expect(rtt).toBe(15);
  });

  it("returns the first succeeded candidate-pair RTT when multiple exist", async () => {
    const stats: RTCStats[] = [
      {
        id: "pair-a",
        type: "candidate-pair",
        timestamp: Date.now(),
        // @ts-expect-error
        state: "succeeded",
        currentRoundTripTime: 0.010,
      },
      {
        id: "pair-b",
        type: "candidate-pair",
        timestamp: Date.now(),
        // @ts-expect-error
        state: "succeeded",
        currentRoundTripTime: 0.020,
      },
    ];
    const room = makeFakeRoomWithStats(stats);

    const rtt = await readRoomRtt(room as any);

    // First succeeded pair wins — Map preserves insertion order, so pair-a (10ms) is returned.
    expect(rtt).toBe(10);
  });

  it("handles getStats() rejection gracefully — returns null", async () => {
    const fakePC = { getStats: async () => { throw new Error("stats unavailable"); } };
    const room = {
      engine: { pcManager: { subscriber: { pc: fakePC }, publisher: { pc: fakePC } } },
    };

    const rtt = await readRoomRtt(room as any);

    expect(rtt).toBeNull();
  });
});
