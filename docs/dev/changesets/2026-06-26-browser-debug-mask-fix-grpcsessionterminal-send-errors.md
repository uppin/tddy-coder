# 2026-06-26 — Browser DEBUG mask + fix GrpcSessionTerminal.send() errors

**Type:** Feature+Fix

`dev.daemon.yaml`: `debug: "tddy:term:*"`; `tddy-daemon` `DaemonConfig.debug: Option<String>` → `run_server` → `ClientConfig`; `tddy-coder` `ClientConfig.debug` (omitted when `None`); `tddy-web`: `debugMask.ts` + `debug.d.ts` ambient types; namespaced `tddy:term:*` loggers in `GhosttyTerminal`/`GhosttyTerminalGrpc`; `?debug=` URL param + `localStorage` persistence (config-invalidation); fix `GrpcSessionTerminal.send()`: `.catch(()=>{})` + `controlToken` prop forwarded via ref (no stream recreation); `useTerminalControl` exposes `controlTokenRef`; 6 bun unit + 2 Cypress CT tests. Feature [local-web-dev.md](../ft/web/local-web-dev.md#browser-debug-mask); PR [#233](https://github.com/uppin/tddy-coder/pull/233). (tddy-daemon, tddy-coder, tddy-web)
