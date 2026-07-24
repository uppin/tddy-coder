/**
 * `useMeterSnapshot` — subscribe to a {@link TrafficMeter} and expose its live snapshot as React
 * state. The readout refreshes on every recorded byte AND on a fixed interval, so idle rates decay
 * to zero even when no new traffic arrives. Passing `null` yields a stable zero snapshot.
 *
 * Shared by the screen-level traffic readouts (`StatusBar`'s HTTP meter and
 * `useAttachedSessionTraffic`'s aggregate session meter).
 */

import { useEffect, useState } from "react";
import type { TrafficMeter } from "../../rpc/trafficMeter";

export type MeterSnap = { bytesIn: number; bytesOut: number; inRate: number; outRate: number };

export const ZERO_SNAP: MeterSnap = { bytesIn: 0, bytesOut: 0, inRate: 0, outRate: 0 };

/** Refresh the traffic readout at least this often so idle rates decay even without new traffic. */
export const TRAFFIC_REFRESH_MS = 5000;

export function useMeterSnapshot(meter: TrafficMeter | null): MeterSnap {
  const [snap, setSnap] = useState<MeterSnap>(() => (meter ? meter.snapshot() : ZERO_SNAP));
  useEffect(() => {
    if (!meter) {
      setSnap(ZERO_SNAP);
      return;
    }
    setSnap(meter.snapshot());
    const unsubscribe = meter.subscribe(() => setSnap(meter.snapshot()));
    const interval = setInterval(() => setSnap(meter.snapshot()), TRAFFIC_REFRESH_MS);
    return () => {
      unsubscribe();
      clearInterval(interval);
    };
  }, [meter]);
  return snap;
}
