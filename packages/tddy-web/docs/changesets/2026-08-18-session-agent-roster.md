# 2026-08-18 — **session agent roster

**Type:** Feature

fanned-out agent picker and Agent roster pane** — `useSessionAgentRoster.ts` subscribes to `StreamSessionAgents`; `SessionAgentRosterPane.tsx` renders four distinct states with add/detach (confirmation when a detach deletes a checkout on another host); `useAvailableAgents.ts` fans `ListSubagents` out across common-room daemons with per-daemon error isolation and returns qualified ids. `CreateSessionPane.tsx` consumes the fan-out and sends qualified ids; `InspectorTabs.tsx`/`SessionInspectorDrawer.tsx`/`appRoutes.ts` add the Agents tab. Feature [session-agent-roster.md](../../../../docs/ft/daemon/session-agent-roster.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)
