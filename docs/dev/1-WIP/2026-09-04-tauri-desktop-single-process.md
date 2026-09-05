# Changeset: Tauri desktop — single-process daemon with a UI

**Date**: 2026-09-04
**Status**: 🚧 In Progress
**Type**: Architecture Change

## Affected Areas

**CRITICAL**: List ALL areas with code or documentation changes:

- **Desktop app** (`packages/tddy-desktop/`): Electrobun replaced in place by a Tauri app
  - `electrobun.config.ts`, `src/bun/**` (9 modules + 5 test files), `test/acceptance/**`, `test/e2e/**`, `scripts/electrobun-dev.ts`, `resources/bin/` — **deleted**
  - `src-tauri/` (new Rust crate): `main.rs`, `lib.rs`, `ipc.rs`, `Cargo.toml`, `tauri.conf.json`, `build.rs`
  - `package.json` — `electrobun` dependency removed, scripts driven by `cargo tauri`
- **Webview IPC RPC flavour** (`packages/tddy-tauri-rpc/`, new crate): `RpcBridge` hosted over a frame-oriented duplex channel; depends on `tddy-rpc` only, not on `tauri`
- **Browser envelope engine** (`packages/tddy-rpc-web/`, new package): the transport-agnostic envelope engine extracted from `tddy-livekit-web`
- **Browser Tauri transport** (`packages/tddy-tauri-web/`, new package): `createTauriTransport()` over `invoke` + `Channel`
- **LiveKit browser transport** (`packages/tddy-livekit-web/`): `src/transport.ts` refactored onto the extracted core (1067 → 462 lines, keeping the room listeners, chunked publish, metering, reassembly and the factory); `src/async-queue.ts` becomes a re-export. Public exports unchanged; **one observable behaviour change** — an aborted *unary* call now rejects with `ConnectError(Code.Canceled)` instead of a plain `Error("cancelled")`, because unary and the three streaming kinds now share one call-lifetime mechanism. A `ConnectError` is what a ConnectRPC transport should reject with, no test pinned the old type (the five LiveKit suites pass unedited), and no consumer in `tddy-web` matches on it
- **Daemon** (`packages/tddy-daemon/`):
  - `src/main.rs` — bootstrap extracted: **959 → 183 lines** (−838/+62). What stays is what only a binary owns: args, `SIGPIPE`, logger init, the pre-tokio spawn-worker fork, `set_git_ssh_command`, `startup_config_check`, the tokio runtime, the HTTP listener and the SIGTERM handler
  - `src/runtime.rs` (new) — `build(config, RuntimeOptions) -> DaemonRuntime`. **Assembly, not I/O**: it binds no socket, joins no room and spawns no task. Everything live comes back as `RuntimeTasks` for the host to `spawn()` — the common-room participant, peer discovery, the local UDS transport, the LSP idle reaper, the relay idle monitor and the Telegram dispatcher. That is what makes two rosters reproducible in one process instead of competing for a port, a socket path or a LiveKit identity. Also de-duplicated the "is the LiveKit block complete" predicate that peer discovery and common-room registration each carried a copy of
  - six stale doc references to `main.rs` retargeted to `runtime.rs` (`bsp_service.rs`, `config.rs`, `connection_service.rs` ×2, `livekit_peer_discovery.rs`, `screen_sharing_service.rs`)
  - `src/daemon_config_service.rs` (new) — `DaemonConfigService`: authenticate, persist, apply
  - `src/daemon_settings.rs` (new) — the settings rules as pure functions (redaction, validation, secret merging, restart-required, reconnect decision)
  - `src/common_room_supervisor.rs` (new) — the common-room connection under a `watch`-driven supervisor: `CommonRoomTarget` (a LiveKit block proven joinable), `SupervisedCommonRoom` (the handle `DaemonConfigService` reconfigures through), `CommonRoomSupervisorTask` (owns the running connection, reconciles it **down-then-up**) and `DaemonCommonRoomConnector` (the real join: RPC roster + peer discovery)
  - `src/livekit_peer_discovery.rs` — `spawn_common_room_discovery_task` split into `spawn_common_room_discovery_loop` (returns its handle, so a reconfiguration can stop it) and `spawn_oauth_loopback_tunnel` (follows the room slot, so it outlives a reconnect and binds its callback ports once)
  - `src/auth.rs` — the session-token rule extracted as `session_token_authenticator`, so the config service authenticates by the same rule as `token.TokenService` rather than a second one
  - `src/runtime.rs` — `RuntimeOptions::with_config_path`; `daemon_config.DaemonConfigService` registered in the roster for both hosts; the common-room registration replaced by the supervisor task
  - `src/config.rs` — 18 structs gained `serde::Serialize` and 52 `#[serde(default)]` attributes gained `skip_serializing_if`, so a written file carries no noise; `deny_unknown_fields` untouched
  - `src/lib.rs` — new module declarations
- **Core logging config** (`packages/tddy-core/`): `src/log_backend.rs` — `LogConfig` and its six nested types gained `Serialize`, plus a hand-written `impl Serialize for LogOutput` mirroring its existing custom `Deserialize`. **Unavoidable and outside the original plan**: `DaemonConfig.log` is `Option<LogConfig>`, so `DaemonConfig: Serialize` cannot exist without it. The alternative — `#[serde(skip_serializing)]` on `log:` — would have silently deleted an operator's entire logging block on their first settings save. This widens the blast radius to every `tddy-core` consumer; `tddy-core`'s suite passes (312) and clippy is clean
- **RPC service definitions** (`packages/tddy-service/`): `proto/daemon_config.proto` (new) + generated code, service registration
- **Web app** (`packages/tddy-web/`):
  - `src/rpc/daemonTransportFlavour.ts` (new) — which flavour this page must use
  - `src/rpc/daemonTransport.ts` (new) — builds the flavour that names, both carrying one interceptor stack; `createDefaultHttpTransport` moved here out of `transportProvider.tsx` (re-exported from it)
  - `src/rpc/interceptedTransport.ts` (new) — `transportWithInterceptors`, so the *same* `Interceptor` functions layer onto a transport that is not `createConnectTransport`
  - `src/rpc/transportProvider.tsx` — the provider's default transport is now `createDefaultDaemonTransport`
  - `src/rpc/clientConfig.ts` (new) + `src/index.tsx` — client config over RPC when there is no HTTP origin, `fetch("/api/config")` when there is
  - `src/gen/daemon_config_pb.ts` (new, generated) — the config service client
  - `src/components/settings/**` (new) — settings screen + `SettingsAppPage` route container
  - `src/routing/appRoutes.ts`, `src/index.tsx`, `src/components/shell/DaemonNavMenu.tsx` — `#/settings` route and its navigation entry
  - `cypress/support/testIds.ts`, `cypress/support/drivers/daemonSettingsDriver.tsx` — settings test surface
- **Toolchain** (repo root): `flake.nix` (Linux WebKitGTK set, `cargo-tauri`), `Cargo.toml` (2 new members: `tddy-tauri-rpc`, `tddy-desktop/src-tauri`), `package.json` (2 new workspaces, `test` script)
- **CI** (`.github/workflows/ci.yml`): a *Transport package tests* step running `tddy-rpc-web` and `tddy-tauri-web` (both emit JUnit XML, so the existing report gate fails the job on a regression); the Rust jobs pick up the new crates through `--workspace`
- **Documentation**: `docs/ft/desktop/tddy-desktop-electrobun.md` (rewritten at wrap), `docs/ft/daemon/daemon-settings.md` (new at wrap), `docs/ft/web/local-web-dev.md`, `docs/ft/desktop/changelog.md`, `docs/ft/daemon/changelog.md`, `docs/ft/web/changelog.md`, `docs/dev/changesets.md`, `packages/tddy-daemon/docs/changesets.md`, `packages/tddy-livekit-web/docs/changesets.md`

## Related Feature Documentation

- [PRD-2026-09-04-tauri-desktop-single-process.md](../../ft/desktop/1-WIP/PRD-2026-09-04-tauri-desktop-single-process.md)
- [Tddy desktop app (Electrobun)](../../ft/desktop/tddy-desktop-electrobun.md) — the design being replaced
- [Local web development](../../ft/web/local-web-dev.md)
- [Codex OAuth relay (daemon)](../../ft/daemon/codex-oauth-relay.md)

## Summary

Replace the Electrobun shell with a Tauri app whose Rust process hosts `tddy-daemon` in-process, and carry UI↔daemon RPC over a new webview-IPC flavour of `tddy-rpc` instead of a loopback HTTP port. Add a `DaemonConfigService` and a settings screen so the app is configurable as what it now is: a daemon with a UI.

## Background

`packages/tddy-desktop` is a Bun main process that spawns `tddy-daemon` as a child, resolves its binary from four candidate paths, prepares a config file, loads the repo `.env`, waits for the daemon's HTTP port, and tears the child down on exit. Every UI↔daemon call then travels over `127.0.0.1:8899`, reachable by any local process. The orchestration logic is TypeScript standing in front of Rust behaviour it cannot type-check, and the two lifecycles fail independently.

`tddy-rpc` was built for exactly this: `proto/rpc_envelope.proto` describes itself as being for "a byte-oriented transport (LiveKit data channel, stdio pipes, ...)", and three flavours already exist — `tddy-connectrpc` (HTTP/axum), `tddy-livekit` (data channel), `tddy-stdio` (process pipes). Tauri's IPC is a byte-oriented duplex channel; the flavour is missing, not the framework.

## Scope

**High-level deliverables tracking progress throughout development:**

- [x] **Documentation**: PRD written; feature docs rewritten ([tddy-desktop-tauri.md](../../ft/desktop/tddy-desktop-tauri.md), [daemon-settings.md](../../ft/daemon/daemon-settings.md))
- [x] **Implementation**: Tauri app, webview-IPC flavour (Rust + TS), daemon runtime extraction, config service, settings screen
- [x] **Testing**: All acceptance tests passing — 988 Rust (`tddy-daemon` 649 = 636 lib + 2 + 11, `tddy-core` 312, `tddy-tauri-rpc` 13, `tddy-desktop` 14), 1 027 TypeScript unit (`tddy-web` 970, `tddy-livekit-web` 44, `tddy-rpc-web` 6, `tddy-tauri-web` 7) and 1 214 Cypress component specs
- [x] **Integration**: One web bundle drives desktop (IPC) and browser (HTTP); the same 15-service roster in both daemon hosts, pinned by `embedded_runtime.rs`; the running app dispatches `GetClientConfig` over the bridge with no listening socket
- [~] **Technical Debt**: Every gap is recorded below rather than closed. The ones that matter: YAML comments are lost on write, peer discovery does not follow a runtime reconnect, cancellation never reaches the peer, and the desktop host sends no Telegram lifecycle messages
- [~] **Code Quality**: `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` both clean on macOS. `cargo tauri build` produces a valid `.app`; the `.dmg` step and everything Linux are unverified here

## Technical Changes

### State A (Current)

**Desktop shell** — Electrobun, Bun main process, two OS processes:

- `src/bun/index.ts` calls `startEmbeddedDaemon()` at module load, then constructs an Electrobun `BrowserWindow` pointing at `VITE_URL` or `file://…/resources/web/index.html`.
- `src/bun/embedded-daemon.ts` resolves the workspace root (env → upward walk for `dev.desktop.yaml`/`Cargo.toml` → four-levels-up fallback), loads repo `.env` without overriding, prepares a config (substituting `CURRENT_USER`), resolves the binary from `TDDY_DAEMON_BINARY` → `resources/bin/tddy-daemon` → `target/release` → `target/debug`, and `Bun.spawn`s it.
- `src/bun/wait-for-daemon-rpc.ts` polls the daemon's HTTP port before the UI is usable.
- `src/bun/livekit-oauth-relay.ts`, `oauth-callback-server.ts`, `codex-oauth-metadata.ts` exist for tests only — production OAuth loopback TCP lives in `tddy-daemon`.
- `electrobun.config.ts` copies `resources/bin/tddy-daemon` into the bundle; `prebuild` runs `cargo build --release -p tddy-daemon`.

**Daemon** — one binary, bootstrap inline in `main()`:

- `src/main.rs:89-959`: parse args, load YAML, `apply_env_overrides`, build 15+ `tddy_rpc::ServiceEntry` values, append the reflection entry (`main.rs:850-851`), connect the LiveKit common room and register a filtered entry subset (`main.rs:875-895`), spawn the UDS `ConnectionService` server (`main.rs:630`), install signal handling, then `tddy_daemon::server::run_server(...)` (`main.rs:932-936`).
- `src/server.rs:20-82`: `MultiRpcService::new(rpc_entries)` → `RpcBridge` → `connect_router` merged into the axum app by `tddy_coder::web_server::serve_web_bundle_with_shutdown`, which also serves `/api/config` (`packages/tddy-coder/src/web_server.rs:88`) and falls back to the static bundle.
- No library entry point: everything above is unreachable except by running the binary.

**RPC framework** — transport-agnostic core, three flavours:

- `packages/tddy-rpc`: `envelope.rs` (`RpcRequest`/`RpcResponse` with `client_epoch` and `call_metadata`), `client_engine.rs`, `server_engine.rs`, `bridge.rs` (`RpcBridge`, `MultiRpcService`, `ServiceEntry`), `transport.rs` (`RpcClientTransport`, plus `FrameKind`/`encode_frame`/`FrameDecoder` for byte-stream transports).
- `packages/tddy-stdio`: `StdioEndpoint` over one framed duplex pipe — hosts a service for inbound requests and returns a client for outbound calls on the same channel. `tests/rpc_over_stdio.rs` drives it against a child-process fixture.
- `packages/tddy-livekit-web`: `src/transport.ts` (1067 lines) implements the ConnectRPC `Transport` over a LiveKit data channel — envelope encode/decode, `mintClientEpoch()` stale-frame rejection, call-metadata mismatch rejection, pending-call settlement, `AsyncQueue` streaming, `chunking.ts` for the data-channel frame limit, `RoomRpcRegistry`. Covered by `transport.test.ts`, `transport-stale-correlation.test.ts`, `transport-call-settlement.test.ts`, `transport-factory.test.ts`, `chunking.test.ts` and `cypress/component/transport.cy.tsx` (against a real Rust echo server).

**Web app** — two transports, HTTP is the default:

- `src/rpc/transportProvider.tsx:57-78` builds `createConnectTransport({ baseUrl: window.location.origin + "/rpc", useBinaryFormat: true })` with traffic-metering and auth-gate interceptors; `:102-119` builds the LiveKit transport for remote hosts. `RpcTransportProvider` (`:173`) is the injection seam tests already use.
- The only raw HTTP the app performs is `fetch("/api/config")` (`src/index.tsx:218,376`); everything else, uploads included, is RPC.

**Configuration** — read-only, startup-only:

- `DaemonConfig` (`packages/tddy-daemon/src/config.rs:235`, 2283-line module, `#[serde(deny_unknown_fields)]` throughout) is parsed once at startup, then mutated only by `apply_env_overrides`.
- The web app receives a read-only projection as `ClientConfig` from `GET /api/config`. There is **no** config RPC service (`packages/tddy-service/proto/` holds 20 services, none of them configuration) and no settings screen (`packages/tddy-web/src` has no settings component).

**Toolchain** — no Tauri support:

- `flake.nix` carries no WebKitGTK/GTK/libsoup inputs and no `cargo-tauri`.
- CI runs on `ubuntu-latest` only: `cargo clippy --workspace --all-targets`, `cargo build --workspace --bins --examples`, `cargo nextest run --workspace`, and Cypress component suites for `tddy-web` and `tddy-livekit-web`.
- Root `package.json` `test` includes `bun run --filter tddy-desktop test`.

### State B (Target)

**Desktop app** — Tauri, one OS process:

- `packages/tddy-desktop/src-tauri` is a workspace crate. `main.rs` loads the daemon config (same resolution rules: `TDDY_DAEMON_CONFIG` → repo-root `dev.desktop.yaml`, repo `.env` loaded without overriding), builds the daemon runtime via `tddy_daemon::runtime`, and hands the resulting `RpcBridge` to a `WebviewRpcHost` held in Tauri state.
- The window loads the bundle through Tauri's asset protocol, or `VITE_URL` in dev. External `http(s)` links open in the system browser via `tauri-plugin-opener`.
- **No child process, no binary resolution, no HTTP readiness wait, no TCP listener.**
- Bundles for macOS and Linux from `cargo tauri build`.

**Webview IPC flavour** — `packages/tddy-tauri-rpc`:

- `WebviewRpcHost<S: RpcService>` owns a `ServerEngine` and a registered `FrameSink`. `handle_request_frame(bytes)` decodes an `RpcRequest`, dispatches it, and writes every `RpcResponse` — one per stream message, `end_of_stream` on the last — to the sink, echoing `client_epoch` and `call_metadata` on each.
- `connect(sink, epoch)` replaces the registered sink and **abandons every stream opened by the previous connection**, which is the host-side counterpart of the browser's epoch check: a page reload restarts request ids while the host may still be streaming for the previous page.
- `FrameSink` is a trait (`fn send(&self, frame: Vec<u8>) -> Result<(), SinkClosed>`), so the crate has **no `tauri` dependency** and the whole flavour is driven by ordinary integration tests. `src-tauri` implements it over `tauri::ipc::Channel`.
- Two Tauri commands only: `tddy_rpc_connect(channel)` and `tddy_rpc_send(frame: Vec<u8>)`. Frames cross as raw bytes (`tauri::ipc::Request` raw body / `InvokeResponseBody::Raw`), never JSON.

**Browser side** — engine extracted, two thin flavours on top:

- `packages/tddy-rpc-web` exports `createEnvelopeTransport({ send, subscribe, targetLabel })`: request-id allocation, client epoch, pending-call settlement, call-metadata mismatch rejection, `AsyncQueue` streaming, ConnectRPC `Transport` conformance — with no knowledge of the byte pipe.
- `packages/tddy-livekit-web` keeps `RoomRpcRegistry`, the `tddy-rpc` data-channel topic, chunking and reconnect handling, and consumes the extracted core. Public exports and behaviour unchanged.
- `packages/tddy-tauri-web` exports `createTauriTransport()`: `send` → `invoke("tddy_rpc_send", …)`, `subscribe` → a `Channel` registered through `invoke("tddy_rpc_connect", …)`.

**Web app** — one bundle, transport chosen at runtime:

- `RpcTransportProvider` builds the webview-IPC transport when the Tauri bridge is present on `window`, and today's `/rpc` Connect transport otherwise. Interceptor stack (traffic metering, auth gate) is applied identically to both.
- Client config comes from `DaemonConfigService.GetClientConfig` when there is no HTTP origin, and from `fetch("/api/config")` in the browser.
- A settings screen reads `GetConfig`, edits the LiveKit block, and saves through `UpdateConfig`.

**Daemon** — library entry point, runtime-reconfigurable LiveKit, config service:

- `tddy_daemon::runtime::build(config, RuntimeOptions) -> DaemonRuntime` returns `{ entries, bridge, config, livekit: CommonRoomHandle, shutdown }`. `RuntimeOptions` distinguishes hosts: the binary enables systemd socket activation and HTTP serving, the embedded host enables neither. `main.rs` becomes: parse args → load config → `runtime::build` → `server::run_server`.
- The service roster is produced by one function for both hosts, so it cannot drift.
- The common-room connection lives behind a supervisor with a `watch` channel; a LiveKit block change disconnects and reconnects it.
- `DaemonConfigService`: `GetConfig` (effective config, `api_secret`/tokens redacted), `UpdateConfig` (deserialize-validate → atomic write to the YAML path the daemon was loaded from → apply live what can be applied → return `restart_required` field paths for the rest), `GetClientConfig`.

**Toolchain**:

- `flake.nix` Linux `buildInputs` += `webkitgtk_4_1`, `libsoup_3`, `gtk3`, `glib-networking`, `librsvg`, `cairo`, `pango`, `gdk-pixbuf`, `atk`; `packages` += `cargo-tauri`.
- Root `Cargo.toml` members += `packages/tddy-tauri-rpc`, `packages/tddy-desktop/src-tauri`.
- Root `package.json` workspaces += `packages/tddy-rpc-web`, `packages/tddy-tauri-web`; `test` script's `tddy-desktop` leg replaced by the new packages.

### Delta (What's Changing)

#### Desktop app (`packages/tddy-desktop/`)

- **Architecture**: Bun main process + spawned daemon child → single Rust process hosting the daemon.
- **API**: `startEmbeddedDaemon()`/`waitForDaemonHttp()` removed; `tddy_rpc_connect`/`tddy_rpc_send` commands added.
- **Implementation**: 9 TS modules and 5 TS test files deleted; `src-tauri/{main,lib,ipc}.rs` + `tauri.conf.json` added.
- **Dependencies**: `electrobun` removed; `tauri`, `tauri-build`, `tauri-plugin-opener`, `@tauri-apps/api`, `@tauri-apps/cli` added.

#### Webview IPC flavour (`packages/tddy-tauri-rpc/`, new)

- **Architecture**: fourth `tddy-rpc` flavour, modelled on `tddy-stdio`'s single-duplex-channel endpoint but message-oriented (no length-prefix frame codec, no chunking).
- **API**: `WebviewRpcHost::new(service)`, `::connect(sink, epoch)`, `::handle_request_frame(bytes)`; `FrameSink` trait.
- **Implementation**: `ServerEngine` dispatch, per-connection stream registry, abort handling, epoch/metadata echo.

#### Browser transports (`packages/tddy-rpc-web/` new, `packages/tddy-tauri-web/` new, `packages/tddy-livekit-web/`)

- **Architecture**: envelope engine extracted behind a byte-pipe interface; LiveKit and Tauri become thin adapters.
- **API**: `createEnvelopeTransport({ send, subscribe })` exported from `tddy-rpc-web`; `createTauriTransport()` from `tddy-tauri-web`; `tddy-livekit-web`'s exports unchanged.
- **Implementation**: ~600 lines move out of `transport.ts`; `async-queue.ts` relocates; chunking stays with LiveKit.

#### Web app (`packages/tddy-web/`)

- **Integration**: `RpcTransportProvider` gains runtime transport selection; `src/index.tsx` gains an RPC path for client config.
- **API**: new settings screen + route.
- **Dependencies**: `tddy-rpc-web`, `tddy-tauri-web` workspace deps.

#### Daemon (`packages/tddy-daemon/`)

- **Architecture**: bootstrap moves from `main()` into a library module with an options-driven host distinction; the common-room connection becomes supervised and reconfigurable.
- **API**: `runtime::build`, `RuntimeOptions`, `DaemonRuntime`, `CommonRoomHandle`, `DaemonConfigServiceImpl`.
- **Implementation**: `main.rs` shrinks from 959 lines to a wrapper; `daemon_config_service.rs` added; `config.rs` gains a validating serialize-and-write path.

#### RPC service definitions (`packages/tddy-service/`)

- **API**: `daemon_config.proto` — `GetConfig`, `UpdateConfig`, `GetClientConfig`, with `restart_required` on the update response.

#### Toolchain and CI (repo root, `.github/workflows/ci.yml`)

- **Dependencies**: WebKitGTK set + `cargo-tauri` in the dev shell — a hard prerequisite, because `cargo build --workspace` on `ubuntu-latest` fails without them once the Tauri crate is a member.
- **Integration**: component-test leg for the new TS packages; `--filter tddy-desktop test` replaced.

## Implementation Milestones

- [~] **M1 — Toolchain**: `flake.nix` extended (Linux WebKitGTK/GTK set + `cargo-tauri`); `./dev cargo build --workspace` succeeds on macOS with the Tauri crate present; `@tauri-apps/api` 2.11.1 + `@tauri-apps/cli` 2.11.4 installed from the private registry and the lockfile's registry-URL churn stripped back out. **Linux is unverified** — no Linux host available here
- [x] **M2 — Daemon runtime extracted**: `tddy_daemon::runtime::build` + thin `main.rs`; roster parity and no-listener tests pass; the existing daemon suite is unchanged and green
- [x] **M3 — Webview IPC host**: `packages/tddy-tauri-rpc` green against a fake `FrameSink` — unary, three streaming kinds, concurrency, abort, unknown service, epoch/metadata echo, reconnect abandonment
- [~] **M4 — Envelope engine extracted**: `packages/tddy-rpc-web` green; `tddy-livekit-web`'s five bun suites (44 tests) pass with no test edits. Its Cypress component suite — the one driving a **real Rust echo server** over a real data channel — still needs a run; it requires Docker plus `target/debug/examples/echo_server`
- [x] **M5 — Browser Tauri transport**: `packages/tddy-tauri-web` green against an in-memory IPC double
- [~] **M6 — Tauri app**: `packages/tddy-desktop/src-tauri` is a workspace member; window, asset protocol, real `Channel`/`invoke` wiring, embedded runtime, external links and shutdown all in place; on macOS the dashboard drives the in-process daemon (`GetClientConfig` dispatched over the bridge) with **no listening socket** (`lsof -a -iTCP -sTCP:LISTEN -p <pid>` empty, nothing on 8899); Electrobun deleted in the same step. **Linux is unverified**, and `cargo tauri build` has not been run on either platform (M10)
- [x] **M7 — Transport selection**: one bundle serves desktop (IPC) and browser (HTTP); `./web-dev` unaffected; client config over RPC in the desktop path
- [x] **M8 — Config service**: `daemon_config.proto`, `DaemonConfigService`, runtime LiveKit reconnect — redaction, YAML write-back, restart-required reporting, validation refusal. The service is in the roster of both hosts, wired to a real supervisor that tears the running common-room connection down and brings up the one the new block describes
- [x] **M9 — Settings screen**: reads effective config, saves LiveKit edits, surfaces restart-required fields and validation errors; reachable at `#/settings` from the daemon-mode navigation menu
- [~] **M10 — Bundles and lint**: `cargo fmt --all -- --check` clean and `cargo clippy --workspace --all-targets -- -D warnings` **exit 0** across the whole workspace with the Tauri crate in it. `cargo tauri build` on macOS produces a valid `Tddy Desktop.app` (Mach-O arm64, `CFBundleIdentifier dev.tddy.desktop`); the **`.dmg` step fails on this host only** — `bundle_dmg.sh` drives Finder over AppleScript to lay out the disk-image window and times out with `AppleEvent timed out (-1712)` for want of Automation permission, which is environmental rather than a defect in the app. **Linux bundles and CI are unverified** — no Linux host here

## Testing Plan

### Testing Strategy

**Determine Appropriate Test Level:**

- **E2E**: rejected. `tauri-driver` wraps `WebKitWebDriver` (Linux) and `msedgedriver` (Windows) and **cannot drive macOS**, so a WebDriver suite would cover one of the two target platforms while adding a CI dependency and minutes of runtime. There is no headless webview to drive on the primary development platform.
- **Integration**: the right level for the two new host-side boundaries. The webview-IPC flavour is a frame-in/frame-out component, and the config service is a validate-write-apply component — both fully observable without a UI, and both meaningless to unit-test below the boundary.
- **Unit**: the right level for the browser-side transports and the provider's transport selection, which are pure TypeScript over an injectable pipe.

**Primary Test Approach:**

Rust integration tests over a fake `FrameSink` for the webview-IPC host, and over a temp-file YAML plus a fake common-room supervisor for the config service. TypeScript `bun:test` for the extracted envelope engine, the Tauri transport and the provider's selection. Cypress component tests where React mounting is the thing under test — the settings screen.

The Tauri transport is tested with `bun:test` rather than Cypress because there is no real Tauri runtime inside a browser test either way: both would drive the same in-memory IPC double, and `bun:test` matches the precedent already set by `tddy-livekit-web`'s own `transport.test.ts` (Cypress there exists to reach a *real* Rust echo server over a real data channel, which has no analogue here).

**Deliberately not covered by automated tests:** the real `tauri::ipc::Channel`/`invoke` wiring inside `src-tauri` — the seam between Tauri's IPC layer and `FrameSink`. Recorded under Technical Debt with the manual verification step that stands in for it.

### Coverage Requirements

**Acceptance tests MUST cover (at appropriate test level):**

- [ ] **Happy path**: unary and all three streaming kinds over webview IPC; config read; config write applied live
- [ ] **Error scenarios**: unknown service; Connect error code propagation; update rejected by validation; IPC channel closed mid-stream
- [ ] **Edge cases**: two concurrent calls interleaving frames; a page reload abandoning the previous connection's streams; a config field that cannot be applied live
- [ ] **Integration points**: embedded runtime vs binary runtime roster parity; transport selection between desktop and browser; `tddy-livekit-web` unchanged after extraction

## Acceptance Tests

36 tests across 7 files. Every one is written and failing on a missing implementation.

### Webview IPC flavour — host side (`packages/tddy-tauri-rpc/tests/rpc_over_webview_ipc.rs`, 11)

Support: `packages/tddy-tauri-rpc/tests/support/mod.rs` — `a_webview_rpc_host()`, an `a_request_frame()` builder, `a_recording_sink()` (`RecordingSink` + `SinkFrames`), an `EchoService` covering unary / server stream / client stream / bidi / a never-completing stream, and a `ResponseAssertions` extension trait.

- [ ] **Integration**: `calls_a_unary_method_and_returns_the_exact_response_bytes`
- [ ] **Integration**: `delivers_every_server_stream_message_in_the_order_the_service_produced_them`
- [ ] **Integration**: `accepts_three_client_stream_messages_before_answering_with_one_response`
- [ ] **Integration**: `answers_a_bidirectional_stream_with_one_response_per_request_message`
- [ ] **Integration**: `keeps_two_concurrent_unary_calls_separate_when_their_frames_interleave`
- [ ] **Integration**: `answers_an_unknown_service_with_a_not_found_status`
- [ ] **Integration**: `echoes_the_client_epoch_and_call_metadata_back_on_every_response_frame`
- [ ] **Integration**: `abandons_streams_opened_by_a_previous_page_connection_when_the_webview_reconnects`
- [ ] **Integration**: `refuses_a_request_frame_that_arrives_before_the_webview_connects`
- [ ] **Integration**: `refuses_a_malformed_request_frame_and_keeps_serving_the_connection`
- [ ] **Integration**: `serves_the_echo_service_under_the_name_the_daemon_registers_it_with`

### Daemon runtime (`packages/tddy-daemon/tests/embedded_runtime.rs`, 2)

- [ ] **Integration**: `builds_the_same_service_roster_for_the_embedded_runtime_as_for_the_binary_runtime`
- [ ] **Integration**: `leaves_the_configured_web_port_unbound_when_built_for_an_embedded_host`

### Daemon config service (`packages/tddy-daemon/tests/daemon_config_service.rs`, 10)

Support: a temp-file YAML fixture, a `the_current_settings()` update builder, and `RecordingCommonRoom` — a `CommonRoomSupervisor` recording what the service asked the running connection to become.

- [ ] **Integration**: `returns_the_effective_configuration_with_the_livekit_api_secret_redacted`
- [ ] **Integration**: `reports_the_path_of_the_file_an_update_will_be_written_to`
- [ ] **Integration**: `writes_an_edited_livekit_url_back_to_the_yaml_file_the_daemon_was_loaded_from`
- [ ] **Integration**: `reconnects_the_common_room_when_the_livekit_url_changes`
- [ ] **Integration**: `keeps_the_common_room_connected_when_an_unrelated_field_changes`
- [ ] **Integration**: `persists_a_changed_web_port_and_reports_it_as_restart_required`
- [ ] **Integration**: `refuses_a_livekit_url_that_is_not_a_websocket_url_and_leaves_the_file_unchanged`
- [ ] **Integration**: `returns_the_client_config_the_web_bundle_otherwise_fetches_over_http`
- [ ] **Integration**: `refuses_to_return_the_configuration_to_a_caller_without_a_valid_session_token`
- [ ] **Integration**: `refuses_to_write_the_configuration_for_a_caller_without_a_valid_session_token`

### Browser envelope engine (`packages/tddy-rpc-web/src/envelope-transport.test.ts`, 2)

Support: `packages/tddy-rpc-web/src/test-utils/framePipeDouble.ts` — an in-memory `FramePipe` that records request frames and pushes response frames.

- [ ] **Unit**: `encodes one request frame per unary call with an incrementing request id`
- [ ] **Unit**: `settles a call only with the response frame that names its own method`

### Browser webview-IPC transport (`packages/tddy-tauri-web/src/transport.test.ts`, 5)

Support: `packages/tddy-tauri-web/src/test-utils/webviewIpcDouble.ts` — an in-memory `WebviewIpcBridge` with an awaitable `nextRequest()` and `answer` / `stream` / `streamPartially` / `fail` / `closeChannel`.

- [ ] **Unit**: `resolves a unary call through the webview IPC bridge`
- [ ] **Unit**: `yields every server-stream message in the order the host sent them`
- [ ] **Unit**: `surfaces the Connect error code carried by an error frame`
- [ ] **Unit**: `ignores response frames minted for a previous page's client epoch`
- [ ] **Unit**: `rejects a pending call when the IPC channel closes mid-stream`

### Transport selection (`packages/tddy-web/src/rpc/daemonTransportFlavour.test.ts`, 2)

- [ ] **Unit**: `names the webview IPC flavour when the page runs inside the Tauri host`
- [ ] **Unit**: `names the HTTP flavour when the page runs in a plain browser`

### Settings screen (`packages/tddy-web/cypress/component/DaemonSettingsAcceptance.cy.tsx`, 4)

Driver: `packages/tddy-web/cypress/support/drivers/daemonSettingsDriver.tsx` — `aDaemonConfigBackend()` / `aDaemonThatCannotApply()` / `aDaemonRefusingUpdates()` over `anInMemoryRpcBackend`, plus a fluent screen driver. Test ids in `cypress/support/testIds.ts`.

- [ ] **Component**: `shows the effective LiveKit configuration with the API secret masked`
- [ ] **Component**: `saves an edited LiveKit URL through UpdateConfig`
- [ ] **Component**: `lists the fields that need a restart after saving`
- [ ] **Component**: `keeps the entered values and shows the daemon's validation error when the update is rejected`


## Unit and Integration Tests

47 further tests below the acceptance boundary, where the rules are dense enough to deserve their
own coverage.

### Settings rules, as pure functions (`packages/tddy-daemon/src/daemon_settings.rs`, 16 cases)

`redacted_settings` and `apply_update` hold every rule the config service needs — redaction,
validation, secret merging, restart-required computation, reconnect decision — so the service is
left with authenticate, persist, apply. Tested in-module with `rstest` cases.

- [ ] **Unit**: `reports_a_stored_livekit_secret_as_set_without_returning_it`
- [ ] **Unit**: `reports_no_livekit_settings_when_the_daemon_has_no_livekit_block`
- [ ] **Unit**: `accepts_a_websocket_livekit_url` (`ws`, `wss`)
- [ ] **Unit**: `refuses_a_livekit_url_that_is_not_a_websocket_url` (`http`, `https`, empty, not-a-url)
- [ ] **Unit**: `keeps_the_stored_api_secret_when_an_update_omits_it`
- [ ] **Unit**: `replaces_the_stored_api_secret_when_an_update_carries_a_new_one`
- [ ] **Unit**: `asks_for_a_common_room_reconnect_when_the_livekit_url_changes`
- [ ] **Unit**: `asks_for_a_common_room_reconnect_when_the_room_name_changes`
- [ ] **Unit**: `leaves_the_common_room_alone_when_the_livekit_block_is_unchanged`
- [ ] **Unit**: `names_the_web_port_as_restart_required_when_it_changes`
- [ ] **Unit**: `names_the_web_host_as_restart_required_when_it_changes`
- [ ] **Unit**: `names_nothing_as_restart_required_when_only_livekit_changes`

### Common-room supervisor (`packages/tddy-daemon/src/common_room_supervisor.rs`, 16 cases)

The lifecycle the config service depends on, driven over a recording `CommonRoomConnector` — no
LiveKit server, no sleeps, every wait bounded around a real signal.

- [x] **Unit**: `joins_the_common_room_the_configuration_names_when_the_daemon_starts`
- [x] **Unit**: `replaces_the_running_connection_when_the_livekit_url_changes`
- [x] **Unit**: `rejoins_the_same_server_under_the_new_name_when_only_the_room_changes`
- [x] **Unit**: `leaves_the_daemon_disconnected_rather_than_on_the_room_it_was_told_to_leave` (block removed, no server, no room, no credentials)
- [x] **Unit**: `joins_nothing_when_the_daemon_starts_without_a_joinable_common_room`
- [x] **Unit**: `leaves_the_common_room_when_the_daemon_drops_its_supervisor`
- [x] **Unit**: `takes_the_four_connection_strings_from_a_complete_livekit_block`
- [x] **Unit**: `names_no_room_to_join_when_the_livekit_block_is_incomplete` (no block, no url, no api key, no api secret, no room, blank room)

### Webview connection lifecycle (`packages/tddy-tauri-rpc/tests/webview_connection_lifecycle.rs`, 2)

Support gains `a_sink_whose_peer_is_gone()` — a `FrameSink` that refuses every frame.

- [ ] **Integration**: `releases_the_connection_once_the_sink_reports_its_peer_is_gone`
- [ ] **Integration**: `serves_a_second_page_on_the_same_host_after_the_first_one_is_gone`

### Envelope engine internals (`packages/tddy-rpc-web/src/envelope-transport.test.ts`, 4 added)

- [ ] **Unit**: `mints a non-zero client epoch for the connection`
- [ ] **Unit**: `carries the caller-supplied client epoch on every request frame`
- [ ] **Unit**: `ignores a response frame whose request id no call holds`
- [ ] **Unit**: `settles every pending call when the pipe closes`

### Webview bridge handshake (`packages/tddy-tauri-web/src/transport.test.ts`, 2 added)

- [ ] **Unit**: `registers its response channel before sending the first request frame`
- [ ] **Unit**: `carries the epoch it registered with on every request frame`

### Settings form mapping (`packages/tddy-web/src/components/settings/settingsForm.test.ts`, 7)

`toFormState` / `toUpdateSettings` carry the two rules that would destroy a daemon's credentials if
they were wrong: a secret the daemon never returns must not come back as an empty string, and an
update replaces the whole message so every field goes back.

- [ ] **Unit**: `fills the form from the daemon's effective configuration`
- [ ] **Unit**: `marks the API secret as stored while leaving the secret field blank`
- [ ] **Unit**: `leaves the form blank for a daemon with no LiveKit block`
- [ ] **Unit**: `omits the API secret from an update when the field was left blank`
- [ ] **Unit**: `sends a newly typed API secret in the update`
- [ ] **Unit**: `carries every field an update replaces, not only the edited one`
- [ ] **Unit**: `sends the web port as a number`

`packages/tddy-web`'s `test:unit` glob gains `src/components/settings` so these run in CI.

## Technical Debt & Production Readiness

Track gaps and technical debt introduced during this changeset.
Items remaining when the changeset is wrapped will be transferred to `docs/dev/TODO.md` by `/wrap-context-docs`.

- [ ] The `src-tauri` IPC seam (real `tauri::ipc::Channel` + `invoke` → `FrameSink`) is not covered by automated tests — manual verification: `cargo tauri dev` on macOS and Linux, open a session terminal (server-streaming) and upload a file (client-streaming), confirm no listening port with `lsof -iTCP -sTCP:LISTEN -p <pid>`
- [ ] Releasing a departed page is deterministic only on a current-thread runtime — see the publish-before-accept decision below. Under a multi-thread runtime a frame can race through before the release lands; the frame after it is refused. Revisit if the flavour is ever driven from a multi-thread runtime where that matters
- [ ] Settings screen edits the LiveKit block only; the remaining `DaemonConfig` sections are read-only in the UI. Because `listen` renders read-only, nothing on the screen can yet *produce* a `listen.web_port` change — the daemon supports it and the component test covers the display path, but making the port editable is a follow-up (two inputs plus a test id)
- [ ] Runtime reconfiguration is implemented for LiveKit only; every other changed field is reported as restart-required rather than applied
- [ ] No auto-update, code signing, notarization or installer for either bundle (Electrobun's phase 5 polish item, carried forward)
- [ ] Windows bundle out of scope — the daemon's Unix-domain local socket and sandbox runner are unix-only
- [ ] `./install` and `publish.sh` still do not ship a desktop app (unchanged from State A)
- [ ] Peer discovery is assembled at startup and only when the daemon starts with a joinable common room, so a daemon that *gains* one at runtime serves its roster on the new room but does not discover the other daemons in it until restarted. The supervisor logs a warning naming exactly that; `TODO` at `packages/tddy-daemon/src/common_room_supervisor.rs` in `DaemonCommonRoomConnector::connect`
- [ ] **Cancelling a call is local only**: the client settles it and stops reading, but the peer is never told, so a server stream goes on being served until it ends by itself. `RpcRequest.abort` is the field for saying so and is hardcoded `false`. **Pre-existing** — the original LiveKit transport did the same (`HEAD:packages/tddy-livekit-web/src/transport.ts:570`) — but the extraction makes it shared by every flavour, and it matters more in the desktop app where terminal streams are the common case. `TODO` at `packages/tddy-rpc-web/src/envelope-transport.ts:292`
- [ ] The desktop host sends no Telegram "started"/"stopped" lifecycle messages, because the binary sends them from inside `server::run_server` and window creation must not block on a Telegram HTTP call. Sharing it needs that message moved out of the HTTP server. `TODO` at `packages/tddy-desktop/src-tauri/src/lib.rs:119`
- [ ] Writing the config back re-serializes it, which **loses the operator's YAML comments** (`dev.desktop.yaml` is heavily commented). `TODO` at `packages/tddy-daemon/src/daemon_config_service.rs:90-92`; preserving them needs a comment-aware YAML editor, i.e. a new dependency and a developer decision. Field values are safe: all three real config files (`dev.daemon.yaml`, `dev.desktop.yaml`, `config.example.yaml`) were verified to round-trip identically through the new `Serialize`
- [ ] `packages/tddy-livekit-web/cypress/component/transport.cy.tsx` — the suite that drives the LiveKit transport against a **real Rust echo server** over a real data channel — has not been run since the extraction. It needs a Docker LiveKit container plus `target/debug/examples/echo_server`, so it is the one piece of the regression net still outstanding; run it before this changeset wraps
- [ ] `packages/tddy-rpc-web/src/gen/` holds the envelope and echo-fixture types as generated output copied from `tddy-livekit-web`; `bun run --filter tddy-rpc-web generate` must be run once to prove it reproduces, and `tddy-livekit-web` should then import them from here rather than keeping its own copy
- [ ] **Found, out of scope, reverted**: `packages/tddy-web/src/gen/sandbox_pb.ts` is stale relative to `sandbox.proto` — it is missing `in_jail_tool_request` / `in_jail_tool_response` from the landed workspace-tool-sandbox changeset. Regenerating produced a 24-line diff unrelated to this changeset, so it was reverted; the fix is `bun run --filter tddy-web generate` in a changeset of its own

## Decisions & Trade-offs

- **Webview IPC over loopback HTTP**: chosen for no listening port and no HTTP hop, at the cost of a new transport on both sides. A custom URI-scheme handler was rejected outright: Tauri's responder returns one complete `http::Response`, so server-streaming RPCs — terminal IO, notifications, worktree stats — could not stream.
- **Two IPC commands carrying envelope frames, not one command per RPC**: makes the browser-side engine shareable with the LiveKit flavour instead of a second implementation, and keeps `client_epoch` stale-frame handling identical across flavours.
- **`FrameSink` trait so `tddy-tauri-rpc` never depends on `tauri`**: the flavour is fully testable with ordinary integration tests, and the untested surface shrinks to the thin adapter inside `src-tauri`.
- **Extract the envelope engine rather than duplicate it**: ~600 lines of epoch, correlation and settlement logic would otherwise exist twice; the refactor is protected by five existing bun suites plus a real-server Cypress suite. The alternative — copy into `tddy-tauri-web` — was rejected as the more expensive option over any horizon longer than this changeset.
- **LiveKit compiled in, configured at runtime**: follows from positioning the app as a daemon with a UI. Cost: the WebRTC dependency tree is in every desktop bundle whether or not a room is configured.
- **Runtime transport detection over a build-time flag**: one web bundle, one build, and the browser dashboard cannot silently diverge from the desktop one. Cost: `@tauri-apps/api` is present in the browser bundle (import-time side-effect-free, and the Tauri path is never taken without the bridge).
- **Bootstrap extracted for both hosts rather than a separate embedded path**: two bootstrap paths would drift, and roster parity is the property that makes "the desktop app is the daemon" true. Cost: a refactor of the file every daemon test path depends on, done as its own milestone with no behaviour change.
- **`bun:test` for the Tauri transport instead of Cypress**: both would drive the same in-memory IPC double, so Cypress would add a browser and a config for no additional coverage. Cypress is retained where a React mount is the subject.
- **`GetClientConfig` is deliberately ungated**, unlike every other method on `DaemonConfigService`. It is the payload that tells a page there *is* a daemon to sign in to, so it is read before any session token exists — a gate there makes a desktop webview unable to bootstrap at all. It is the same snapshot the daemon already serves unauthenticated at `GET /api/config` and carries no secrets (a LiveKit URL and room name, the agent allowlist, the debug mask, the instance id). The request keeps its `session_token` field so a signed-in caller may send one; it is not read. Pinned by `serves_the_client_config_to_a_caller_that_has_not_signed_in_yet`.
- **Publish-before-accept in `handle_request_frame`** (`packages/tddy-tauri-rpc/src/host.rs:103-107`): the host hands the runtime a turn before dispatching a call, so responses the engine has already produced reach the sink first. A failed publish is the *only* signal that the page is gone, so a host driven purely by inbound frames would keep dispatching calls for a page that left. Decided deliberately, with a known limit: this makes "the next frame is refused" deterministic on a current-thread runtime and best-effort on a multi-thread one, where the release stays eventual. The alternatives considered were making the sink's departure observable and having the test await it, or a bounded retry in the test; both were rejected in favour of leaving the approved test set untouched.
- **The common room is reconnected down-then-up, and an unjoinable LiveKit block is a disconnect**: the supervisor tears the running connection down *before* it brings anything up, and a block missing any of url / api_key / api_secret / common_room never becomes a `CommonRoomTarget` at all. So the failure mode of a reconfiguration that cannot be connected is a disconnected daemon, never one silently still in the old room while its settings screen reports the new one. Cost: a brief window with no common room during every reconnect, which is what a reconnect is.
- **The supervisor owns the discovery loop but not the OAuth loopback tunnel**: the tunnel supervisor follows the room slot rather than a room connection, so restarting it per reconnect would only race itself for its 127.0.0.1 callback ports. It is started once by `RuntimeTasks::spawn`; the discovery loop is aborted and restarted, and its room is `close()`d rather than dropped.
- **`CommonRoomConnector` as the seam, not a LiveKit test double**: the supervisor's lifecycle — replace, disconnect, refuse-and-stay-disconnected, leave on shutdown — is pinned by 16 in-module tests over a recording connector, with no LiveKit server and no sleeps. What is *not* covered is the real join inside `DaemonCommonRoomConnector`, which the Docker-backed common-room suites exercise.
- **Replace `packages/tddy-desktop` in place**: no dead second shell, no rename at the end; the working Electrobun app disappears at M6 rather than at the end of the changeset.

## References

- PRD: [PRD-2026-09-04-tauri-desktop-single-process.md](../../ft/desktop/1-WIP/PRD-2026-09-04-tauri-desktop-single-process.md)
- Design being replaced: [tddy-desktop-electrobun.md](../../ft/desktop/tddy-desktop-electrobun.md)
- Envelope protocol: `packages/tddy-rpc/proto/rpc_envelope.proto`
- Nearest flavour precedent: `packages/tddy-stdio/src/endpoint.rs`, `packages/tddy-stdio/tests/rpc_over_stdio.rs`
- Transport seam: `packages/tddy-web/src/rpc/transportProvider.tsx`
- Bootstrap to extract: `packages/tddy-daemon/src/main.rs:89-959`; HTTP wiring `packages/tddy-daemon/src/server.rs:20-82`
- Config model: `packages/tddy-daemon/src/config.rs:235`
- Test style: `.claude/skills/fluent-tests/references/generic-guidelines.md`, `references/rust/std-test.md`, `references/typescript/cypress-component.md`

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests — verify 100% pass
- [ ] Validate changes
- [ ] USER REVIEW — development complete
- [ ] Linting and type checking
- [ ] Wrap documentation
- [ ] USER REVIEW — work complete, decide next steps
