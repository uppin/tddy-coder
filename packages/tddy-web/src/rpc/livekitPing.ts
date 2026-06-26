/**
 * Utilities for reading round-trip time from a LiveKit Room's underlying
 * WebRTC peer-connection stats, and a React hook that polls it on an interval.
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip)
 */

import { useState, useEffect } from "react";

// ---------------------------------------------------------------------------
// readRoomRtt
// ---------------------------------------------------------------------------

/**
 * Read the current round-trip time (in ms) from the LiveKit Room's WebRTC
 * peer connection stats.
 *
 * Accesses `room.engine.pcManager.subscriber.pc.getStats()` (falling back
 * to publisher) and finds the first `candidate-pair` entry with
 * `state === "succeeded"` and a non-null `currentRoundTripTime`.
 *
 * Returns `null` when the Room is not yet connected, getStats() throws, or
 * no valid candidate pair exists.
 */
export async function readRoomRtt(room: unknown): Promise<number | null> {
  try {
    const pcManager = (room as any)?.engine?.pcManager;
    if (!pcManager) return null;

    // Try subscriber PC first, then publisher
    const pcs = [pcManager.subscriber?.pc, pcManager.publisher?.pc].filter(Boolean);

    for (const pc of pcs) {
      try {
        const stats: Iterable<RTCStats> = await pc.getStats();
        for (const entry of stats) {
          if (entry.type !== "candidate-pair") continue;
          const pair = entry as RTCStats & { state?: string; currentRoundTripTime?: number };
          if (pair.state !== "succeeded") continue;
          if (pair.currentRoundTripTime == null) continue;
          return Math.round(pair.currentRoundTripTime * 1000);
        }
      } catch {
        // Try next PC
      }
    }

    return null;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// useLiveKitPing
// ---------------------------------------------------------------------------

const DEFAULT_INTERVAL_MS = 2000;

/**
 * React hook that polls `readRoomRtt` every `intervalMs` milliseconds and
 * returns the latest RTT in ms, or `null` when unavailable.
 */
export function useLiveKitPing(
  room: unknown | null | undefined,
  intervalMs: number = DEFAULT_INTERVAL_MS,
): number | null {
  const [pingMs, setPingMs] = useState<number | null>(null);

  useEffect(() => {
    if (!room) {
      setPingMs(null);
      return;
    }

    let cancelled = false;

    async function poll() {
      if (cancelled) return;
      const rtt = await readRoomRtt(room);
      if (!cancelled) {
        setPingMs(rtt);
      }
    }

    void poll();
    const id = setInterval(poll, intervalMs);

    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [room, intervalMs]);

  return pingMs;
}
