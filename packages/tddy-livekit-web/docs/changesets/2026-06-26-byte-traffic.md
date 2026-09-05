# 2026-06-26 — **byte-traffic

**Type:** Feature

optional traffic meter on the LiveKit transport** — `LiveKitTransportOptions` gains an optional `meter?: { record(dir: "in" | "out", bytes: number): void }`; `publishRequest` records `out` (`payload.length`) and the `DataReceived` listener records `in`, so the web dashboard's per-session `TrafficMeter` can count exact wire-payload bytes. Additive and backward-compatible (no meter ⇒ no-op). Tests: `transport.test.ts`. Feature [session-drawer.md § Session Traffic Strip](../../../../docs/ft/web/session-drawer.md#session-traffic-strip). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-livekit-web)
