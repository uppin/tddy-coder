# 2026-09-05 — From 2026-09-05 Tauri desktop (single-process daemon)

**Category:** Future enhancement

Source changeset: the desktop app is `tddy-daemon` in one process. Features
[tddy-desktop-tauri.md](../../ft/desktop/tddy-desktop-tauri.md),
[daemon-settings.md](../../ft/daemon/daemon-settings.md).

**Correctness / behaviour**

- Writing the daemon config **loses the operator's YAML comments** (`dev.desktop.yaml` is heavily commented). Field values round-trip safely — verified against `dev.daemon.yaml`, `dev.desktop.yaml` and `config.example.yaml`. Preserving comments needs a comment-aware YAML editor, i.e. a new dependency. `TODO` at `packages/tddy-daemon/src/daemon_config_service.rs`.
- **Peer discovery does not follow a runtime common-room reconnect**: a daemon that *gains* a joinable room at runtime serves its roster there but does not discover the other daemons in it until restarted. `TODO` at `packages/tddy-daemon-livekit/src/common_room_supervisor.rs` in `DaemonCommonRoomConnector::connect` — moved there from `tddy-daemon` by `#unbundle` node 4 ([#473](https://github.com/uppin/tddy-coder/pull/473)). **The structural half of why it was never fixed is now gone.** The connector used to sit behind `common_room_supervisor → daemon_config_service → livekit_peer_discovery`, a cycle through the wiring layer; node 4 cut it by moving the `CommonRoomSupervisor` trait beside its one implementation, and the connector and the peer registry are now in one crate. The reconnect path is therefore expressible without reaching through `config`. Still not implemented.
- **Cancelling a call never reaches the peer**: the client settles locally and stops reading, but `RpcRequest.abort` is hardcoded `false`, so a server stream is served until it ends by itself. Pre-existing (the LiveKit transport did the same), but now shared by every flavour and most visible in the desktop app, where terminal streams are the common case. `TODO` at `packages/tddy-rpc-web/src/envelope-transport.ts`.
- The desktop host sends no Telegram **"started"/"stopped" lifecycle messages**; the binary sends them from inside `server::run_server`, and window creation must not block on a Telegram HTTP call. Sharing it needs that message moved out of the HTTP server. `TODO` at `packages/tddy-desktop/src-tauri/src/lib.rs`.
- Releasing a departed page is deterministic only on a current-thread runtime (the publish-before-accept `yield_now` in `packages/tddy-tauri-rpc/src/host.rs`). Under a multi-thread runtime a frame can race through before the release lands; the frame after it is refused.

**Verification still owed**

- **Linux is entirely unverified**: the `flake.nix` WebKitGTK inputs, the `deb`/`appimage` bundle targets, `cargo build --workspace` and CI on Linux.
- The macOS **`.dmg`** step fails locally — `bundle_dmg.sh` drives Finder over AppleScript and times out without Automation permission. The `.app` bundle itself is valid.
- `packages/tddy-livekit-web/cypress/component/transport.cy.tsx` (the transport against a **real Rust echo server**) has not been run since the envelope engine was extracted; it needs Docker plus `target/debug/examples/echo_server`.
- The daemon's Docker-dependent suites (`livekit_peer_daemons_acceptance`, `forward_to_peer_shared_registry`, `session_agent_remote_acceptance`, …) abort in harness setup without `/var/run/docker.sock`, so the **real** LiveKit join path inside `DaemonCommonRoomConnector` is unexercised; only the supervisor's lifecycle around it is proven. They do not *skip*, they **fail**: `LiveKitTestkit::start()` returns `Err` and every caller `.expect()`s it, so the gap is loud rather than silent. Four of them now live in `tddy-daemon-livekit` and contend with one another there — [2026-09-10-the-docker-backed-livekit-suites-contend-across-binaries.md](./2026-09-10-the-docker-backed-livekit-suites-contend-across-binaries.md).
- **Streaming RPCs over the webview IPC bridge** (a session terminal, a file upload) were not exercised end to end in the running app; the frame-level probe covered unary, error and malformed frames only.
- Window-close shutdown was exercised through the identical `RunEvent::Exit` path via `SIGTERM`, not by clicking the close button.

**Scope deliberately left open**

- The settings screen edits the **LiveKit block only**; everything else is read-only, so nothing in the UI can yet produce a `listen.web_port` change even though the daemon supports it and reports it as restart-required.
- Runtime reconfiguration is LiveKit-only; every other changed field is reported as restart-required.
- `./install` and `publish.sh` still do not ship a desktop app.
- `packages/tddy-web/src/gen/sandbox_pb.ts` is stale relative to `sandbox.proto` (missing `in_jail_tool_request`/`in_jail_tool_response` from the landed workspace-tool-sandbox work). Found while regenerating for this changeset and reverted to keep the diff scoped; the fix is `bun run --filter tddy-web generate`.
