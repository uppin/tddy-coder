# 2026-07-25 — Agent Activity: lazy tool-call bodies (fetch on click)

- Tool-call input/output is **no longer downloaded with the transcript** — the replay stream now carries only each call's name/status, so opening the pane on a busy session is cheap regardless of how much the tools read or printed.
- **Clicking a tool call fetches just that call's body on demand**, showing a brief loading state and, on failure, an error state; fetched bodies are cached so re-opening the same row is instant. See [agent-activity-pane.md § 4 Lazy tool bodies](../agent-activity-pane.md#4-lazy-tool-bodies--fetch-on-click-added-2026-07-25).
