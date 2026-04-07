# Changesets Applied

Wrapped changeset history for tddy-rpc.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-03-14** [Feature] MultiRpcService — MultiRpcService and ServiceEntry for multiplexing multiple RPC services on a single participant. Enables TokenService + TerminalService on same LiveKit participant. (tddy-rpc)
- **2026-03-13** [Architecture Change] Dual-Transport Service Codegen — New package. Generic RPC framework: Status, Code, Request, Response, Streaming, RpcMessage, RpcService trait, RpcBridge, RpcResult, ResponseBody. Optional tonic feature for From conversions. (tddy-rpc)
