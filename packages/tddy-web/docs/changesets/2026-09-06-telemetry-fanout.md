# 2026-09-06 — Per-host telemetry on the Hosts screen

**Type:** Feature

`useHostStats` resolves its client per host instead of only through the daemon selector, so the Hosts
screen can show a live per-core CPU and free-disk reading on every row while the Host Stats Footer's
zero-argument call keeps behaving exactly as before. Three call shapes now: omitted follows the
selector, an id reads that host, `null` subscribes to nothing.

The stream loop moved out of the hook into `subscribeHostStats`, and the feed decision out of the
component into `telemetryFeedFor` — both pure enough to unit-test, which is the point. Neither
property they pin is observable through a component test: every host shares one in-memory transport,
and the request names no host.

Cancellation is the substantive change. Releasing the iterator cannot end a Connect call — the
library hands back an iterable whose iterator has `next` and nothing else — and a `for await` loop
parked on a frame that never arrives can never reach a `break`, so a host that subscribed and stayed
silent was never let go of. The subscription now cancels via an `AbortSignal`, as the other stream
hooks in this package do.

`HostRowTelemetry` renders four states and never a fabricated number, deciding the CPU and disk slots
independently so a reading carrying only disk does not draw an empty bar strip.

See [`../host-stats-streaming.md`](../host-stats-streaming.md).
