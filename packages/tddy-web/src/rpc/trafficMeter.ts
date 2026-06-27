/**
 * TrafficMeter — accumulates inbound and outbound byte counts with a
 * sliding-window rate estimate (bytes/second over the last 2 seconds).
 *
 * TrafficMeterRegistry — manages one meter per named scope (e.g. "http",
 * "livekit-room-<id>"), returning the same instance for repeated lookups
 * and a fresh instance after an explicit delete.
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip)
 */

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

interface TimestampedSample {
  at: number; // Date.now() ms
  bytes: number;
}

const WINDOW_MS = 2000; // sliding window for rate calculation

// ---------------------------------------------------------------------------
// TrafficMeter
// ---------------------------------------------------------------------------

export class TrafficMeter {
  bytesIn = 0;
  bytesOut = 0;
  inRate = 0;
  outRate = 0;

  private samplesIn: TimestampedSample[] = [];
  private samplesOut: TimestampedSample[] = [];
  private subscribers = new Set<() => void>();

  record(dir: "in" | "out", n: number): void {
    const now = Date.now();
    if (dir === "in") {
      this.bytesIn += n;
      this.samplesIn.push({ at: now, bytes: n });
      this.inRate = this._computeRate(this.samplesIn, now);
    } else {
      this.bytesOut += n;
      this.samplesOut.push({ at: now, bytes: n });
      this.outRate = this._computeRate(this.samplesOut, now);
    }
    this._notify();
  }

  subscribe(cb: () => void): () => void {
    this.subscribers.add(cb);
    return () => {
      this.subscribers.delete(cb);
    };
  }

  snapshot(): { bytesIn: number; bytesOut: number; inRate: number; outRate: number } {
    return {
      bytesIn: this.bytesIn,
      bytesOut: this.bytesOut,
      inRate: this.inRate,
      outRate: this.outRate,
    };
  }

  reset(): void {
    this.bytesIn = 0;
    this.bytesOut = 0;
    this.inRate = 0;
    this.outRate = 0;
    this.samplesIn = [];
    this.samplesOut = [];
    this._notify();
  }

  // -------------------------------------------------------------------------
  // Private helpers
  // -------------------------------------------------------------------------

  private _computeRate(samples: TimestampedSample[], now: number): number {
    // Evict samples older than the window
    const cutoff = now - WINDOW_MS;
    let i = 0;
    while (i < samples.length && samples[i].at < cutoff) i++;
    if (i > 0) samples.splice(0, i);

    // Sum bytes within the window and divide by window seconds
    const total = samples.reduce((sum, s) => sum + s.bytes, 0);
    return total / (WINDOW_MS / 1000);
  }

  private _notify(): void {
    for (const cb of this.subscribers) {
      cb();
    }
  }
}

// ---------------------------------------------------------------------------
// TrafficMeterRegistry
// ---------------------------------------------------------------------------

export class TrafficMeterRegistry {
  private meters = new Map<string, TrafficMeter>();

  get(scope: string): TrafficMeter {
    let meter = this.meters.get(scope);
    if (!meter) {
      meter = new TrafficMeter();
      this.meters.set(scope, meter);
    }
    return meter;
  }

  delete(scope: string): void {
    this.meters.delete(scope);
  }
}
