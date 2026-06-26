import React, { useState, useEffect } from "react";
import type { SessionAttachmentState } from "./useSessionAttachment";
import { SessionTrafficStrip } from "./SessionTrafficStrip";
import { useSessionLiveKitRoom } from "./useSessionLiveKitRoom";
import { useLiveKitPing } from "../../rpc/livekitPing";
import { useTrafficMeterRegistry } from "../../rpc/transportProvider";
import type { TrafficMeter } from "../../rpc/trafficMeter";

type MeterSnap = { bytesIn: number; bytesOut: number; inRate: number; outRate: number };
const ZERO_SNAP: MeterSnap = { bytesIn: 0, bytesOut: 0, inRate: 0, outRate: 0 };

function useMeterSnapshot(meter: TrafficMeter | null): MeterSnap {
  const [snap, setSnap] = useState<MeterSnap>(() => (meter ? meter.snapshot() : ZERO_SNAP));
  useEffect(() => {
    if (!meter) {
      setSnap(ZERO_SNAP);
      return;
    }
    setSnap(meter.snapshot());
    return meter.subscribe(() => setSnap(meter.snapshot()));
  }, [meter]);
  return snap;
}

interface SessionTrafficBarProps {
  attachment: SessionAttachmentState;
}

export function SessionTrafficBar({ attachment }: SessionTrafficBarProps) {
  const livekitRoomName =
    attachment.status === "connected-livekit" ? attachment.livekitRoom : null;
  const { room } = useSessionLiveKitRoom(attachment);
  const pingMs = useLiveKitPing(room);
  const meterRegistry = useTrafficMeterRegistry();
  const httpSnap = useMeterSnapshot(meterRegistry?.get("http") ?? null);
  const livekitSnap = useMeterSnapshot(
    livekitRoomName && meterRegistry ? meterRegistry.get(livekitRoomName) : null,
  );

  return (
    <SessionTrafficStrip
      bytesIn={httpSnap.bytesIn + livekitSnap.bytesIn}
      bytesOut={httpSnap.bytesOut + livekitSnap.bytesOut}
      inRate={httpSnap.inRate + livekitSnap.inRate}
      outRate={httpSnap.outRate + livekitSnap.outRate}
      pingMs={pingMs}
    />
  );
}
