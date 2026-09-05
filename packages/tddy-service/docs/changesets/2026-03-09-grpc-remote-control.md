# 2026-03-09 — gRPC Remote Control

**Type:** Feature

New package. TddyRemote service with Stream RPC (bidirectional). Proto: ClientMessage (UserIntent variants), ServerMessage (PresenterEvent variants). TddyRemoteService subscribes to broadcast, streams events to clients; spawns task to forward client intents to mpsc. convert.rs: client_message_to_intent, event_to_server_message. build.rs + tonic-build for codegen. Integration test: SubmitFeatureInput → GoalStarted + ModeChanged. (tddy-grpc)
