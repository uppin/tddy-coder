/**
 * `useAttachedSessionTraffic` — aggregate per-session terminal (data-plane) traffic across ALL
 * mounted runtimes, not just the focused one. It sums each runtime's byte-tap counters (`bytesIn` /
 * `bytesOut`, folded by `makeByteTap` per output chunk / input yield) and derives a live aggregate
 * rate from their advances. Backgrounded runtimes keep streaming per the `SessionRuntimeRegistry`
 * design, so a session left in the background still contributes to the readout.
 *
 * Fed into `StatusBar` alongside the HTTP (control-plane) meter so the screen-level traffic strip
 * reflects both planes.
 *
 * Changeset: `statusbar-session-traffic`
 * PRD: `docs/ft/web/host-stats-footer.md`
 */

import { useEffect, useRef } from "react";
import { TrafficMeter } from "../../rpc/trafficMeter";
import { useMeterSnapshot } from "./useMeterSnapshot";
import type { SessionRuntimeRegistry, SessionRuntimeState } from "./sessionRuntimeRegistry";

export interface AttachedSessionTraffic {
  bytesIn: number;
  bytesOut: number;
  inRate: number;
  outRate: number;
}

export function useAttachedSessionTraffic(
  runtimes: ReadonlyArray<SessionRuntimeState>,
  runtimeRegistry: SessionRuntimeRegistry | null,
): AttachedSessionTraffic {
  // Cumulative totals: the sum of every mounted runtime's byte-tap counters (focused +
  // backgrounded). Read straight from the reactive snapshot so the displayed totals match render.
  let bytesIn = 0;
  let bytesOut = 0;
  for (const r of runtimes) {
    bytesIn += r.bytesIn;
    bytesOut += r.bytesOut;
  }

  // One shared rate meter fed each session's byte-tap advances. A single meter suffices: recording
  // every runtime's positive delta into it yields the aggregate rate across all runtimes for free.
  const meterRef = useRef<TrafficMeter | null>(null);
  meterRef.current ??= new TrafficMeter();
  const meter = meterRef.current;

  // Fold per-session cumulative advances into the rate meter as the registry notifies. The byte tap
  // only ever increases a runtime's counters, so a positive diff since the last observation is the
  // traffic that arrived in between. A runtime first seen here is baselined (its current total is
  // recorded WITHOUT feeding the rate) so a session that already had traffic when this hook mounted
  // does not spike the rate; removed (disconnected) runtimes are dropped so a later re-add starts
  // fresh.
  const seenRef = useRef<Map<string, { bytesIn: number; bytesOut: number }>>(new Map());
  useEffect(() => {
    if (!runtimeRegistry) return;
    const fold = () => {
      const seen = seenRef.current;
      const live = new Set<string>();
      for (const r of runtimeRegistry.runtimes) {
        live.add(r.sessionId);
        const last = seen.get(r.sessionId);
        if (last) {
          const dIn = r.bytesIn - last.bytesIn;
          const dOut = r.bytesOut - last.bytesOut;
          if (dIn > 0) meter.record("in", dIn);
          if (dOut > 0) meter.record("out", dOut);
        }
        seen.set(r.sessionId, { bytesIn: r.bytesIn, bytesOut: r.bytesOut });
      }
      for (const id of [...seen.keys()]) if (!live.has(id)) seen.delete(id);
    };
    fold();
    return runtimeRegistry.subscribe(fold);
  }, [runtimeRegistry, meter]);

  // The rate comes from the shared meter (refreshes on each record and on an interval so idle rates
  // decay); the totals come straight from the runtime snapshot above.
  const rate = useMeterSnapshot(meter);

  return { bytesIn, bytesOut, inRate: rate.inRate, outRate: rate.outRate };
}
