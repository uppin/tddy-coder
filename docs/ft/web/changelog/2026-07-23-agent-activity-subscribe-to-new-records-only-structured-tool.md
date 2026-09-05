# 2026-07-23 — Agent activity: subscribe to new records only + structured tool input/output

- The agent-activity pane can subscribe **live-only** — skipping the full-history replay — via `useSessionActivity`'s `mode` (`StreamMode.LIVE_ONLY`); the overlay keeps the snapshot-then-live default so history still populates on open ([agent-activity-pane.md](../agent-activity-pane.md#streaming-design--streamsessionactivity)).
- Tool **input/output** are now structured `google.protobuf.Value` end-to-end (not opaque JSON strings), so the detail dialog renders a real object/array/string/scalar — a bare-string tool result shows verbatim ([agent-activity-pane.md](../agent-activity-pane.md#data-model--a-new-per-session-agent-activity-log)).
