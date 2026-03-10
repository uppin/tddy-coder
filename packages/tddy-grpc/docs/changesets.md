# Changesets Applied

Wrapped changeset history for tddy-grpc.

- **2026-03-10** [Feature] Plan Approval Gate — Proto: ApprovePlan, ViewPlan, RefinePlan, DismissViewer intents; AppModePlanReview, AppModeMarkdownViewer. convert.rs: client_message_to_intent and intent_to_client_message for new variants. (tddy-grpc)
- **2026-03-09** [Feature] gRPC Remote Control — New package. TddyRemote service with Stream RPC (bidirectional). Proto: ClientMessage (UserIntent variants), ServerMessage (PresenterEvent variants). TddyRemoteService subscribes to broadcast, streams events to clients; spawns task to forward client intents to mpsc. convert.rs: client_message_to_intent, event_to_server_message. build.rs + tonic-build for codegen. Integration test: SubmitFeatureInput → GoalStarted + ModeChanged. (tddy-grpc)
