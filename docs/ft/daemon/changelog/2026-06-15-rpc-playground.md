# 2026-06-15 — RPC Playground

- **Backend**: `grpc.reflection.v1.ServerReflection` service (`reflection_service.rs`, vendored `reflection.proto`, embedded `FileDescriptorSet` from build.rs); `MultiRpcService::service_names()` in `tddy-rpc`; `reflection_entry_from()` helper registered in daemon `main.rs` and all `tddy-coder` `MultiRpcService` sites; daemon spawns a dedicated `LiveKitParticipant` in the common room (identity `daemon-{id}`) so the playground reaches it via data channel.
- **Frontend** (`tddy-web`): `/rpc-playground` route (hash-based, no 404 on reload); participant picker filtered to `coder`-role only; `RpcPlaygroundScreen` + `RpcPlaygroundAppPage`; request editor (builder ↔ raw JSON, synced); streaming panel; `invoke.ts` auto-injects `sessionToken` when the request type has a `session_token` field.
- **Reflection codegen** (`tddy-livekit-web`): `reflection_pb.ts` generated via buf; `createLiveKitTransport` used for all reflection + invocation calls (avoids fetch streaming body limits).
- **Test infrastructure**: Cypress tests in `tddy-livekit-web` and `tddy-web` auto-start a Docker LiveKit container when `LIVEKIT_TESTKIT_WS_URL` is not set.
- Feature: [rpc-playground.md](../rpc-playground.md). Cross-package: [docs/dev/changesets/](../../../dev/changesets/).
