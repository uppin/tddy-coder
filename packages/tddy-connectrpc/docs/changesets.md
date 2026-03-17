# Changesets Applied

Wrapped changeset history for tddy-connectrpc.

- **2026-03-14** [Feature] TokenService via Connect-RPC — New acceptance test `token_service_generate_token_returns_200_with_token_and_ttl`. TokenService exposed over Connect-RPC for web token fetch. (tddy-connectrpc)
- **2026-03-14** [Feature] ConnectRPC Transport Module — New crate. Connect protocol HTTP transport for tddy-rpc services. Axum router at `/rpc`, envelope framing (5-byte), protocol handling. Unary, server streaming, client streaming, bidi streaming. Protobuf-binary-first; RpcBridge dispatch. tddy_rpc::Code extended with to_connect_str() and to_http_status(). Mounted in tddy-coder web server. Companion tddy-rust-typescript-tests for integration tests. (tddy-connectrpc)
