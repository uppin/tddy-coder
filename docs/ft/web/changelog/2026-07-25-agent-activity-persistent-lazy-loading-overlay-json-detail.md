# 2026-07-25 — Agent Activity: persistent, lazy-loading overlay + JSON detail

- The Agent Activity overlay now **persists per session** — switching to another session and back keeps its transcript and unread badge instead of resetting and re-downloading. See [agent-activity-pane.md § Persisted, lazily-counted activity](../agent-activity-pane.md#persisted-lazily-counted-activity-added-2026-07-25).
- It also **loads lazily**: a cheap count-first stream drives the icon and unread badge (counting the entries you actually see — agent text plus one per tool call), and the full transcript is fetched only the first time you open the pane.
- **Clicking a tool call** opens a detail dialog showing its input and output as prettified, color-highlighted JSON.
