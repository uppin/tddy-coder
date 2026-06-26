# WIP Changeset: VNC Sessions

**Feature slug:** `vnc-sessions`  
**Branch:** `vnc-support`  
**Status:** Green phase complete — vault + service + web state/input + inspector tab UI implemented; bridge binary stubs remain for a follow-up PR

## Problem / Motivation

Operators running code inside VMs or remote graphical environments have no way to see or
interact with the desktop from the tddy web UI. We add VNC support so the session inspector
can manage VNC targets whose desktops are streamed live into the browser as a LiveKit video
track with full remote-control input forwarding.

Reference implementation for the VNC↔LiveKit bridge:
`~/Code/makers-lt/common/vnc-livekit/` (vnc-rs + livekit 0.7 + RGBA→I420 framebuffer pump).

## TODO

- [x] Create/update PRD documentation
  - `docs/ft/web/vnc-sessions.md`
- [x] Create changeset (this file)
- [x] Failing acceptance tests written
  - `packages/tddy-web/cypress/component/SessionInspectorVncAcceptance.cy.tsx` (4 CT tests — failing, UI stubs pending)
- [x] Failing unit/integration tests written + now passing
  - `packages/tddy-daemon/tests/vnc_vault_acceptance.rs` (7 tests — all pass)
  - `packages/tddy-daemon/tests/vnc_service_acceptance.rs` (5 tests — all pass)
  - `packages/tddy-web/src/components/sessions/vncTabState.test.ts` (5 bun tests — all pass)
  - `packages/tddy-web/src/components/sessions/vncInput.test.ts` (15 bun tests — all pass)
- [x] Implement production logic (`/green`)
  - `VncVault` — Argon2id key derivation + ChaCha20-Poly1305 encrypt/decrypt ✅
  - `VncServiceImpl` in tddy-daemon — control plane (add/list/remove/unlock/start/stop) ✅
  - `vncTabState.ts` reducer ✅
  - `vncInput.ts` helpers (coordinate scaling, keysym mapping, RFB masks) ✅
  - `VncStreamer`, `VncClient`, `bridge::run` — **stubs, follow-up PR** (bridge binary not yet wired)
  - `VncOverlay.tsx` — **stub, follow-up PR**
  - `SessionVncTab.tsx` ✅ — target list + Add form + passphrase-gated submit
  - `VncPassphraseDialog.tsx` ✅ — passphrase input dialog
  - `InspectorTabs.tsx` ✅ — "vnc" tab button added
  - `SessionInspectorDrawer.tsx` ✅ — VncService client wired, renders SessionVncTab
- [ ] Wrap changeset

## Files Changed

### tddy-service
- `packages/tddy-service/proto/vnc.proto` *(new)* — `VncService` control-plane messages and service definition.
- `packages/tddy-service/proto/vnc_input.proto` *(new)* — `VncInputService` bidi input stream (split from vnc.proto to avoid codegen duplicate-import issue with `TddyServiceGenerator`).
- `packages/tddy-service/build.rs` — two separate VNC codegen passes + descriptor entries for both protos.
- `packages/tddy-service/src/lib.rs` — expose `proto::vnc` and `proto::vnc_input` modules.

### tddy-vnc (new package)
- `packages/tddy-vnc/Cargo.toml` *(new)* — lib + bin; deps: tddy-livekit, livekit 0.7, vnc-rs, image, prost, tokio, anyhow, env_logger.
- `packages/tddy-vnc/src/lib.rs` *(new)* — re-exports common, vnc_client, streamer, bridge.
- `packages/tddy-vnc/src/common.rs` *(new, implemented)* — `char_to_keysym`, `rgba_to_abgr` with unit tests.
- `packages/tddy-vnc/src/vnc_client.rs` *(new, stub)* — `VncClientState`; connect/input methods TBD.
- `packages/tddy-vnc/src/streamer.rs` *(new, stub)* — `VncStreamer`; RGBA→I420→LiveKit TBD.
- `packages/tddy-vnc/src/bridge.rs` *(new, stub)* — `run` pump loop TBD.
- `packages/tddy-vnc/src/main.rs` *(new)* — reads JSON `BridgeConfig` from stdin, calls `bridge::run`.
- `Cargo.toml` (workspace root) — add `"packages/tddy-vnc"` to members.

### tddy-daemon
- `packages/tddy-daemon/src/vnc_vault.rs` *(new, implemented)* — `VncVault` with Argon2id KDF + ChaCha20-Poly1305 AEAD; `.vnc.yaml` at mode 0600.
- `packages/tddy-daemon/src/vnc_service.rs` *(new, implemented)* — `VncServiceImpl` with all 6 RPCs; bridge spawning marked FIXME.
- `packages/tddy-daemon/src/lib.rs` — expose `pub mod vnc_service` and `pub mod vnc_vault`.
- `packages/tddy-daemon/src/config.rs` — add `VncConfig` with `binary_path` (sibling-binary resolution) and `resolve_vnc_binary_path` helper.
- `packages/tddy-daemon/Cargo.toml` — add `argon2`, `chacha20poly1305`, `rand`; no longer depends on `tddy-vnc` lib (vault ownership moved to daemon).

### tddy-web
- `packages/tddy-web/src/components/sessions/vncTabState.ts` *(new, implemented)* — `VncTabState` + `applyVncTabAction` reducer.
- `packages/tddy-web/src/components/sessions/vncInput.ts` *(new, implemented)* — `scaleCoordinates`, `keyboardEventToKeysym`, `mouseButtonToRfbMask`, `wheelDeltaToRfbMask`.
- `packages/tddy-web/src/components/sessions/SessionVncTab.tsx` *(new, implemented)* — target list, Add form, passphrase-gated submit.
- `packages/tddy-web/src/components/sessions/VncOverlay.tsx` *(new, stub)* — renders null; follow-up PR.
- `packages/tddy-web/src/components/sessions/VncPassphraseDialog.tsx` *(new, implemented)* — passphrase input dialog with confirm/cancel.
- `packages/tddy-web/src/components/sessions/InspectorTabs.tsx` — added `"vnc"` to `InspectorTab` union + VNC tab button.
- `packages/tddy-web/src/components/sessions/SessionInspectorDrawer.tsx` — wires VncService client, renders `SessionVncTab` for the vnc tab.
- `packages/tddy-web/src/gen/vnc_pb.ts` *(new, generated)* — VncService TypeScript descriptor (buf generate).
- `packages/tddy-web/src/gen/vnc_input_pb.ts` *(new, generated)* — VncInputService TypeScript descriptor (buf generate).
- `packages/tddy-web/cypress/support/testIds.ts` — add VNC test ID constants.
- `packages/tddy-web/cypress/support/pages/sessionsDrawerPage.ts` — add VNC page helpers.
- `packages/tddy-web/cypress/support/rpc/vncRpcs.ts` *(new)* — VNC RPC intercept helpers.
- `packages/tddy-web/src/test-utils/dom-preload.ts` *(new)* — `KeyboardEvent` polyfill for bun test environment.
- `packages/tddy-web/bunfig.toml` — preload `dom-preload.ts` for bun tests.
- `bunfig.toml` (root) — preload `dom-preload.ts` for bun tests.

### Tests
- `packages/tddy-daemon/tests/vnc_vault_acceptance.rs` *(new)* — 7 tests, all pass
- `packages/tddy-daemon/tests/vnc_service_acceptance.rs` *(new)* — 5 tests, all pass
- `packages/tddy-web/cypress/component/SessionInspectorVncAcceptance.cy.tsx` *(new)* — 5 Cypress CT tests, all passing
- `packages/tddy-web/src/components/sessions/vncTabState.test.ts` *(new)* — 5 bun tests, all pass
- `packages/tddy-web/src/components/sessions/vncInput.test.ts` *(new)* — 15 bun tests, all pass

### Docs
- `docs/ft/web/vnc-sessions.md` — PRD (AC-VNC-1 through AC-VNC-8)
- `docs/dev/1-WIP/vnc-sessions.md` — this changeset

## Design Decisions

- **Vault ownership**: `vnc_vault.rs` lives in `tddy-daemon` (not `tddy-vnc`). The binary only receives a decrypted password via stdin config — no vault dependency in the spawned binary.
- **Proto split**: `vnc.proto` (VncService) and `vnc_input.proto` (VncInputService) are separate files because `TddyServiceGenerator` emits module-level `use` imports once per service, causing duplicate-import compile errors when two services share a proto file.
- **Lazy passphrase**: `UnlockVncVault` creates a new vault if none exists, treating the passphrase as the vault passphrase. Subsequent calls on an existing vault validate the passphrase.
- **Binary path config**: `VncConfig.binary_path` — empty string at runtime resolves to `current_exe` sibling (`tddy-vnc`) with PATH fallback, matching the `tddy-tools` resolution pattern.
