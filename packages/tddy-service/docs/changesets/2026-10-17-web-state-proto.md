# 2026-10-17 — web_state.proto

**Type:** Feature

Adds `proto/web_state.proto` with `RememberedDaemonLabels { map<string, string> labels }`. It holds
browser-local dashboard state that `tddy-web` keeps in `sessionStorage` as protobuf JSON. No service
uses it, and it is deliberately absent from `build.rs`. It is generated for TypeScript only, into
`tddy-web/src/gen` and `tddy-rust-typescript-tests/gen`. Cross-package entry:
[2026-10-17-host-selection-survives-reload.md](../../../../docs/dev/changesets/2026-10-17-host-selection-survives-reload.md).
