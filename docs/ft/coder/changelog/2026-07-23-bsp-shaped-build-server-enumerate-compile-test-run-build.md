# 2026-07-23 — BSP-shaped build server: enumerate + compile/test/run build targets over RPC

- A session now exposes its `BUILD.yaml` build targets over a BSP (Build Server Protocol)–shaped RPC service (`bsp.BspService`): enumerate workspace targets with their capabilities/tags/languages, list a target's sources and output paths, reload after a `BUILD.yaml` change, and compile/test/run a target ([bsp-build-server.md](../bsp-build-server.md)).
- `BUILD.yaml` targets can now declare `capabilities` (compile/test/run/debug), `tags`, and `languages` — derived from the target type when omitted — and compile/test/run are gated by the resolved capabilities.
- Served per-session on the coder participant, and session-addressed on the daemon (resolving each request's session to its worktree and catalog), over the same protobuf/Connect + LiveKit transports the web UI already uses.
- Not a literal JSON-RPC 2.0 BSP transport — external BSP-client (Metals/IntelliJ) compatibility is an explicit non-goal.
