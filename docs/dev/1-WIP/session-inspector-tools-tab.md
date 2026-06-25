# Changeset: Session Inspector "Tools" Tab

**Feature:** `session-inspector-tools-tab`  
**PRD:** [`docs/ft/web/session-drawer.md`](../../../ft/web/session-drawer.md) (Tools Tab section)  
**Branch:** `actual-tools-tab`

## Summary

Adds a **Tools** tab to the session inspector drawer in the `#/sessions` screen. The tab
provides (1) a durable per-session tool-call log showing input/output/stdio for every
`ExecuteTool` invocation, and (2) an inline invoke panel for running exec-tools against
the session's worktree.

## TODO

- [ ] Create/update PRD documentation
- [ ] Create changeset
- [x] ~~Create/update PRD documentation~~ (`docs/ft/web/session-drawer.md` updated)
- [x] ~~Create changeset~~ (this file)
- [ ] `tool_call_log.rs` — new durable JSONL persistence module + unit tests (red → green)
- [ ] Wire `append_tool_call` into `ConnectionService::execute_tool` handler
- [ ] Add `ListSessionToolCalls` RPC to `connection.proto` + Rust handler + integration tests
- [ ] Run `bunx buf generate` → regenerate `packages/tddy-web/src/gen/connection_pb.ts`
- [ ] Frontend: `InspectorTabs.tsx` + drawer restructure (Details tab, regression)
- [ ] Frontend: `toolSchema.ts` + `toolSchema.test.ts` (`defaultArgsFromSchema` helper)
- [ ] Frontend: `SessionToolsTab.tsx` (invoke panel + call log)
- [ ] Frontend: thread `client`/`sessionToken` through `SessionsDrawerScreen` → `SessionMainPane` → `SessionInspectorDrawer`
- [ ] Cypress component test: `SessionToolsTab.cy.tsx`
- [ ] Cypress acceptance: extend `SessionInspectorAcceptance.cy.tsx` with tab-switching specs

## Acceptance Criteria

| # | Criterion |
|---|-----------|
| AC1 | Inspector drawer shows "Details" and "Tools" tabs; "Details" is selected by default |
| AC2 | Clicking "Tools" tab reveals the invoke panel and call log; clicking "Details" restores the metadata panel |
| AC3 | On the Tools tab, the invoke picker lists tools from `ListExecTools` (name + description) |
| AC4 | Selecting a tool seeds the JSON args textarea with a skeleton built from its `input_schema_json` |
| AC5 | Clicking Invoke calls `ExecuteTool({sessionId, toolName, argsJson})` and renders `result_json` in a code block |
| AC6 | When `is_error: true`, the invoke result shows the error box instead of the success block |
| AC7 | After a successful invoke, the call log row for that call appears (refetch fires automatically) |
| AC8 | Call log renders one collapsible row per recorded call, showing tool name, status pill, and relative time |
| AC9 | Expanding a call row reveals Input (`args_json`), Output (`result_json`), and stdio panels |
| AC10 | For Shell tool call rows, the stdio panel shows `stdout`, `stderr`, and `exit_code` parsed from `result_json` |
| AC11 | The call log persists across daemon restarts (read from JSONL, not from the in-memory registry) |
| AC12 | The call log is scoped to the session — calls from a different session are not shown |
| AC13 | When no calls have been made, the log shows an empty-state message |

## Validation Results

### Build
- `cargo build -p tddy-daemon` ✅ clean
- `cargo clippy -p tddy-daemon -- -D warnings` ✅ clean
- `bun run build` (TypeScript) ✅ clean

### Issues Found

#### [WARNING] `packages/tddy-web/src/components/sessions/SessionToolsTab.tsx:212`
React key instability for sync tool call rows. The fallback key uses `String(displayIndex)` — the position in the **reversed** array — which shifts by 1 for all existing items whenever a new entry is prepended. For sync tools (Read, Write, StrReplace etc.), `task_id` is always `""` (confirmed in `tool_engine.rs`), so the fallback fires for every non-Shell call.

**Effect**: React incorrectly recycles DOM nodes across renders when new items are added; any expanded row state resets for shifted items.

**Fix**: Use `String(originalIndex)` instead — `originalIndex` is the position in the original (append-only) `callLog` array and is stable. Or use `String(call.createdUnixMs)` for full uniqueness.

#### [INFO] `packages/tddy-web/src/components/sessions/SessionToolsTab.tsx:24`
`sessionToken` is declared in `SessionToolsTabProps` but is never destructured or used in the component body. The token is already baked into the callback closures by the parent (`SessionInspectorDrawer`). Dead prop.

#### [INFO] `packages/tddy-web/src/components/sessions/SessionInspectorDrawer.tsx:250-279`
Inline arrow function callbacks (`onListExecTools`, `onListSessionToolCalls`) get new references on every parent render. Both are in `useEffect` dep arrays in `SessionToolsTab`, so the catalog + call log are re-fetched whenever the inspector re-renders (e.g., on expand/restore state changes). 2 extra RPC calls per inspector state change — acceptable but worth noting.

### Risk Assessment
- **Critical**: 0
- **Warning**: 1 (React key instability — UX impact only, no data loss)
- **Info**: 2

## Packages Affected

- `packages/tddy-service` — proto changes (`connection.proto`)
- `packages/tddy-daemon` — new `tool_call_log.rs` module; `connection_service.rs` handler changes
- `packages/tddy-web` — new and modified components, regenerated client, new Cypress tests
