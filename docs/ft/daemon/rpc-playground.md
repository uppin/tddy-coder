# RPC Playground — dynamic discovery and invocation

**Status:** WIP (feature/host-rpcs)  
**Product area:** Daemon, Web, Coder sessions

## Summary

The **RPC Playground** is a new web UI screen (`/rpc-playground`) that lets developers discover the RPC services hosted by any connected LiveKit participant (daemon, coder session, Claude-CLI session) and invoke any method dynamically — without needing a compiled ConnectES client. Discovery uses the standard **gRPC ServerReflection** protocol (`grpc.reflection.v1.ServerReflection`, `ServerReflectionInfo` bidi stream). Descriptors are fetched lazily per service by symbol, with transitive dependency resolution. Request messages are composed via a **synced JSON ⇄ generated-form editor** and invoked over the existing **LiveKit binary data channel** (`tddy-rpc` topic). All four method kinds are supported: unary, server-streaming, client-streaming, and bidi-streaming.

## Screen overview

Layout at `/rpc-playground` (daemon-mode + existing `useAuth` guard required):

1. **Participant picker** — lists LiveKit room participants (from `useCommonRoom` + `useRoomParticipants`). Selecting a participant opens a reflection channel to it.
2. **Service/method tree** — calls `ServerReflectionInfo.list_services`, enumerates service names with method kinds (`unary`, `server_streaming`, `client_streaming`, `bidi_streaming`) colour-badged.
3. **Request editor** — two synced views toggled by a segmented control:
   - **Builder view**: auto-generated form fields per descriptor field kind (bool → checkbox, enum → select, scalar → text/number, message/list/map → nested JSON textarea). Single canonical JSON string is the source of truth — both views read from and write to it so toggling never transforms values.
   - **Raw view**: plain JSON textarea, seeded with the default JSON from `create(method.input, {})`.
4. **Invoke / stream controls** — unary and server-streaming: a single Invoke button. Client-streaming and bidi-streaming: an interactive streaming panel (Open / Send / Half-close / Cancel lifecycle, status badge, per-message request editor, incremental response log).
5. **Response view** — decoded response(s) as pretty-printed JSON; `RpcError` code + message on failure.

## Reflection protocol

Uses the standard **gRPC ServerReflection v1** (`grpc.reflection.v1.ServerReflection`) bidi service. Proto vendored at `packages/tddy-service/proto/grpc/reflection/v1/reflection.proto`. The service is compiled through `TddyServiceGenerator` so `ServerReflectionServer<T>` is a normal `ServiceEntry` registered as `"grpc.reflection.v1.ServerReflection"`.

Request flow (browser):
1. Open a `ServerReflectionInfo` bidi stream via `LiveKitTransport` to the selected participant identity.
2. Send `list_services: ""` → receive `list_services_response` with the participant's registered service names.
3. For each service the user expands: send `file_containing_symbol: "<service full name>"` → receive `file_descriptor_response` (serialized `FileDescriptorProto` bytes for that file plus transitive deps, returned one per response).
4. Accumulate `FileDescriptorProto`s into a `FileDescriptorSet` → `createFileRegistry(...)` (`@bufbuild/protobuf` v2). This yields runtime `DescMethod` objects that `LiveKitTransport` can invoke directly — no compiled ConnectES client needed.

The reflection implementation (`packages/tddy-service/src/reflection_service.rs`) decodes a combined `FileDescriptorSet` embedded at build time (`include_bytes!`, produced by a descriptor-only `prost_build` pass in `packages/tddy-service/build.rs`). It indexes files by name and by declared symbol for efficient lookup. `list_services` returns only the names registered in the host's `MultiRpcService` (not all compiled protos), so each participant exposes only what it actually serves.

## Dynamic invocation

Because `LiveKitTransport` derives `service`, `method`, and `methodKind` at runtime from a `DescMethod` (`.parent.typeName`, `.name`, `.methodKind`), a `DescMethod` synthesized from a runtime `createFileRegistry` is indistinguishable from a compiled one. The playground calls `transport.unary(method, …)` or `transport.stream(method, …)` directly — no `createClient` required. Request bytes come from `fromJson(method.input, parsedJson)` and responses decode via `fromJson`/`toJsonString`.

## Registration

`reflection_entry_from(service_names, DESCRIPTOR_BYTES) -> ServiceEntry` is a helper (name `"grpc.reflection.v1.ServerReflection"`) that any participant assembling a `MultiRpcService` can push. Registered in:

- `packages/tddy-daemon/src/main.rs` — after `rpc_entries` is assembled.
- `packages/tddy-coder/src/run.rs` — at every `MultiRpcService` assembly site.

## Transport note

The reflection bidi stream runs over the existing `LiveKitTransport` / `tddy-rpc` data channel, same as all other services. Descriptors are returned as serialized `FileDescriptorProto` bytes in `file_descriptor_response.file_descriptor_proto[]` — each element is one proto file. The bidi stream stays open while the user browses multiple services, re-using a single LiveKit session.

## Related documentation

- [gRPC remote control and transport stack](../../packages/tddy-livekit/docs/README.md) *(when written)*
- [LiveKit common-room peer discovery](livekit-peer-discovery.md)
- [Web terminal (screen/routing patterns)](../web/web-terminal.md)
- [Daemon changelog](changelog.md)
- [Web changelog](../web/changelog.md)
