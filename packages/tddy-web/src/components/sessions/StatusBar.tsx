import React from "react";
import type { SessionAttachmentState } from "./useSessionAttachment";
import { SessionTrafficStrip } from "./SessionTrafficStrip";
import { useSessionLiveKitRoom } from "./useSessionLiveKitRoom";
import { useLiveKitPing } from "../../rpc/livekitPing";
import { useTrafficMeterRegistry } from "../../rpc/transportProvider";
import { useMeterSnapshot } from "./useMeterSnapshot";
import { useAttachedSessionTraffic } from "./useAttachedSessionTraffic";
import type { SessionRuntimeRegistry, SessionRuntimeState } from "./sessionRuntimeRegistry";

interface StatusBarProps {
  attachment: SessionAttachmentState;
  /** All mounted session runtimes (focused + backgrounded) — their per-session byte taps are
   *  aggregated into the data-plane half of the readout. Defaults to none. */
  runtimes?: ReadonlyArray<SessionRuntimeState>;
  /** The runtime registry — subscribed to so the aggregate rate tracks live byte-tap advances
   *  across every mounted runtime. Defaults to `null`. */
  runtimeRegistry?: SessionRuntimeRegistry | null;
}

export function StatusBar({ attachment, runtimes = [], runtimeRegistry = null }: StatusBarProps) {
  const { room } = useSessionLiveKitRoom(attachment);
  const pingMs = useLiveKitPing(room);
  const meterRegistry = useTrafficMeterRegistry();
  // Control plane: the HTTP RPC meter. Data plane: every mounted session's terminal byte traffic,
  // aggregated across focused AND backgrounded runtimes (not just the focused session's room).
  const httpSnap = useMeterSnapshot(meterRegistry?.get("http") ?? null);
  const sessionSnap = useAttachedSessionTraffic(runtimes, runtimeRegistry);

  return (
    <SessionTrafficStrip
      bytesIn={httpSnap.bytesIn + sessionSnap.bytesIn}
      bytesOut={httpSnap.bytesOut + sessionSnap.bytesOut}
      inRate={httpSnap.inRate + sessionSnap.inRate}
      outRate={httpSnap.outRate + sessionSnap.outRate}
      pingMs={pingMs}
    />
  );
}
