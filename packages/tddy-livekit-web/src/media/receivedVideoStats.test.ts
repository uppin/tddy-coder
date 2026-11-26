import { describe, expect, it } from "bun:test";
import { inboundVideoSampleOf, receivedVideoStatsBetween } from "./receivedVideoStats";

/** An `inbound-rtp` video entry `atMs` into the call, `bytes` received so far. */
function anInboundVideoEntry(atMs: number, bytes: number) {
  return {
    type: "inbound-rtp",
    kind: "video",
    timestamp: atMs,
    bytesReceived: bytes,
    frameWidth: 594,
    frameHeight: 1292,
    framesPerSecond: 29.6,
  };
}

describe("receivedVideoStatsBetween", () => {
  it("measures the bitrate from the bytes received across the interval", () => {
    // Given two samples one second apart, 250 000 bytes apart
    const previous = inboundVideoSampleOf([anInboundVideoEntry(1_000, 1_000_000)]);
    const current = inboundVideoSampleOf([anInboundVideoEntry(2_000, 1_250_000)]);

    // When
    const stats = receivedVideoStatsBetween(previous, current!);

    // Then
    expect(stats).toEqual({ kbps: 2000, width: 594, height: 1292, fps: 30 });
  });

  it("measures nothing from a first sample", () => {
    // Given
    const current = inboundVideoSampleOf([anInboundVideoEntry(1_000, 1_000_000)]);

    // When
    const stats = receivedVideoStatsBetween(null, current!);

    // Then
    expect(stats).toBeNull();
  });
});

describe("inboundVideoSampleOf", () => {
  it("ignores everything but received video", () => {
    // Given
    const report = [{ type: "inbound-rtp", kind: "audio", bytesReceived: 9 }, { type: "transport" }];

    // When
    const sample = inboundVideoSampleOf(report);

    // Then
    expect(sample).toBeNull();
  });
});
