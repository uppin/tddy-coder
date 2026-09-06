# 2026-07-26 — tddy-web — Agent Activity tool-detail dialog

**Category:** Future enhancement
**Source:** acp-tool-detail-explicit-states changeset, 2026-07-26

- **No retry affordance on a failed body lookup** — `AgentActivityDetailDialog` reports a failure inline
  and nothing is cached, so a retry *is* possible but only by closing and reopening the row. A "Retry"
  button in the error block would make that discoverable.
- **The tool-detail cache has no per-session entry cap** — `AgentActivityRegistry`'s
  `MAX_SESSIONS = 100` LRU is the only bound, so a session in which the operator opens very many tool
  rows retains every fetched body for the page's lifetime. Bound it per session if a heavy transcript
  ever shows growth (related: the persisted-log size caps tracked below).
- **The `animate-pulse` skeleton is inline in the dialog** rather than a shared UI primitive — it is the
  only skeleton in tddy-web today. Promote it to `components/ui/` when a second surface needs one.
