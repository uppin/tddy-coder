# 2026-03-13 — LiveKit ConnectRPC Transport for Browser

- **tddy-livekit-web**: New TypeScript package implementing ConnectRPC Transport over LiveKit data channels. Enables browser-based ConnectRPC clients to call unary and streaming RPCs served by Rust LiveKitParticipant.
- **LiveKitTransport**: Implements Transport interface with unary() and stream(); supports unary, client streaming, server streaming, bidirectional streaming.
- **AsyncQueue**: Backpressure-aware async channel for streaming responses.
- **Rust extensions**: tddy-livekit gains EchoClientStream, EchoBidiStream; sender_identity in RpcRequest for targeted response routing; RpcBridge handle_rpc_stream; stream accumulation in LiveKitParticipant.
- **Test infra**: Cypress component tests against real Rust echo server; examples/echo_server.rs; startEchoServer/stopEchoServer/generateToken Cypress tasks.
- **Packages**: tddy-livekit-web (new), tddy-livekit (proto, bridge, client, participant, echo_service, rpc_scenarios).
