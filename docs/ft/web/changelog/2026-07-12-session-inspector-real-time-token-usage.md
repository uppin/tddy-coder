# 2026-07-12 — Session Inspector: real-time token usage

- The Session Inspector has a new **Usage** tab showing live per-conversation token usage (main agent + each subagent: input/output/total tokens and turns) with a summing TOTAL row, updating as the session runs.
- Usage streams over the session's existing event stream (`TddyRemote.Stream`) — no new endpoint — and a running session now broadcasts updates as its agents produce turns.
- Known limitations: a View opened mid-session sees totals on the next update tick (no snapshot-on-connect yet), and end-to-end live rendering still depends on the per-session presenter stream's LiveKit room wiring. See [session-usage-inspector.md](../session-usage-inspector.md). PR [#295](https://github.com/uppin/tddy-coder/pull/295).
