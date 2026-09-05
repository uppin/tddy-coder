# Tddy Desktop on Tauri — single-process daemon with a UI — PRD

**Date**: 2026-09-04
**PRD Type**: Architecture Change

## Affected Features

**CRITICAL**: List ALL feature documents affected by this PRD:

- **Primary Feature**: [Tddy desktop app (Electrobun)](../tddy-desktop-electrobun.md) — Electrobun is dropped entirely. The Bun main process, the spawned-daemon child, `electrobun.config.ts` and the `resources/bin` binary copy are replaced by a Tauri (Rust) application that **is** the daemon. The document's Actors, High-level architecture, layout and Electrobun-specifics sections are rewritten; its OAuth relay sections survive unchanged in substance because the relay already lives in `tddy-daemon`.
- **Local web development**: [local-web-dev.md](../../web/local-web-dev.md) — `./web-dev` (browser + separate daemon) stays as-is; a new desktop dev loop (`bun run --filter tddy-desktop dev` → `cargo tauri dev`) is documented beside it. The web bundle gains a runtime transport switch, so the Vite dev server path is unchanged.
- **Codex OAuth web relay**: [codex-oauth-web-relay.md](../../web/codex-oauth-web-relay.md) — unchanged in behaviour; the operator-side loopback TCP and the browser-open remain `tddy-daemon`'s, and the daemon now happens to run inside the desktop process. The three test-only TS modules that referenced it in the shell (`livekit-oauth-relay.ts`, `oauth-callback-server.ts`, `codex-oauth-metadata.ts`) are deleted with Electrobun.
- **Codex OAuth relay (daemon)**: [codex-oauth-relay.md](../../daemon/codex-oauth-relay.md) — unchanged; noted because it names `tddy-desktop` as the process that spawns the daemon.
- **New capability — daemon settings**: no feature document exists for daemon configuration today. This PRD introduces one (`docs/ft/daemon/daemon-settings.md`, created during wrap): a `DaemonConfigService` that reads and writes the daemon's YAML, and a settings screen in `tddy-web` that drives it.
- **New capability — webview IPC RPC flavour**: the transport-agnostic RPC framework has no feature document (it is described per-transport in package docs). This PRD adds a fourth flavour beside `tddy-connectrpc` (HTTP), `tddy-livekit` (data channel) and `tddy-stdio` (pipes).

## Summary

Replace the Electrobun desktop shell with a Tauri application whose Rust main process hosts `tddy-daemon` in-process — one process instead of two — serves the `tddy-web` bundle through Tauri's asset protocol, and carries RPC between webview and daemon over a new **webview-IPC flavour of `tddy-rpc`** rather than a loopback HTTP port. LiveKit stays compiled in and is configured at runtime, because the desktop app is positioned as *tddy-daemon with a UI*: a new `DaemonConfigService` plus a settings screen in `tddy-web` read and write the daemon's own configuration.

## Background

Today `packages/tddy-desktop` is an Electrobun app: a Bun main process that opens a webview onto the `tddy-web` bundle and **spawns `target/release/tddy-daemon` as a child process** (`src/bun/embedded-daemon.ts`), then waits for the daemon's HTTP port before the UI can talk to it (`src/bun/wait-for-daemon-rpc.ts`). Three consequences motivate this change:

1. **Two processes, two lifecycles.** The shell must resolve a daemon binary from four candidate paths, prepare a config file, load the repo `.env`, spawn, wait for HTTP, and tear the child down on exit. A crashed or orphaned daemon leaves a bound port; a mismatched binary version is invisible until an RPC fails.
2. **A TCP port is the IPC.** All UI↔daemon traffic goes over `127.0.0.1:<web_port>` (`8899` in `dev.desktop.yaml`), reachable by any local process, and the daemon's Connect-RPC surface is authenticated for a multi-user server rather than for a single desktop user.
3. **The shell is Bun/TypeScript while everything it orchestrates is Rust.** Binary resolution, config preparation and process supervision are re-implemented in TypeScript against Rust behaviour they cannot type-check.

The RPC framework already anticipates this: `packages/tddy-rpc/proto/rpc_envelope.proto` describes itself as being for "a byte-oriented transport (LiveKit data channel, stdio pipes, ...)", and three flavours already exist. A webview-IPC flavour is the missing one, and Tauri's IPC (`invoke` for client→host frames, `tauri::ipc::Channel` for host→client frames) is a byte-oriented duplex channel of exactly the shape the envelope was designed for.

## Proposed Changes

### What's Changing

#### Desktop shell: Electrobun → Tauri (`packages/tddy-desktop`)

- `electrobun.config.ts`, `src/bun/**`, `test/acceptance/**`, `test/e2e/**`, `scripts/electrobun-dev.ts`, `resources/bin/`, and the `electrobun` dependency are **deleted**. The package is replaced in place: `src-tauri/` (Rust crate, workspace member), `tauri.conf.json`, and a `package.json` whose `dev`/`build` scripts drive `cargo tauri`.
- The Tauri main process calls the daemon's new library entry point, so **the daemon runs on the app's own Tokio runtime** — no child process, no binary resolution, no port wait, no teardown race.
- The webview loads the `tddy-web` bundle through Tauri's asset protocol (production) or `VITE_URL` (dev), keeping the existing dev-server workflow.
- `scripts/desktop-dev.sh` and `dev.desktop.yaml` are retained; the script drives `cargo tauri dev` and the YAML remains the desktop daemon's config source.
- **Platform targets: macOS and Linux.** Windows is out of scope (the daemon's Unix-domain local socket and sandbox runner are unix-only).

#### New RPC flavour: webview IPC (`packages/tddy-tauri-rpc`, new crate)

- Hosts an `RpcBridge` over a **frame-oriented duplex channel**, mirroring what `tddy-stdio` does for pipes: inbound `Request` frames are dispatched to the daemon's `MultiRpcService`, outbound `Response` frames go back over the same channel.
- The crate depends on **`tddy-rpc` only, not on `tauri`**: the channel is abstracted behind a `FrameSink` trait, so the whole flavour is exercised by ordinary Rust integration tests with a fake sink, and the Tauri app supplies the real implementation over `tauri::ipc::Channel`.
- The Tauri app exposes exactly **two IPC commands**: `tddy_rpc_connect(channel)` registers the webview's response channel once per page load, and `tddy_rpc_send(frame)` carries one request frame. This is deliberately the same one-duplex-channel model the LiveKit flavour uses, which is what allows the browser-side engine to be shared rather than rewritten.
- No frame-size chunking: unlike a LiveKit data channel, Tauri IPC carries whole payloads.

#### Browser-side transport (`packages/tddy-rpc-web` new, `packages/tddy-tauri-web` new, `packages/tddy-livekit-web` refactored)

- The envelope engine inside `tddy-livekit-web`'s 1067-line `transport.ts` — request-id allocation, client-epoch stale-frame rejection, call-metadata mismatch rejection, pending-call settlement, `AsyncQueue` streaming — is **extracted into `tddy-rpc-web`** behind a byte-pipe interface (`send(frame)` / `onFrame(cb)`).
- `tddy-livekit-web` keeps its LiveKit-specific parts (room registry, data-channel topic, chunking, reconnect) and consumes the extracted core. Its existing five test files and Cypress component suite are the safety net for that refactor.
- `tddy-tauri-web` is new and small: `createTauriTransport()` wires the extracted core to `invoke("tddy_rpc_send")` and a `Channel` from `@tauri-apps/api`.

#### Transport selection (`packages/tddy-web`)

- `createDefaultHttpTransport` is joined by a runtime choice in `RpcTransportProvider`: when the Tauri IPC bridge is present on `window`, the provider builds the Tauri transport; otherwise it builds today's `/rpc` Connect transport. **One web bundle serves both the desktop app and the browser dashboard.**
- `/api/config` has no HTTP origin inside the desktop app, so the same `ClientConfig` payload is served as an RPC (`DaemonConfigService.GetClientConfig`), with the existing `fetch("/api/config")` retained for the browser path.

#### Daemon bootstrap extraction (`packages/tddy-daemon`)

- The ~850-line body of `main()` (config load, 15+ `ServiceEntry` registrations, LiveKit common-room connect, spawn worker, Telegram, UDS server, signal handling) moves into a new `tddy_daemon::runtime` module with a library entry point that returns the assembled services and lifecycle handles. `main.rs` becomes a thin wrapper: build the runtime, serve HTTP.
- `RuntimeOptions` distinguishes the two hosts: the binary keeps systemd socket activation and HTTP serving; the embedded/desktop runtime skips both.
- **The service roster must be identical for both hosts** — that is what makes "the desktop app is the daemon" true rather than approximately true, and it is pinned by a test.

#### Daemon settings (`packages/tddy-service`, `packages/tddy-daemon`, `packages/tddy-web`)

- New `daemon_config.proto`: `GetConfig` (effective config, **secrets redacted**), `UpdateConfig` (validate → write back to the YAML file the daemon was loaded from → apply what can be applied live, report the rest as restart-required), and `GetClientConfig` (the `/api/config` payload over RPC).
- LiveKit becomes **runtime-reconfigurable**: a supervisor around the common-room connection disconnects and reconnects when the LiveKit block changes. Fields that cannot be applied live (listen port, users, web bundle path) are reported in the response rather than silently ignored.
- New settings screen in `tddy-web` reads `GetConfig`, edits the LiveKit block, saves via `UpdateConfig`, and surfaces both validation errors and restart-required fields.

#### Toolchain and infrastructure

- `flake.nix`: Linux `buildInputs` gain the Tauri/WebKitGTK set (`webkitgtk_4_1`, `libsoup_3`, `gtk3`, `glib-networking`, `librsvg`, `cairo`, `pango`, `gdk-pixbuf`, `atk`); `packages` gains `cargo-tauri`. macOS needs no additions (system WKWebView).
- Root `Cargo.toml` gains `packages/tddy-desktop/src-tauri` and `packages/tddy-tauri-rpc` as workspace members. **Consequence for CI**: `cargo build --workspace` and `cargo clippy --workspace --all-targets` on `ubuntu-latest` now require those system libraries, which is why the flake change is a hard prerequisite rather than a convenience.
- Root `package.json`: `workspaces` gains `tddy-rpc-web` and `tddy-tauri-web`; the `test` script's `--filter tddy-desktop test` leg is replaced by the new packages' tests.
- `./release`, `./install` and `publish.sh` are **not** changed by this PRD — they never shipped the desktop app, and the Tauri bundle is produced by `cargo tauri build`.

### What's Staying the Same

- **The daemon's behaviour, service roster and wire protocol.** No proto is changed; one is added. A daemon started from the CLI and a daemon inside the desktop app serve the same services.
- **The Codex OAuth relay.** Operator-side loopback TCP, common-room metadata scan and `LoopbackTunnelService.StreamBytes` stay in `tddy-daemon` exactly as [tddy-desktop-electrobun.md](../tddy-desktop-electrobun.md) § *LiveKit: OAuth metadata and loopback tunnel* describes.
- **The browser dashboard.** `./web-dev`, the `--headless` daemon install, remote and multi-host access from a browser all keep working, over the same `/rpc` Connect transport and the same single web bundle.
- **The LiveKit transport** for remote hosts: inside the desktop app, a session on another daemon is still reached over LiveKit, unchanged.
- **`tddy-coder` remains a separate agent process** spawned by the daemon. This PRD does not bundle it.
- **The YAML file is still the configuration source of truth.** The settings screen edits that file; it does not introduce a second store.

## Impact Analysis

### Technical Impact

**Code changes required (per affected feature)**

| Area | Change |
|---|---|
| `packages/tddy-desktop` | Electrobun deleted; `src-tauri/` Rust app added (window, asset protocol, two IPC commands, embedded runtime, external-link handling) |
| `packages/tddy-tauri-rpc` (new) | `RpcBridge` over a `FrameSink`; request dispatch, response routing, per-connection epoch, abort handling |
| `packages/tddy-rpc-web` (new) | Envelope engine extracted from `tddy-livekit-web` |
| `packages/tddy-tauri-web` (new) | `createTauriTransport()` over `invoke` + `Channel` |
| `packages/tddy-livekit-web` | Refactored onto the extracted core; behaviour unchanged |
| `packages/tddy-web` | Runtime transport selection; `/api/config` via RPC in the desktop path; settings screen and route |
| `packages/tddy-daemon` | `runtime` module extracted from `main.rs`; `DaemonConfigService`; runtime LiveKit reconfiguration |
| `packages/tddy-service` | `daemon_config.proto` + generated code |
| Repo root | `flake.nix`, `Cargo.toml`, `package.json`, `scripts/desktop-dev.sh` |

**Dependencies affected**

- **Added (Rust)**: `tauri` 2.x + `tauri-build` + `tauri-plugin-opener` (external links) in the desktop crate only. `cargo-tauri` in the dev shell. — *requires developer consent per AGENTS.md § Judgment Boundaries (ASK before adding external dependencies).*
- **Added (npm)**: `@tauri-apps/api` and `@tauri-apps/cli`. Both are **present on the private registry** (`npm.dev.wixpress.com`): `api` 2.11.1 (96 versions), `cli` 2.11.4 (171 versions), including the platform binaries the CLI needs (`cli-darwin-arm64`, `cli-linux-x64-gnu`, both 2.11.4). All 956 currently-locked packages also resolve there at their exact locked versions.
- **Removed (npm)**: `electrobun`.

**Performance implications**

- One fewer process, no HTTP/TCP hop, no port-wait on startup: UI-visible startup latency drops by the daemon's HTTP-readiness wait.
- Per-call overhead drops from a loopback TCP round trip plus Connect framing to an in-process IPC frame plus envelope encode/decode.
- Tauri's IPC is a serialization boundary: request and response bodies must be carried as raw bytes (`Vec<u8>` / `ArrayBuffer`), not JSON-encoded, or large payloads (file uploads, terminal bursts) regress badly.

**Memory impact**

- LiveKit/WebRTC remains compiled in, so the desktop bundle carries it whether or not a room is configured. This is a deliberate consequence of positioning the app as the daemon with a settings screen rather than a lean shell.

**Wire format implications**

- None on any existing protocol. `rpc_envelope.proto` is reused unchanged, including `client_epoch` — which matters here for the same reason it matters in the browser: a page reload restarts request ids while the host may still be streaming for the previous page.

### User Impact

**UX changes**

- The desktop app becomes self-contained: launch it and it *is* the daemon. No separate terminal, no port, no "waiting for daemon".
- Daemon configuration becomes editable in the UI (LiveKit first), with explicit feedback about which changes take effect immediately and which need a restart.
- Linux users get a desktop bundle for the first time.

**Workflow modifications**

- `bun run --filter tddy-desktop dev` now needs a Rust toolchain and the WebKitGTK libraries on Linux (both supplied by `./dev`).
- `./web-dev` and the browser dashboard workflow are unchanged.

**Breaking changes**

- The Electrobun app and every `TDDY_DESKTOP_*` / `TDDY_DAEMON_BINARY` path it honoured are gone. Anyone depending on the Electrobun bundle must switch to the Tauri bundle.

**Migration requirements**

- None for data or config: `dev.desktop.yaml` / `TDDY_DAEMON_CONFIG` continue to be read by the same `DaemonConfig` parser.

## Implementation Plan

The three deliverables here — the Tauri migration, the new RPC flavour, and the config service plus settings screen — are each substantial, and the honest assessment is that this is large for one changeset. The plan therefore sequences them so each phase is independently green and reviewable, and so that a phase boundary is a usable PR boundary if the work is split later.

1. **Toolchain first.** Extend `flake.nix` (Linux WebKitGTK set, `cargo-tauri`) and confirm `@tauri-apps/*` resolve on the private registry. Nothing else can build until `./dev cargo build --workspace` works on Linux with a Tauri crate present.
2. **Extract the daemon runtime.** `tddy_daemon::runtime` + thin `main.rs`, with the roster-parity test. No behaviour change; the existing daemon test suite is the regression net.
3. **Build the webview-IPC flavour, host side.** `packages/tddy-tauri-rpc` against a fake `FrameSink` — unary, all three streaming kinds, concurrency, abort, unknown service, stale epoch. No Tauri dependency, so this is fast to iterate.
4. **Extract the browser-side envelope engine.** `packages/tddy-rpc-web`, with `tddy-livekit-web` refactored onto it and its existing suites unchanged and passing.
5. **Build the webview-IPC flavour, browser side.** `packages/tddy-tauri-web` + `createTauriTransport()`, Cypress component tests against an in-memory IPC double.
6. **Replace the shell.** `src-tauri/` app: window, asset protocol, the two IPC commands over the real `Channel`, embedded runtime, external links. Delete Electrobun in the same step so the tree never carries two shells.
7. **Wire transport selection in `tddy-web`** and serve the client config over RPC.
8. **Add the config service** (`daemon_config.proto`, `DaemonConfigService`, runtime LiveKit reconfiguration) and the **settings screen**.
9. **Documentation**: rewrite `tddy-desktop-electrobun.md` as the Tauri design, add `docs/ft/daemon/daemon-settings.md`, update `local-web-dev.md`, changelogs and the changeset index.

**Testing approach** — per affected feature, at the level the user chose (Rust integration + Cypress component; `tauri-driver` cannot drive macOS, so no WebDriver e2e):

- Webview-IPC flavour, host side: Rust integration tests over a fake frame sink.
- Daemon runtime + config service: Rust integration tests, including a temp-file YAML round trip.
- Browser-side transport: Cypress component tests over an in-memory IPC double.
- Transport selection: `bun:test` unit tests on the provider.
- Settings screen: Cypress component tests with `anInMemoryRpcBackend`.

**Rollout**

- No staged rollout: the desktop app is opt-in and unreleased (`private: true`, never shipped by `./install` or `publish.sh`). The browser dashboard is unaffected, which is what keeps the blast radius to the desktop app.

## Acceptance Criteria

- [ ] The desktop app runs the daemon in its **own process** — no child `tddy-daemon`, no binary resolution, no HTTP readiness wait ([tddy-desktop-electrobun.md](../tddy-desktop-electrobun.md))
- [ ] The desktop app serves UI↔daemon RPC over webview IPC with **no listening TCP port** ([tddy-desktop-electrobun.md](../tddy-desktop-electrobun.md))
- [ ] Unary, server-streaming, client-streaming and bidirectional calls all work over the webview-IPC flavour
- [ ] Electrobun is gone from the tree: no `electrobun` dependency, no `src/bun/**`, no `resources/bin` daemon copy ([tddy-desktop-electrobun.md](../tddy-desktop-electrobun.md))
- [ ] The embedded runtime and the `tddy-daemon` binary expose the **same service roster**
- [ ] The same `tddy-web` bundle runs in the desktop app (Tauri transport) and in a browser against a standalone daemon (HTTP transport) ([local-web-dev.md](../../web/local-web-dev.md))
- [ ] `tddy-livekit-web` behaviour is unchanged after the envelope-engine extraction — its existing suites pass untouched
- [ ] LiveKit connects when configured and is left disconnected when not, decided at **runtime** from the YAML
- [ ] A settings screen reads the effective daemon config with secrets redacted and writes edits back to the YAML the daemon was loaded from (new `docs/ft/daemon/daemon-settings.md`)
- [ ] A config change that cannot be applied live is reported as restart-required rather than silently dropped
- [ ] An invalid config update is rejected without writing the file
- [ ] `cargo tauri build` produces a bundle on **macOS and Linux**
- [ ] `./dev cargo clippy --workspace --all-targets -- -D warnings` is clean on Linux with the Tauri crates in the workspace
- [ ] Tests passing for all affected features

## References

### Affected Features (Complete List)

- [Tddy desktop app (Electrobun)](../tddy-desktop-electrobun.md) — rewritten as the Tauri design; Electrobun main process, embedded-daemon spawn and build/copy sections removed
- [Local web development](../../web/local-web-dev.md) — desktop dev loop replaced with `cargo tauri dev`; browser path unchanged
- [Codex OAuth web relay](../../web/codex-oauth-web-relay.md) — shell-side test-only relay modules deleted; relay behaviour unchanged
- [Codex OAuth relay (daemon)](../../daemon/codex-oauth-relay.md) — references to the spawning desktop process updated
- `docs/ft/daemon/daemon-settings.md` (new) — `DaemonConfigService` and the settings screen

### Related Documentation

- [Web terminal / LiveKit](../../web/web-terminal.md) — the LiveKit transport the extraction must not regress
- [Continuous integration](../../dev/guides/ci.md) — the Linux-only CI matrix that the flake change unblocks
- [Testing practices](../../dev/guides/testing.md)
- Current shell: `packages/tddy-desktop/src/bun/index.ts`, `embedded-daemon.ts`, `wait-for-daemon-rpc.ts`
- Envelope protocol: `packages/tddy-rpc/proto/rpc_envelope.proto`; existing flavours `packages/tddy-connectrpc`, `packages/tddy-livekit`, `packages/tddy-stdio`
- Transport seam: `packages/tddy-web/src/rpc/transportProvider.tsx`
- Daemon bootstrap to extract: `packages/tddy-daemon/src/main.rs:89-959`; HTTP wiring `packages/tddy-daemon/src/server.rs:20-82`
