# 2026-07-25 — Lazy ACP tool bodies

- `StreamAcpReplay` streams **body-less** tool calls — `raw_input`/`raw_output` are stripped from the `SNAPSHOT_THEN_LIVE`/`LIVE_ONLY` frames (title/status/kind/id kept), so opening the Agent Activity pane no longer downloads every tool's full I/O ([acp-replay-lazy-tool-bodies.md](../acp-replay-lazy-tool-bodies.md)).
- A new unary **`GetAcpToolCallDetail`** returns one tool call's full `raw_input`/`raw_output` on demand (same coalesced transcript view; `NOT_FOUND` for an unknown id), served by both `StreamAcpReplay` hosts (daemon + coder participant) to back the detail dialog. `COUNT_THEN_LIVE` and `StreamSessionActivity` unchanged; web adoption of the lookup is a follow-up.
