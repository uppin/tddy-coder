# Changesets Applied

Wrapped changeset history for tddy-livekit.

- **2026-03-14** [Feature] LiveKit Token Generation — TokenGenerator in token.rs (generate, time_until_refresh). connect_with_bridge accepts Arc<RpcBridge<S>> for service reuse. run_with_reconnect: token refresh loop (TTL minus 60s), reconnects before expiry. livekit-api 0.4 dependency. (tddy-livekit)
- **2026-03-13** [Architecture Change] Dual-Transport Service Codegen — Slimmed to thin LiveKit transport adapter. Proto envelope (rpc_envelope.proto), participant, RpcRequest→RpcMessage→RpcBridge. Depends on tddy-rpc only; no service impls. (tddy-livekit)
- **2026-03-13** [Feature] Ghostty Terminal via LiveKit — terminal.proto with TerminalService/StreamTerminalIO. TerminalServiceImpl: broadcast output, mpsc input sink; RpcService impl for handle_rpc_stream. terminal_service_acceptance test. (tddy-livekit)
- **2026-03-13** [Feature] LiveKit ConnectRPC TypeScript Transport — EchoClientStream, EchoBidiStream in proto and EchoServiceImpl. RpcBridge handle_rpc_stream, handle_decoded_requests. LiveKitParticipant stream accumulation by request_id. RpcClient call_client_stream, call_bidi_stream; end_of_stream fix for unary. sender_identity in RpcRequest for targeted response routing. examples/echo_server.rs. rpc_scenarios: client/bidi tests, ThreeParticipantHarness. (tddy-livekit)
