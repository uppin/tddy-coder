# 2026-03-13 — LiveKit ConnectRPC TypeScript Transport

**Type:** Feature

EchoClientStream, EchoBidiStream in proto and EchoServiceImpl. RpcBridge handle_rpc_stream, handle_decoded_requests. LiveKitParticipant stream accumulation by request_id. RpcClient call_client_stream, call_bidi_stream; end_of_stream fix for unary. sender_identity in RpcRequest for targeted response routing. examples/echo_server.rs. rpc_scenarios: client/bidi tests, ThreeParticipantHarness. (tddy-livekit)
