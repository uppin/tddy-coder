# 2026-06-25 — **Session inspector Tools tab

**Type:** Feature

invoke panel + durable call log** — `InspectorTabs` (Details/Tools tab strip, `role="tab"`, `aria-selected`, `data-testid`); `SessionInspectorDrawer` restructured (tab state, Details scrolls existing metadata/controls, Tools renders `SessionToolsTab`); `SessionToolsTab`: invoke panel (`<select>` from `listExecTools`, `<textarea>` seeded by `defaultArgsFromSchema`, Invoke button → `executeTool`, result/error blocks) + call log (collapsible rows newest-first; expand shows Input/Output/stdio panels; Shell rows parse `stdout`/`stderr`/`exit_code` from `result_json`; empty-state message); `toolSchema.ts` (`defaultArgsFromSchema` walks JSON Schema `properties`/`required` → skeleton JSON); Cypress `SessionToolsTab.cy.tsx` (12 component tests) + `SessionInspectorAcceptance.cy.tsx` extended (5 tab-switching tests). Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md). (tddy-web)
