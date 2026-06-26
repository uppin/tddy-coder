/**
 * Unit tests for TrafficMeter and TrafficMeterRegistry.
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip)
 */

import { describe, it, expect, beforeEach } from "bun:test";
import { TrafficMeter, TrafficMeterRegistry } from "./trafficMeter";

// ---------------------------------------------------------------------------
// TrafficMeter
// ---------------------------------------------------------------------------

describe("TrafficMeter", () => {
  let meter: TrafficMeter;

  beforeEach(() => {
    meter = new TrafficMeter();
  });

  it("starts with zero bytes in and out", () => {
    const snap = meter.snapshot();
    expect(snap.bytesIn).toBe(0);
    expect(snap.bytesOut).toBe(0);
  });

  it("accumulates bytes in on record('in', n)", () => {
    meter.record("in", 100);
    meter.record("in", 50);
    expect(meter.snapshot().bytesIn).toBe(150);
  });

  it("accumulates bytes out on record('out', n)", () => {
    meter.record("out", 200);
    meter.record("out", 300);
    expect(meter.snapshot().bytesOut).toBe(500);
  });

  it("accumulates in and out independently", () => {
    meter.record("in", 100);
    meter.record("out", 200);
    const snap = meter.snapshot();
    expect(snap.bytesIn).toBe(100);
    expect(snap.bytesOut).toBe(200);
  });

  it("notifies subscribers synchronously on each record call", () => {
    const calls: number[] = [];
    meter.subscribe(() => calls.push(meter.snapshot().bytesIn));
    meter.record("in", 10);
    meter.record("in", 20);
    expect(calls).toEqual([10, 30]);
  });

  it("unsubscribing stops future notifications", () => {
    const calls: number[] = [];
    const unsub = meter.subscribe(() => calls.push(1));
    meter.record("in", 10);
    unsub();
    meter.record("in", 20);
    expect(calls).toHaveLength(1);
  });

  it("multiple subscribers are all notified", () => {
    const a: number[] = [];
    const b: number[] = [];
    meter.subscribe(() => a.push(1));
    meter.subscribe(() => b.push(1));
    meter.record("in", 1);
    expect(a).toHaveLength(1);
    expect(b).toHaveLength(1);
  });

  it("reset() zeros all byte counts", () => {
    meter.record("in", 500);
    meter.record("out", 300);
    meter.reset();
    const snap = meter.snapshot();
    expect(snap.bytesIn).toBe(0);
    expect(snap.bytesOut).toBe(0);
  });

  it("reset() notifies subscribers", () => {
    const calls: number[] = [];
    meter.subscribe(() => calls.push(meter.snapshot().bytesIn));
    meter.record("in", 100);
    meter.reset();
    expect(calls).toEqual([100, 0]);
  });

  it("inRate and outRate start at 0", () => {
    const snap = meter.snapshot();
    expect(snap.inRate).toBe(0);
    expect(snap.outRate).toBe(0);
  });

  it("inRate equals total bytes / window seconds when all samples are fresh", () => {
    // Window is 2 s; recording N bytes right now → rate = N / 2 B/s.
    meter.record("in", 1000);
    expect(meter.snapshot().inRate).toBe(500); // 1000 bytes / 2 s
  });

  it("outRate equals total bytes / window seconds when all samples are fresh", () => {
    meter.record("out", 2000);
    expect(meter.snapshot().outRate).toBe(1000); // 2000 bytes / 2 s
  });

  it("snapshot() returns a plain object (not the meter itself)", () => {
    meter.record("in", 100);
    const snap = meter.snapshot();
    expect(snap).toMatchObject({ bytesIn: 100, bytesOut: 0 });
    expect(typeof snap.inRate).toBe("number");
    expect(typeof snap.outRate).toBe("number");
  });

  it("snapshot() is immutable — mutating it does not affect the meter", () => {
    meter.record("in", 100);
    const snap = meter.snapshot();
    (snap as Record<string, unknown>)["bytesIn"] = 9999;
    expect(meter.snapshot().bytesIn).toBe(100);
  });
});

// ---------------------------------------------------------------------------
// TrafficMeterRegistry
// ---------------------------------------------------------------------------

describe("TrafficMeterRegistry", () => {
  let registry: TrafficMeterRegistry;

  beforeEach(() => {
    registry = new TrafficMeterRegistry();
  });

  it("returns a TrafficMeter for a given scope key", () => {
    const meter = registry.get("http");
    expect(meter).toBeInstanceOf(TrafficMeter);
  });

  it("returns the same meter instance for the same scope key", () => {
    expect(registry.get("http")).toBe(registry.get("http"));
  });

  it("returns distinct meter instances for different scope keys", () => {
    expect(registry.get("http")).not.toBe(registry.get("livekit-room-1"));
  });

  it("meters are independent — recording in one does not affect another", () => {
    registry.get("http").record("in", 100);
    expect(registry.get("livekit-room-1").snapshot().bytesIn).toBe(0);
  });

  it("delete() removes the meter for a scope; get() after delete returns a fresh one", () => {
    const m1 = registry.get("room-x");
    m1.record("in", 999);
    registry.delete("room-x");
    const m2 = registry.get("room-x");
    expect(m2).not.toBe(m1);
    expect(m2.snapshot().bytesIn).toBe(0);
  });
});
