# 2026-06-26 — **PR-stack session UI

**Type:** Feature

recipe dropdown, parent picker, drawer grouping** — `CreateSessionPane.tsx`: recipe `<select>` with all 9 recipes (default "tdd"), parent-picker `<select>` (tool sessions only, populated via `listSessions`, filters non-orchestrator sessions), `stackParent` passed to `startSession`; `stackParents.ts`: `stackParentCandidates(sessions)` helper; `sessionStackGroups.ts`: `groupSessionsByStack(sessions) → { groups, flat }` (children with missing parent fall into flat); `SessionDrawer.tsx`: groups rendered as collapsible `<details>/<summary>` wrapping orchestrator + children; `SessionDrawerItem.tsx`: `depth?: number` prop with `data-depth` attribute. TS proto regenerated with `orchestratorSessionId`/`stackParent` fields. Tests: 13 bun unit, 7 SessionDrawer CT, 9 CreateSessionPane CT (recipe+picker). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)
