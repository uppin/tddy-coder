# Changeset: RPC Playground

**Date:** 2026-06-15  
**Branch:** feature/host-rpcs  
**Status:** Complete — all tests passing; PR #207 open for review; follow-up fixes applied 2026-06-16

## Summary

Add a "RPC Playground" web UI screen and the backend reflection infrastructure that powers it. Any connected LiveKit participant (daemon, coder session, Claude-CLI session) can be inspected: its RPC services are discovered via standard gRPC ServerReflection, protobuf descriptors are fetched lazily, and any method can be invoked dynamically over the existing LiveKit binary channel with a synced JSON ⇄ generated-form request editor.

## TODOs

- [x] Create/update PRD documentation (`docs/ft/daemon/rpc-playground.md`)
- [x] Create changeset (`docs/dev/1-WIP/2026-06-15-changeset-rpc-playground.md`)
- [x] Write failing acceptance tests (Cypress)
- [x] Write failing unit/integration tests (Rust + bun)
- [ ] Backend — Phase 0: `FileDescriptorSet` emission (build.rs + reflection.proto)
- [ ] Backend — Phase 1: `MultiRpcService::service_names()` in `tddy-rpc`
- [ ] Backend — Phase 2: `reflection_service.rs` + `ServerReflectionImpl`
- [ ] Backend — Phase 3: Register reflection `ServiceEntry` in daemon + coder
- [ ] Frontend — Phase 4: Generate reflection TS (buf), `reflection.ts`, `registry.ts`
- [ ] Frontend — Phase 5: `protoMessageJson.ts`, `RpcRequestEditor.tsx`, `ProtoMessageBuilder.tsx`
- [ ] Frontend — Phase 6: `invoke.ts`, `StreamPlayground.tsx`
- [ ] Frontend — Phase 7: Screen + routing + nav (`RpcPlaygroundAppPage.tsx`, `appRoutes.ts`, `DaemonNavMenu.tsx`)

## Packages touched

`tddy-rpc`, `tddy-service`, `tddy-daemon`, `tddy-coder`, `tddy-livekit-web`, `tddy-web`, `docs`

## Acceptance tests (Cypress — failing)

### `packages/tddy-livekit-web/cypress/component/reflection.cy.tsx`
- `discovers EchoService via ServerReflectionInfo bidi stream` — starts echo server with reflection registered, asserts `list_services` response contains `test.EchoService`.
- `fetches Echo method descriptor and invokes it dynamically` — fetches descriptor via `file_containing_symbol`, builds runtime registry, invokes `Echo` with `{"message":"hi"}` over `LiveKitTransport` without a compiled client, asserts response JSON contains `"message": "hi"`.
- `invokes EchoServerStream dynamically` — asserts 3 streamed response chunks decoded from `method.output`.
- `invokes EchoBidiStream dynamically via interactive stream` — sends 3 messages, asserts 3 decoded responses.

### `packages/tddy-web/cypress/component/RpcPlaygroundScreen.cy.tsx`
- `renders participant picker and service/method tree` — mounts `RpcPlaygroundScreen` with mocked reflection, asserts `[data-testid="rpc-playground-participant-select"]` and a service node for `test.EchoService`.
- `selecting a method seeds request editor with default JSON` — clicks a method, asserts `[data-testid="rpc-request-editor"]` contains `"message"` field.
- `toggling builder and raw view retains values` — enters value in builder, switches to raw, asserts value preserved; switches back, asserts field still set.
- `clicking Invoke shows decoded response` — intercepts reflection RPCs, clicks Invoke, asserts `[data-testid="rpc-response"]` contains expected JSON.
- `shows error code and message on RPC failure` — mock transport returns `NOT_FOUND`, asserts error code displayed.

### Route + nav (bun + Cypress)
- `appRoutes.test.ts` additions: `isRpcPlaygroundPath("/rpc-playground")` is true; other paths false.
- `DaemonNavMenu` renders `shell-menu-rpc-playground` entry and navigates to `/rpc-playground`.

## Unit / integration tests (Rust — failing)

### `packages/tddy-rpc/src/bridge.rs` (new test module)
- `multi_rpc_service_exposes_service_names` — builds `MultiRpcService` with two entries, asserts `service_names()` returns both names in registration order.

### `packages/tddy-service/src/integration_tests.rs` (new `reflection_acceptance` module)
- `service_descriptor_set_embeds_all_registered_services` — embedded `SERVICE_DESCRIPTOR_BYTES` constant decodes as a `FileDescriptorSet` covering echo, token, auth, connection, and terminal services.
- `reflection_list_services_returns_only_registered` — build `MultiRpcService` with Echo + Token, call `ListServices` via reflection bridge, assert exactly `["test.EchoService", "token.TokenService"]` returned.
- `reflection_file_containing_symbol_returns_file_and_deps` — query `"test.EchoService"` symbol, assert response contains a decodable `FileDescriptorProto` for the echo service file.
- `reflection_file_by_filename_returns_requested_file` — query by the exact `.proto` filename, assert decodable `FileDescriptorProto`.
- `reflection_unknown_symbol_returns_error_response` — query `"nonexistent.Service"`, assert `error_response` with `NOT_FOUND` code, no panic.
- `reflection_routed_through_rpc_bridge` — drive `ServerReflectionInfo` bidi stream via `RpcBridge::start_bidi_stream("grpc.reflection.v1.ServerReflection", "ServerReflectionInfo", rx)`, assert a decodable `ServerReflectionResponse`.

## Unit tests (bun — failing)

### `packages/tddy-web/src/rpc-playground/registry.test.ts`
- `buildRegistry resolves service and method from embedded fixture bytes` — builds registry from test fixture `FileDescriptorSet`; `findMethod("test.EchoService", "Echo")` returns method with `methodKind === "unary"` and `input.typeName === "test.EchoRequest"`.
- `findMethod returns server_streaming kind for EchoServerStream` — asserts `methodKind === "server_streaming"`.
- `findMethod throws for unknown service`.
- `findMethod throws for unknown method on known service`.

### `packages/tddy-web/src/rpc-playground/protoMessageJson.test.ts`
- `defaultRequestJson returns valid proto-JSON` — for `EchoRequest` schema, parses as `{message: ""}`.
- `patchRequestJsonField sets one field and re-normalizes` — patch `message` field, assert resulting JSON contains the new value.
- `round-trips form to raw and back without losing values` — patch then parse reads same value back.
- `parseRequestJson with invalid JSON surfaces error without throwing` — `{ error: "..." }` returned, no exception.

### `packages/tddy-web/src/rpc-playground/invoke.test.ts` (fake `Transport`)
- `invokeRpc unary serializes request JSON and returns decoded JSON response` — fake transport echoes input; round-trip confirmed.
- `invokeRpc unary passes correct service typeName and method name to transport` — assert mock saw `parent.typeName === "test.EchoService"` and `name === "Echo"`.
- `invokeRpc maps ConnectError to { code, message }` — fake transport throws `ConnectError(NOT_FOUND)`, asserts error object returned.
- `invokeRpc server-streaming collects multiple decoded chunks` — fake transport yields 3 response frames, asserts 3 decoded JSON strings.

## 2026-06-16 follow-up fixes

- **Hash-based routing**: All client-side routing switched from `pushState`/`pathname` to `#`-based hash routing. Page reloads at any route (e.g., `/#/rpc-playground`, `/#/terminal/session-id`) no longer return 404. Auth callback still uses `window.location.pathname` (real redirect from GitHub OAuth).
- **Daemon LiveKit RPC participant**: `tddy-daemon/src/main.rs` now spawns a `LiveKitParticipant` for the common room with the full set of daemon RPC services (identity: `daemon-{local_id}`). The RPC Playground can now inspect daemon services via LiveKit transport without HTTP streaming issues.
- **LiveKit test auto-start**: Cypress tests in `tddy-livekit-web` and `tddy-web` now auto-start a Docker LiveKit container when `LIVEKIT_TESTKIT_WS_URL` is not set. Skipped tests converted to unconditional `describe` blocks.
- **Fix pty_relay test cfg guard**: `test_encode_resize_uses_osc_format_matching_virtual_tui` was missing `#[cfg(feature = "livekit")]` — added to match the function it tests.
- **RPC Playground participant filter**: Only `coder`-role participants (active sessions + the new daemon RPC participant) are shown in the host selector. The `daemon`-role discovery participant (which doesn't serve LiveKit RPC) is hidden.

## Files created / modified

### New files
- `docs/ft/daemon/rpc-playground.md`
- `docs/dev/1-WIP/2026-06-15-changeset-rpc-playground.md`
- `packages/tddy-service/proto/grpc/reflection/v1/reflection.proto`
- `packages/tddy-service/src/reflection_service.rs`
- `packages/tddy-web/src/rpc-playground/reflection.ts`
- `packages/tddy-web/src/rpc-playground/registry.ts`
- `packages/tddy-web/src/rpc-playground/registry.test.ts`
- `packages/tddy-web/src/rpc-playground/protoMessageJson.ts`
- `packages/tddy-web/src/rpc-playground/protoMessageJson.test.ts`
- `packages/tddy-web/src/rpc-playground/invoke.ts`
- `packages/tddy-web/src/rpc-playground/invoke.test.ts`
- `packages/tddy-web/src/rpc-playground/RpcRequestEditor.tsx`
- `packages/tddy-web/src/rpc-playground/ProtoMessageBuilder.tsx`
- `packages/tddy-web/src/rpc-playground/StreamPlayground.tsx`
- `packages/tddy-web/src/rpc-playground/RpcPlaygroundAppPage.tsx`
- `packages/tddy-web/src/rpc-playground/RpcPlaygroundScreen.tsx`
- `packages/tddy-livekit-web/cypress/component/reflection.cy.tsx`
- `packages/tddy-web/cypress/component/RpcPlaygroundScreen.cy.tsx`

### Modified files
- `packages/tddy-rpc/src/bridge.rs` — add `service_names()`, failing test
- `packages/tddy-service/build.rs` — descriptor-only pass + reflection.proto codegen
- `packages/tddy-service/src/lib.rs` — export `reflection_service` module
- `packages/tddy-service/src/integration_tests.rs` — `reflection_acceptance` module
- `packages/tddy-daemon/src/main.rs` — push reflection entry
- `packages/tddy-coder/src/run.rs` — push reflection entry at all `MultiRpcService` sites
- `packages/tddy-livekit-web/buf.gen.yaml` — add reflection.proto input
- `packages/tddy-web/src/routing/appRoutes.ts` — `RPC_PLAYGROUND_ROUTE`, `isRpcPlaygroundPath`
- `packages/tddy-web/src/routing/appRoutes.test.ts` — add RPC playground path tests
- `packages/tddy-web/src/index.tsx` — add `/rpc-playground` branch in `App()`
- `packages/tddy-web/src/components/shell/DaemonNavMenu.tsx` — add RPC Playground menu item
