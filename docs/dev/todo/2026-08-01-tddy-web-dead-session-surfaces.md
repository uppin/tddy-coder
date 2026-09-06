# 2026-08-01 — tddy-web — dead session surfaces

**Category:** Future enhancement
**Source:** inactive-session-activities changeset, 2026-08-01

- **`SessionDetailPane.tsx` has no importers.** It carries its own Resume/Delete buttons
  (`sessions-detail-resume-*`, `sessions-detail-delete-*`) and a full metadata block, all reachable
  from nothing — `SessionMainPane` superseded it. The ids still live in `cypress/support/testIds.ts`
  (`sessionsDetailResumeBtn`, `sessionsDetailDeleteBtn`), so a spec could be written against a
  component the app never mounts. Delete the component and its ids together, or wire it back in;
  leaving it is what let a second Resume affordance drift out of sync with the real one.
- **`useSessionActivity.ts` is callerless.** It consumes `StreamSessionActivity`
  (`AgentActivityRecord` frames, coalesced by `call_id`) and nothing calls it — both the Agent
  Activity overlay and the new inactive-session Activities view read the ACP replay path
  (`useAcpReplay` over `StreamAcpReplay`) instead. The daemon still serves the RPC. Decide whether
  `StreamSessionActivity` has a remaining consumer before treating it as load-bearing; note it also
  hardcodes `daemonInstanceId: ""`, so it would not peer-forward for a cross-host session as written.
