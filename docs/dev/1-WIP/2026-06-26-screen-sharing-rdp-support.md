# Changeset: screen-sharing-rdp-support — Generalize VNC → Screen Sharing + add RDP support

**Date:** 2026-06-26  
**Branch:** `rdp-support`  
**Packages:** `tddy-service`, `tddy-screenshare` (new), `tddy-vnc`, `tddy-rdp` (new), `tddy-daemon`, `tddy-web`  
**Feature PRD:** [docs/ft/web/screen-sharing-sessions.md](../../ft/web/screen-sharing-sessions.md)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] **Proto rename** — `vnc.proto` → `screen_sharing.proto` (package `screen_sharing`, service `ScreenSharingService`, methods `ListTargets`/`AddTarget`/`RemoveTarget`/`UnlockVault`/`StartStream`/`StopStream`; add `Protocol` enum VNC=1/RDP=2; `VncTarget` → `ScreenSharingTarget` with `protocol` field); `vnc_input.proto` → `screen_sharing_input.proto` (package `screen_sharing_input`, service `ScreenSharingInputService`, messages `ScreenSharingPointerEvent`/`ScreenSharingKeyEvent`/`ScreenSharingInputEvent`/`ScreenSharingInputAck`)
- [x] **tddy-service build.rs** — rename compile blocks + descriptor list entries; update `src/lib.rs` `pub mod` includes
- [ ] **New crate `tddy-screenshare`** — `ScreenSharingClient` trait (`connect`/`framebuffer_dimensions`/`next_frame`/`inject_pointer`/`inject_key`); generic `Streamer` (framebuffer→LiveKit, `TrackSource::Camera`); generic `run_bridge<C: ScreenSharingClient>(config)` pump loop + `ScreenSharingInputService` server over LiveKit data channel; `BridgeConfig` (host/port/password, protocol-neutral); pixel helpers (`rgba_to_abgr`)
- [ ] **`tddy-vnc` refactor** — depend on `tddy-screenshare`; delete shared streamer/bridge/common; `VncClient implements ScreenSharingClient` via `vnc-rs` (replace all `bail!("not implemented")` stubs: connect, framebuffer capture, RFB input); `main.rs` reads `BridgeConfig` from stdin, calls `run_bridge::<VncClient>`
- [ ] **New crate `tddy-rdp`** — `RdpClient implements ScreenSharingClient` via `ironrdp` (TLS/RDP handshake, graphics pipeline → RGBA frames, fast-path input for pointer/key events from `ScreenSharingInputEvent`); `main.rs` reads `BridgeConfig`, calls `run_bridge::<RdpClient>`; add `ironrdp` to Cargo.toml
- [x] **Daemon `screen_sharing_service.rs`** (was `vnc_service.rs`) — `ScreenSharingServiceImpl` implementing `ScreenSharingService`; `add_target` persists `protocol`; `start_stream` dispatches to `tddy-vnc` (VNC) or `tddy-rdp` (RDP) bridge binary; bridge identity `screenshare-<session>-<target>`, track name `screenshare:<target_id>`; FIXME bridge-spawn remains
- [x] **Daemon `screen_sharing_vault.rs`** (was `vnc_vault.rs`) — `ScreenSharingVault`, domain `ScreenSharingTarget` with `protocol` field; vault file `.screen-sharing.yaml`; verifier magic `tddy-screenshare-vault-v1`
- [ ] **Daemon `config.rs`** — `VncConfig` → `ScreenSharingConfig` with `vnc_binary_path` + `rdp_binary_path`; `resolve_binary_path_for_protocol(config, protocol) -> String`; `DaemonConfig` field `screen_sharing: Option<ScreenSharingConfig>`; config YAML key `screen_sharing:`
- [x] **Daemon `lib.rs`** — added `pub mod screen_sharing_service; pub mod screen_sharing_vault` (VNC mods kept)
- [ ] **Daemon `main.rs`** — register `ScreenSharingServiceServer` into `rpc_entries` (service is currently implemented but never mounted on the Connect router)
- [x] **Web: regenerate proto clients** — `bun run generate` → `src/gen/screen_sharing_pb.ts` + `src/gen/screen_sharing_input_pb.ts` (VNC files kept)
- [x] **Web: new components** — `SessionScreenSharingTab.tsx`, `ScreenSharingOverlay.tsx`, `ScreenSharingPassphraseDialog.tsx` (new, VNC equivalents kept); `screenSharingTabState.ts` (new)
- [x] **Web: protocol selector in Add form** — VNC/RDP choice drives port default (5900/3389); submits `Protocol.VNC` or `Protocol.RDP` in `AddTargetRequest`; target rows display protocol label
- [x] **Web: inspector tab** — `InspectorTabs.tsx`: tab type `"screen-sharing"`, label "Screen Sharing"; `data-testid="sessions-inspector-tab-screen-sharing"`
- [x] **Web: `SessionInspectorDrawer.tsx`** — imports `ScreenSharingService` from `screen_sharing_pb`; wires all 6 `on*` callbacks to `SessionScreenSharingTab`
- [x] **Web: fix `room is not defined`** — `SessionMainPane.tsx` was missing `room?: Room | null` prop; caused all VNC + screen sharing Cypress tests to fail with runtime error

## Acceptance tests (RED — failing, awaiting green)

- [x] `packages/tddy-daemon/tests/screen_sharing_service_acceptance.rs`
- [x] `packages/tddy-daemon/tests/screen_sharing_vault_acceptance.rs`
- [x] `packages/tddy-web/cypress/component/SessionInspectorScreenSharingAcceptance.cy.tsx`
- [x] `packages/tddy-web/cypress/component/SessionScreenSharingTargetRowsAcceptance.cy.tsx`

## Unit tests (RED — failing, awaiting green)

- [x] `packages/tddy-web/src/components/sessions/screenSharingTabState.test.ts`
- [ ] `packages/tddy-screenshare/src/` — `ScreenSharingClient` trait mock tests; bridge input dispatch tests
- [ ] `packages/tddy-rdp/src/` — `RdpClient` integration test against xrdp container (testcontainers reuse)
- [ ] `packages/tddy-vnc/src/` — `VncClient` integration test against VNC server container

## Delta summary (to be filled in during green phase)

### `tddy-service`
- `proto/screen_sharing.proto` (new — was `vnc.proto`)
- `proto/screen_sharing_input.proto` (new — was `vnc_input.proto`)
- `build.rs` — updated compile blocks and descriptor list
- `src/lib.rs` — updated `pub mod` includes

### `tddy-screenshare` (new crate)
- `src/client.rs` — `ScreenSharingClient` trait
- `src/streamer.rs` — generic LiveKit framebuffer publisher
- `src/bridge.rs` — `run_bridge<C>` pump loop + `ScreenSharingInputService` server
- `src/config.rs` — `BridgeConfig`
- `src/common.rs` — `rgba_to_abgr`

### `tddy-vnc` (refactored)
- `src/vnc_client.rs` — `VncClient implements ScreenSharingClient` (real `vnc-rs` impl)
- `src/main.rs` — `run_bridge::<VncClient>`

### `tddy-rdp` (new crate)
- `src/rdp_client.rs` — `RdpClient implements ScreenSharingClient` (IronRDP)
- `src/main.rs` — `run_bridge::<RdpClient>`
- `Cargo.toml` — `ironrdp` dependency

### `tddy-daemon`
- `src/screen_sharing_service.rs` (was `vnc_service.rs`)
- `src/screen_sharing_vault.rs` (was `vnc_vault.rs`)
- `src/config.rs` — `ScreenSharingConfig`, `resolve_binary_path_for_protocol`
- `src/lib.rs` — renamed `pub mod` lines
- `src/main.rs` — `ScreenSharingServiceServer` mounted in `rpc_entries`

### `tddy-web`
- `src/gen/screen_sharing_pb.ts` (regenerated)
- `src/gen/screen_sharing_input_pb.ts` (regenerated)
- `src/components/sessions/SessionScreenSharingTab.tsx`
- `src/components/sessions/ScreenSharingOverlay.tsx`
- `src/components/sessions/ScreenSharingPassphraseDialog.tsx`
- `src/components/sessions/screenSharingInput.ts`
- `src/components/sessions/screenSharingTabState.ts`
- `src/components/sessions/InspectorTabs.tsx` — tab id + label updated
- `src/components/sessions/SessionInspectorDrawer.tsx` — imports `ScreenSharingService`
