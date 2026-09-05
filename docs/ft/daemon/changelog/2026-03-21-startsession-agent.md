# 2026-03-21 — StartSession: `agent`

- **ConnectionService**: `StartSessionRequest` includes optional `agent`; forwarded to spawned `tddy-coder` as `--agent` when non-empty (skips interactive backend menu in the child).
