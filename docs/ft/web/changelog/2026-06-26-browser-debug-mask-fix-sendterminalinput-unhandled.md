# 2026-06-26 — Browser DEBUG mask + fix SendTerminalInput unhandled rejections

- `dev.daemon.yaml` ships `debug: "tddy:term:*"` — a [`debug`](https://www.npmjs.com/package/debug)-package namespace mask served at `GET /api/config`; browser adopts it on load with `localStorage` persistence (invalidated only when the config value changes); `?debug=` URL param overrides for a session
- `GhosttyTerminal` / `GhosttyTerminalGrpc` replace ad-hoc `console.log` spam with namespaced loggers: `tddy:term:{write,data,resize,grpc,life,mouse}`
- Fixed `GrpcSessionTerminal.send()`: unhandled `[failed_precondition]` promise rejections silenced via `.catch(() => {})`; `controlToken` prop added and forwarded in every `SendTerminalInput` call (internal ref pattern — stream is not recreated on token changes)
- `useTerminalControl` exposes `controlTokenRef`; token threads `SessionsDrawerScreen` → `SessionMainPane` → `GrpcSessionTerminal`
