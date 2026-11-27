import type { ReceivedVideoStats } from "tddy-client-web";

/** What one stats report said about the received video: enough to measure the next interval. */
export interface InboundVideoSample {
  /** `inbound-rtp` `timestamp`, in milliseconds. */
  readonly timestampMs: number;
  readonly bytesReceived: number;
  readonly width: number;
  readonly height: number;
  readonly fps: number;
}

/** The fields of an `inbound-rtp` video entry this reads; every one may be absent. */
interface InboundRtpEntry {
  readonly type?: string;
  readonly kind?: string;
  readonly timestamp?: number;
  readonly bytesReceived?: number;
  readonly frameWidth?: number;
  readonly frameHeight?: number;
  readonly framesPerSecond?: number;
}

/** The received-video sample in `report`, or `null` when it holds no `inbound-rtp` video entry. */
export function inboundVideoSampleOf(report: Iterable<unknown>): InboundVideoSample | null {
  for (const value of report) {
    const entry = value as InboundRtpEntry;
    if (entry.type !== "inbound-rtp" || entry.kind !== "video") continue;
    return {
      timestampMs: entry.timestamp ?? 0,
      bytesReceived: entry.bytesReceived ?? 0,
      width: entry.frameWidth ?? 0,
      height: entry.frameHeight ?? 0,
      fps: entry.framesPerSecond ?? 0,
    };
  }
  return null;
}

/**
 * The stats for the interval from `previous` to `current`: the bitrate is the bytes received across
 * it, so a first sample, or one that did not move forward in time, measures nothing.
 */
export function receivedVideoStatsBetween(
  previous: InboundVideoSample | null,
  current: InboundVideoSample,
): ReceivedVideoStats | null {
  if (!previous) return null;
  const elapsedMs = current.timestampMs - previous.timestampMs;
  if (elapsedMs <= 0) return null;
  const bytes = Math.max(0, current.bytesReceived - previous.bytesReceived);
  return {
    kbps: Math.round((bytes * 8) / elapsedMs),
    width: current.width,
    height: current.height,
    fps: Math.round(current.fps),
  };
}
