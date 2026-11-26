import { useEffect, useState } from "react";
import type { ReceivedVideoStats, RoomHandle } from "tddy-client-web";
import { useRemoteVideoTrack } from "./RemoteVideo";
import { inboundVideoSampleOf, receivedVideoStatsBetween, type InboundVideoSample } from "./receivedVideoStats";

/** How often the receiver's stats are read. */
const SAMPLE_EVERY_MS = 1000;

/** How the video track `trackName` of `identity` in `room` is arriving, from its receiver's stats. */
export function useReceivedVideoStats(
  room: RoomHandle | null,
  identity: string,
  trackName: string,
): ReceivedVideoStats | null {
  const track = useRemoteVideoTrack(room, identity, trackName);
  const [stats, setStats] = useState<ReceivedVideoStats | null>(null);

  useEffect(() => {
    if (!track || !("getRTCStatsReport" in track)) return;
    let previous: InboundVideoSample | null = null;
    let stopped = false;
    const sample = async () => {
      const report = await track.getRTCStatsReport();
      if (stopped || !report) return;
      const current = inboundVideoSampleOf(report.values());
      if (!current) return;
      const measured = receivedVideoStatsBetween(previous, current);
      previous = current;
      if (measured) setStats(measured);
    };
    const timer = setInterval(() => {
      void sample().catch(() => {
        // A report that could not be read leaves the last reading standing; the next tick retries.
      });
    }, SAMPLE_EVERY_MS);
    return () => {
      stopped = true;
      clearInterval(timer);
      setStats(null);
    };
  }, [track]);

  return stats;
}
