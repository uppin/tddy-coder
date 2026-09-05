# 2026-06-26 — **Screen sharing

**Type:** Feature

generalize VNC → ScreenSharing + RDP support** — renamed `vnc_service.rs` → `screen_sharing_service.rs` (`ScreenSharingServiceImpl`, protocol-aware vault lookup, `start_stream` dispatches to VNC or RDP bridge binary, `build_bridge_spawn_config` extracted, magic-constant named); renamed `vnc_vault.rs` → `screen_sharing_vault.rs` (`ScreenSharingVault`, domain `ScreenSharingTarget.{protocol,username}`, `.screen-sharing.yaml` vault file, `create_dir_all` parent dir on first write); `config.rs`: `ScreenSharingConfig` (`vnc_binary_path` + `rdp_binary_path`), `resolve_binary_path_for_protocol`; `main.rs`: `ScreenSharingServiceServer` mounted; `lib.rs`: `screen_sharing_service` + `screen_sharing_vault` modules; username threaded from proto → vault → service → bridge config; 5 service acceptance tests + 5 vault acceptance tests. Feature [screen-sharing-sessions.md](../../../../docs/ft/web/screen-sharing-sessions.md). (tddy-daemon)
