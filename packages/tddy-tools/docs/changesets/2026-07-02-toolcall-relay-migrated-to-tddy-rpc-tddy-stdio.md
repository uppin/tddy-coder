# 2026-07-02 — Toolcall relay migrated to `tddy-rpc`/`tddy-stdio`

**Type:** Feature

new `toolcall_client::dispatch_toolcall` replaces the four hand-rolled `UnixStream` write-line/read-line relay functions in `cli.rs` (`relay_submit`/`relay_ask`/`relay_list_actions`/`relay_invoke_action`) and the two in `build_cli.rs` (`relay_build_list`/`relay_build`) — connects, wraps via `StdioEndpoint::from_duplex`, calls the RPC method matching the wire request's `"type"` field. Wire request/response JSON shapes unchanged; their `run_*` callers became `async fn`. Fixed a real regression along the way: `cli_integration.rs`'s fake relay server (`submit_relay_error_with_message_surfaces_detail`) still spoke the old raw-line protocol and hung forever against the new framed client — rewritten to host a fixed-response `RpcService`. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-tools, tddy-core)
