# 2026-06-26 — Browser DEBUG mask + fix GrpcSessionTerminal.send() errors

**Type:** Feature+Fix

`debugMask.ts` (`resolveDebugMask`/`applyDebugMaskFromConfig`/`applyDebugMaskFromUrl`/`tddyDebug`); `debug.d.ts` ambient types; `index.tsx` applies mask on boot and on `/api/config` response; `GhosttyTerminal`/`GhosttyTerminalGrpc` namespaced loggers (`tddy:term:{write,data,resize,grpc,life,mouse}`); `GrpcSessionTerminal`: `controlToken?` prop, internal ref pattern (no stream recreation), `.catch(()=>{})` on `sendTerminalInput`; `useTerminalControl` exposes `controlTokenRef`; `SessionsDrawerScreen` → `SessionMainPane` thread the ref. 6 bun unit + 2 Cypress CT tests. PR [#233](https://github.com/uppin/tddy-coder/pull/233). (tddy-web)
