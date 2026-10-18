# Initial Discovery — sign-in-form (2026-10-18-host-desktops-sign-in-form)

Planning evidence for [`2026-10-18-host-desktops-sign-in-form.md`](./2026-10-18-host-desktops-sign-in-form.md) (`#host-desktops`). Exploration 1 is the stack's whole-work discovery, copied verbatim; node-specific passes follow. Deleted at wrap with the changeset.

Deleted at wrap together with the changeset.

## Combined Conclusions

1. **Host-scoped desktop streaming is built end to end in code** — proto, daemon store
   (`host-desktop-targets.json`), start/stop, password prompt over the host's encrypted prompt
   channel, bridge spawn (JSON on stdin), LiveKit common-room track, browser overlay with input.
2. **It is not reachable in the running app.** `HostsScreen.tsx` renders only `HostRowTelemetry`;
   `HostRowTooling` (which mounts `HostRowRemoteDesktop` → Connect → `HostDesktopOverlay`) is mounted
   by nothing in `src/`, only by Cypress specs. Nothing on the Hosts screen calls `GetHostTooling`
   (only production caller: `ProjectsAppPage.tsx:179`).
3. **Credentials are a single prompted password.** No auth-method discovery anywhere; probe is a bare
   TCP connect by documented design ("A connect, and not one byte more"). Host flow creates targets
   with `username: ""`; `HostPassphraseDialog` has one secret field; no domain field anywhere.
   - `tddy-vnc` (vnc-rs 0.5.3 @ `ab684d0`): security types None (1) and VncAuth (2) only; VeNCrypt
     unsupported; an advertised type outside vnc-rs's enum (e.g. Apple 30/35) aborts parsing of the
     whole list. Username ignored.
   - `tddy-rdp` (IronRDP): TLS-only, `enable_credssp: false`, `domain: None` → NLA-requiring servers
     (Windows default) refuse.
4. **One fixed port per protocol.** Probe: VNC 5900 / RDP 3389 on the daemon's own `127.0.0.1`.
   Overlay hard-wired to `127.0.0.1:<probed port>`. `connectableDesktop` returns the first match only
   → one desktop per row. No `RemoveHostTarget`. The store, bridge keys, bridge identities and track
   names already support many targets per host.
5. **No browser Fullscreen API on the viewer.** `ScreenSharingOverlay` is `fixed inset-0 z-50`.
   `src/lib/browserFullscreen.ts` exists (terminal-named) and is used only by terminal surfaces.
6. **Latent bug — pointer scaling.** `StartHostStream` / `StartStream` return hard-coded
   `1920x1080`; the overlay scales pointer coordinates into those, while the bridge publishes the
   real framebuffer size. Any other resolution mis-lands clicks.
7. **Latent bug — auth failure is silent.** `spawn_bridge` returns once stdin is written; a wrong
   password fails inside the bridge, is only logged to inherited stderr; the browser waits forever
   on a track that never arrives.
8. **No Storybook stories** for hosts / remote desktop / overlay.

## Exploration 1 — Explore agent (very thorough)

### Sequence and patterns

1. `ls packages`; `grep -rliE 'ScreenSharing|HostDesktop|remote_desktop|RemoteDesktop' --include='*.proto'` → 3 protos.
2. `grep -rlE 'host-desktop-targets'` across repo.
3. `find … | xargs wc -l` for tddy-screen-sharing, tddy-screenshare, tddy-vnc, tddy-rdp, tddy-host-service, tddy-livekit-screen-capture, tddy-desktop.
4. Outline `screen_sharing_service.rs` via `grep -nE 'fn |struct|Host|prompt|stdin|Command::new|#\[tokio::test'`, then range reads.
5. Wiring: `grep -rnE 'with_host_scope|FileHostDesktopTargetStore|ScreenSharingServiceImpl::new|SCREEN_SHARE_HOST_IDENTITY_PREFIX|ScreenSharingInputService' --include='*.rs'`.
6. Probe: `remote_desktop_probe.rs`; `grep -nE 'remote_desktop|DesktopProtocol|default_port'` in host_tooling.rs, service.rs, host_messages.rs.
7. Bridges: tddy-vnc, tddy-rdp, tddy-screenshare; vnc-rs checkout `~/.cargo/git/checkouts/vnc-rs-725d7b745b354239/ab684d0/src/client/{auth.rs,connector.rs}`.
8. Web: `grep -rlE 'ScreenSharing|startHostStream|HostDesktop|remoteDesktop|DESKTOP_PASSWORD|requestFullscreen|Ctrl\+Alt\+Esc' src cypress docs`; `grep -rn 'HostRowTooling|getHostTooling'`.
9. Cypress titles `grep -nE '^\s*(describe|it)\('`; stories `find -name '*.stories.tsx' | xargs grep -il 'host|desktop|vnc|overlay'`.
10. Docs/backlog: `grep -rliE 'screen.?shar|vnc|rdp|host.?desktop|remote.?desktop|fullscreen|full.screen|5901'` over docs/dev/1-WIP, docs/dev/todo, every `1-WIP`.

### Protos

`packages/tddy-service/proto/screen_sharing.proto` (178 lines)
- L11-31 `service ScreenSharingService`: session `ListTargets/AddTarget/RemoveTarget/StartStream/StopStream`; host:
  ```
  rpc ListHostTargets(ListHostTargetsRequest) returns (ListHostTargetsResponse);
  rpc AddHostTarget(AddHostTargetRequest) returns (AddHostTargetResponse);
  rpc StartHostStream(StartHostStreamRequest) returns (StartStreamResponse);
  rpc StopHostStream(StopHostStreamRequest) returns (StopHostStreamResponse);
  ```
  L24-26: "…There is no browser-side VNC/RDP client and there must never be one." **No `RemoveHostTarget`.**
- L35-39 `enum Protocol { PROTOCOL_UNSPECIFIED=0; PROTOCOL_VNC=1; PROTOCOL_RDP=2 }`.
- L43-50 `ScreenSharingTarget {id, label, host, port, protocol, username}` — "Login user (required for RDP, optional for VNC)". No domain / auth-method.
- L113-120 `StartStreamResponse {livekit_room, livekit_url, bridge_identity, track_name, width, height}`.
- L144-152 `AddHostTargetRequest {session_token, daemon_instance_id, label, host, port, protocol, username}` — no password.
- L157-169 `StartHostStreamRequest {session_token, daemon_instance_id, target_id}` — "The desktop password is deliberately NOT a field here…".

`screen_sharing_input.proto` (32 lines): `rpc StreamInput (stream ScreenSharingInputEvent) returns (stream ScreenSharingInputAck)`; pointer `{x, y, button_mask}`, key `{keysym, pressed}`; no wheel/clipboard/resize.

`host.proto` (433 lines)
- L13-85 `HostService`: `GetHostTooling` (25), `StreamHostPrompts` (36), `AnswerHostPrompt` (40), `AddHostKey`, …
- L177-184 `GetHostToolingResponse`: `repeated HostRemoteDesktop remote_desktop = 5; // One entry per probed protocol (VNC, RDP).`
- L258-271 `HostPromptEvent {prompt_id, daemon_instance_id, kind, subject, host_public_key, host_public_key_fingerprint, expires_at_unix_ms}`.
- L274-282 `enum HostPromptKind { UNSPECIFIED=0; SSH_KEY_PASSPHRASE=1; DESKTOP_PASSWORD=2 }`.
- L289-305 `HostRemoteDesktop`:
  ```
  ProbeOutcome outcome = 1;
  int32 protocol = 2;
  bool can_bridge = 3;
  bool desktop_reachable = 4;  // bare TCP connect, never a handshake
  uint32 port = 5;             // "Port discovery is out of scope."
  string failure_reason = 6;
  ```
- L140-153 `AnswerHostPromptRequest {session_token, daemon_instance_id, prompt_id, bytes encrypted_answer}` (RSA-OAEP SHA-256).

Legacy `vnc.proto` / `vnc_input.proto`: no server registered (TODO 2026-09-10).

### Daemon side

`packages/tddy-host-service/src/host_desktop_targets.rs` (283 lines)
- L33-42 `HostDesktopTarget { target_id, label, host, port: u16, protocol: i32, username }` ("Field names are the persisted JSON keys").
- L54-64 `trait HostDesktopTargetStore { fn list(..); fn add(..) -> Result<String,String>; }` — no remove/update.
- L70-76 `TargetsFile { hosts: BTreeMap<String, Vec<HostDesktopTarget>> }`; L90 `host-desktop-targets.json`; atomic write under mutex; unparseable file is an error.
- Tests: `a_target_added_to_one_host_is_not_visible_on_another`, `each_host_lists_only_its_own_targets`, `a_targets_file_that_cannot_be_parsed_is_reported_rather_than_read_as_no_desktops`.
- Wiring `packages/tddy-daemon/src/runtime.rs:1575-1597`:
  ```
  tddy_screen_sharing::ScreenSharingServiceImpl::new(ss_user_resolver, ss_sessions_base)
      .with_config(Arc::clone(&config_arc))
      .with_host_scope(ss_host_targets, ss_host_keypair, Arc::clone(&host_prompts))
      .with_credential_vaults(auth_result.credential_vaults.clone());
  ```

`packages/tddy-screen-sharing/src/screen_sharing_service.rs` (2250 lines; 967 production)
- L38-40 `DEFAULT_STREAM_WIDTH = 1920; DEFAULT_STREAM_HEIGHT = 1080; DEFAULT_STREAM_FPS = 30`.
- L50-65 `BridgeSpawnConfig {host, port, username, password, livekit_url, livekit_token, livekit_room, livekit_identity, track_name, width, height, target_id, fps}`.
- L97-109 `HostScope { targets, keypair, prompts }`.
- L266-299 `prompt_for_desktop_password` → `prompts.issue(operator, PromptKind::DesktopPassword, subject, now)` → `answer_before_expiry` → `decrypt_desktop_password`; `deadline_exceeded` on no answer. "Every start asks."
- L346-415 `spawn_bridge`: `terminate_bridge` first; `Command::new(&binary).stdin(piped).stdout(null).stderr(inherit)`; writes JSON, closes stdin; returns once spawned.
- L437-467 `prepare_bridge`; L477-528 `build_bridge_spawn_config` (needs livekit url/key/secret).
- Keys: `session_bridge_key` `"{session}:{target}"` (531), `host_bridge_key` `"host:{instance}:{target}"` (539), identity `"screenshare-host-{instance}-{target}"` (547-554), track `screenshare:{target_id}` (556).
- L585-597 `host_livekit_room` = `livekit.common_room` or `failed_precondition`.
- L604-606 `desktop_prompt_subject` = `"{label} ({host}:{port})"`.
- L680-696 `list_host_targets`; L698-734 `add_host_target` (rejects port > 65535); L738-803 `start_host_stream` (lookup → room/identity/track → `prepare_bridge` → prompt → `spawn_bridge` → reply with DEFAULT width/height); L805-820 `stop_host_stream`.
- Browser routes to the host's own daemon via `useHostClient(ScreenSharingService, hostId)` (`packages/tddy-web/src/rpc/connections/registry.tsx:233-241`).

`packages/tddy-host-service/src/host_prompts.rs`: `PROMPT_TTL = 120s`; 7 prompt tests.

`packages/tddy-host-service/src/remote_desktop_probe.rs` (329 lines)
- L29-31 `DEFAULT_VNC_PORT = 5900; DEFAULT_RDP_PORT = 3389` — "Port discovery is out of scope".
- L37 `CONNECT_TIMEOUT = 750ms`; L82-100 connect-and-close, "no bytes are ever written"; L104 `PROBE_HOST = "127.0.0.1"`; L128-134 `can_bridge` = binary resolves.
- `host_tooling.rs:202` `const PROBED_PROTOCOLS: [DesktopProtocol; 2] = [Vnc, Rdp];` and L370 `probe.probe(protocol, protocol.default_port())`.
- Mapping `host_messages.rs:230-243`; `service.rs:504-507`.
- Config `packages/tddy-daemon-kernel/src/config.rs:616-660` `ScreenSharingConfig { vnc_binary_path, rdp_binary_path }`.
- Tests: `reports_a_port_with_a_listener_as_reachable`, `reports_a_port_with_no_listener_as_unreachable`, `writes_no_bytes_to_the_remote_before_closing`, `reports_that_the_host_cannot_bridge_when_the_binary_is_missing`, `reports_that_the_host_can_bridge_when_the_configured_binary_exists`, `names_the_default_port_for_each_protocol`.

### Bridges

`packages/tddy-screenshare`
- `src/bridge.rs:30-62` `BridgeConfig` from stdin JSON; `username` `#[serde(default)]`.
- `run_bridge` (129-200): `C::connect(host, port, username, password)` → `wait_for_dimensions` → join LiveKit, serve `ScreenSharingInputService` on data channel, publish at **actual** size → pump until SIGTERM.
- `src/client.rs:6-50` `trait ScreenSharingClient { async fn connect(host, port, username: Option<&str>, password: Option<&str>) -> Result<Self>; … }`.
- `src/streamer.rs:134-141` refuses frames with a different size (no resize).

`tddy-vnc`: `src/vnc_client.rs:22-42` `VncConnector::new(tcp).set_auth_method(async move { Ok(password) })`; L279-287 `connect(host, port, _username, password)` — username ignored. vnc-rs `connector.rs:52-135` selects only None/VncAuth else `"Security type apart from Vnc Auth has not been implemented"`; `auth.rs:75-77` `read_u8().try_into()?` fails on unknown types. Tests (`tests/vnc_client_integration.rs`, fake server advertises only None): `frames_stream_with_incrementing_qr_content`, `framebuffer_dimensions_match_server_init`, `inject_pointer_is_recorded_by_server`, `inject_key_is_recorded_by_server`, `stop_closes_connection_cleanly`.

`tddy-rdp` `src/rdp_client.rs`: L31-41 `NoNetworkClient` ("no CredSSP is performed in TLS-only mode"); L64-109:
```
credentials: Credentials::UsernamePassword { username: username.unwrap_or("").into(), password: password.unwrap_or("").into() },
enable_tls: true,
enable_credssp: false,
domain: None,
```
TLS cert not verified. Tests (`tests/rdp_client_integration.rs`, ironrdp-server `with_no_client_auth`): `streams_qr_coded_frames_with_incrementing_content`, `framebuffer_dimensions_match_server_desktop_size`, `pointer_event_is_received_by_server`, `keyboard_press_is_received_by_server`, `closes_connection_cleanly_on_stop`.

No way exists to query advertised auth methods without starting a stream.

### tddy-web

`packages/tddy-web/src/components/hosts/`
- `HostsAppPage.tsx` (70): one `listKnownHosts`, renders `<HostsScreen rows nowUnixMs>`.
- `HostsScreen.tsx` (119): columns Host/Status/Last seen/Telemetry/Instance ID/Repos base path; L108 `<HostRowTelemetry instanceId online />` only.
- `HostRowTooling.tsx` (170): props `{instanceId, git, githubCli, sshAgent?, remoteDesktop?}`; L167 `<HostRowRemoteDesktop instanceId readings={remoteDesktop ?? []} />`; fetches nothing.
- `HostRowRemoteDesktop.tsx` (135): L44-53 `desktopFact` ("Desktop on :<port>" …); L70-81 `connectableDesktop(readings)` → first OK+reachable+canBridge; L86-87 `useHostConnection` + `useHasCapability(connection, "media")`; L114-123 Connect button `hosts-row-${instanceId}-connect-desktop`; L125-132 mounts `HostDesktopOverlay`.
- `HostDesktopOverlay.tsx` (298): L55 `PROBED_HOST = "127.0.0.1"`; L71-95 `targetForProbedEndpoint` (list then `addHostTarget({…, username: ""})`); L114-160 start on mount / stop on unmount; L169 `useCommonRoom`; L174-201 latches first `DESKTOP_PASSWORD` prompt; L208-212 `checkHostKey`; L215-232 `answerHostPrompt`; L242-265 `<HostPassphraseDialog … allowEmpty …>`; L266-289 `<ScreenSharingOverlay room bridgeIdentity trackName width height onClose/>`; portalled to `document.body`.
- `HostPassphraseDialog.tsx` (295): single secret field; `encryptForHost` RSA-OAEP; key-continuity checks.

Overlay / input / fullscreen
- `src/components/sessions/ScreenSharingOverlay.tsx` (130): L84 `fixed inset-0 z-50 …`; finds track by identity+name; chord hint "Ctrl+Alt+Esc to close"; L114 `<video className="max-w-full max-h-full">`; input-unavailable notice. No `requestFullscreen`.
- `useScreenSharingInput.ts` (372): L89-91 closing chord; listeners on `document`; scaling via `framebufferPointFor` into `framebufferWidth/Height` (the RPC's 1920x1080).
- `src/lib/browserFullscreen.ts` (110): `requestFullscreenForConnectedTerminal`, `exitDocumentFullscreen`, `isTargetInActiveFullscreen`; used by `ConnectionTerminalChrome.tsx:202`, `SessionRuntime.tsx:295,330`.
- Session scope `SessionScreenSharingTab.tsx`: Add form with protocol selector, username (`sessions-screen-sharing-add-username`), password.

Capability gating: `useHasCapability(connection, "media")`; `docs/capability-gating.md:134` lists the Hosts-row connect action.

Cypress specs (component): `HostDesktopConnectAcceptance.cy.tsx` (connect gating, password prompt, empty answer, session token, close, prompt latching, portal placement), `HostsScreenRemoteDesktopAcceptance.cy.tsx` (4 reachability tests), `ScreenSharingInputScopesAcceptance.cy.tsx`, `ScreenSharingInputForwardingAcceptance.cy.tsx` (pointer, coordinates, keyboard, chord, teardown, stream lifetime, input unavailable), `HostsScreenAcceptance.cy.tsx` (7), `HostsScreen.cy.tsx` (2), `HostsScreenToolingAcceptance.cy.tsx` (mounts `HostRowTooling` directly), `SessionTerminalFullscreenAcceptance.cy.tsx` (fullscreen precedent).

Helpers: `cypress/support/rpc/inMemory.tsx:39` `mountWithRpc`; `anInMemoryRpcBackend()` (`packages/tddy-connectrpc-testkit/src/backend.ts:276`); `withSelectedDaemon` (`cypress/support/rpc/withSelectedDaemon.tsx:86`); `aHostConnection(hostId).reachedOverLiveKit().servingOver(transport).build()` (`hostConnections.ts:94`); `aHostPromptFeed` (`hostPromptFeed.ts`); `anRsaOaepKeypair` (`cypress/support/hostKeys.ts`); `aBridgeRecordingInput()` / `aBridgeRefusingInput()` (`screenSharingInputBridge.ts`); page objects `hostPassphraseDialogPage` (184), `hostRemoteDesktopPage` (241), `hostDesktopPage` (249) in `cypress/support/pages/hostsScreenPage.ts`; `desktopPage` in `screenSharingOverlayPage.ts:55`.

No Storybook stories for hosts / remote desktop / overlay.

### Docs

- `packages/tddy-web/docs/hosts-screen.md`: L110-111 only default ports; L178-183 "⚠ Nothing mounts `HostRowTooling`."
- `packages/tddy-web/docs/remote-desktop-input.md`: "Not forwarded: Scroll wheel, audio, clipboard, file transfer, multi-monitor, touch and pen".
- `docs/ft/web/hosts-screen-tooling.md`: L51-53 always VNC :5900 + RDP :3389; L130-136 no port discovery, bare TCP connect; L214-224 "Not yet reachable in the running app".
- `docs/ft/web/screen-sharing-sessions.md`: AC-SS-1..16; Current limits; Out of scope: multi-monitor, NLA/Kerberos, non-default ports. "Full-screen overlay" means viewport overlay.
- `packages/tddy-host-service/docs/host-registry.md` § Host-scoped desktop targets / Starting and stopping / Password prompted, never stored.
- `packages/tddy-host-service/docs/host-tooling-probe.md` § Remote desktop: "A connect, and not one byte more".
- Wrapped changesets: `2026-09-07-host-desktop-connect.md`, `2026-09-06-desktop-probe.md`, `2026-09-09-remote-desktop-input-forwarding.md`.

### WIP / TODO

- No 1-WIP changesets on this area. `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-integration.md` lists `tddy-screen-sharing` as a `RoomTokenMinter` consumer.
- TODOs: `2026-09-06-no-code-path-issues-gethosttooling.md` (Hosts screen does not mount `HostRowTooling`), `2026-09-06-tooling-probe-has-no-cache-or-dedup.md` ("Worth resolving before it does"), `2026-09-07-remote-desktop-input-forwarding.md` (resolved; scroll wheel open), `2026-09-10-vnc-proto-has-no-server-and-a-live-web-client.md`, `2026-08-15-livekit-connect-streamer-duplication-still-outstanding.md`.
- Code issues: `packages/tddy-screen-sharing/docs/code-issues/oversized-file-screen-sharing-service.md` (967/500, unclaimed), `packages/tddy-host-service/docs/code-issues/oversized-file-host-tooling.md` (775/500, unclaimed). No `Claimed by:` in tddy-host-service / tddy-screen-sharing / tddy-web.

### Rust tests on the host path

`screen_sharing_service.rs` inline tests (from L963): `starting_a_host_desktop_returns_the_room_identity_and_track_the_viewer_needs`, `stopping_a_host_desktop_releases_the_bridge_process`, `opening_a_host_desktop_asks_the_operator_for_its_password`, `the_password_the_operator_typed_reaches_the_bridge_on_stdin_and_never_its_arguments`, `the_password_the_operator_typed_is_never_written_to_disk`, `a_desktop_password_nobody_answers_fails_the_start_rather_than_starting_without_it`, `starting_a_session_desktop_still_works_unchanged`, `a_desktop_this_host_cannot_bridge_is_refused_before_anybody_is_asked_for_its_password`, `a_bridge_that_cannot_be_spawned_fails_the_start_rather_than_reporting_a_stream`, `reopening_a_host_desktop_releases_the_bridge_the_previous_start_left_running`, `a_hosts_desktops_and_a_sessions_desktops_stay_out_of_each_others_lists`, plus 6 auth/port refusal tests. Stand-in shell bridge records argv + stdin (L1047-1105); `AnOperatorAtTheKeyboard` prompt registry (L1125-1232).

## Interview record (2026-10-18)

| Topic | Developer's answer |
|---|---|
| Auth-method discovery | **On-demand handshake** — the periodic probe stays byte-free; a new RPC reads the RFB security list / RDP X.224 negotiation when the operator opens a desktop, and the credential form is shaped from it |
| Auth methods end to end | **VNC None + VncAuth, VNC VeNCrypt (username+password), RDP NLA (CredSSP, username+domain+password), Apple ARD (type 30)** |
| How tddy-vnc gets VeNCrypt / ARD | **Own RFB security layer** in tddy-vnc (version + security negotiation), handing the authenticated stream to vnc-rs from ServerInit — or patching vnc-rs if its API does not allow the hand-off. New crates likely (DH bignum, aes, md-5) — ask before adding |
| Multiple channels | **X11 enumeration, daemon starts x11vnc per display** — only displays the daemon's OS user can open; x11vnc bound to 127.0.0.1 on a free port, `-localhost -nopw`, lives as long as the viewer, killed on close |
| Non-X11 hosts | **Keep the default-port probe too** (VNC 5900, RDP 3389), de-duplicated by port against X11 channels — macOS Screen Sharing and RDP keep working |
| Remembering credentials | **"Remember" checkbox** stores the credentials in the operator's credential vault (screen-sharing records already exist there); unchecked = prompted and kept nowhere, as today |
| Full screen | **Fullscreen API + Keyboard Lock** (`navigator.keyboard.lock()` where supported); Ctrl+Alt+Esc still closes |
| In scope | **Mount the remote-desktop section on the Hosts screen** (with the tooling-probe cache/dedup TODO as prerequisite), **fix the hard-coded 1920x1080 pointer scaling**, **report bridge auth failures** distinctly so the viewer can re-ask |
| Out of scope | Several desktops open at once (one viewer at a time) |
| Shape | **A PR stack via `/plan-pr-stack`**, not a single `/plan-red` PR |
