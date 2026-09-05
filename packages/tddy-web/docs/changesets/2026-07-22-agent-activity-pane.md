# 2026-07-22 — **Agent Activity pane

**Type:** Feature

live view of the agent's own tool calls** — new `useSessionActivity` hook (opens `StreamSessionActivity`, coalesces by `call_id`, exposes `records`/`hasActivity`/`unreadCount`/`markSeen()`) and `AgentActivityOverlay` (icon button hidden until `hasActivity`, unread badge, in-pane overlay list with `[running]`/`[error]` markers, full input/output detail dialog with Escape/backdrop close), wired into the `SessionMainPane` top bar beside the Inspector toggle; session-type-agnostic (incl. sandbox). New `agent-activity-*` testids + `agentActivityPage` page object; Cypress `AgentActivityAcceptance` (8). Feature [agent-activity-pane.md](../../../../docs/ft/web/agent-activity-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)
