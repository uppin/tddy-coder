# 2026-07-02 — Toolcall listener served over `tddy-rpc`/`tddy-stdio`

**Type:** Feature

new `ToolcallRpcService` (`toolcall/listener.rs`) replaces the bespoke newline-delimited-JSON `accept_loop`/`handle_connection`; each accepted `TDDY_SOCKET` connection is hosted via `StdioEndpoint::from_duplex`, dispatching `Submit`/`Ask`/`Approve`/`ListActions`/`InvokeAction`/`Build`/`BuildList` by RPC method name. Wire payloads (the `*Wire` structs, `ToolCallResponse` JSON shapes) are byte-for-byte unchanged — only the framing/dispatch moved; `tddy-rpc`/`tddy-stdio` moved from dev-dependencies to `[dependencies]`. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core, tddy-tools)
