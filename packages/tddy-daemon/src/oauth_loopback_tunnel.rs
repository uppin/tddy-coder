//! Operator-side OAuth callback: TCP on loopback bridged to session host via `LoopbackTunnelService.StreamBytes`.
//!
//! Replaces the desktop `Bun.listen` path when the daemon holds the common-room LiveKit [`Room`].
//! Spawns only when [`DaemonConfig::codex_oauth_loopback_proxy_eligible`] is true (YAML or
//! `TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE`); set false to avoid `127.0.0.1` port conflicts (e.g. 1455).

use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::{ParticipantIdentity, Room, RoomEvent};
use tokio::task::JoinHandle;

use crate::codex_oauth_participant_metadata::{
    parse_codex_oauth_metadata, resolved_codex_oauth_callback_port, CodexOAuthParticipantInfo,
};
use crate::tunnel_streambytes_bridge::run_tcp_accept_loop;

const LOG: &str = "tddy_daemon::oauth_tunnel";

#[derive(Debug, Clone, PartialEq, Eq)]
struct TunnelBinding {
    target_identity: String,
    listen_port: u16,
}

#[derive(Default)]
struct SupervisorState {
    listener: Option<JoinHandle<()>>,
    last_authorize: Option<String>,
    active_binding: Option<TunnelBinding>,
    /// Last identity we published to [`crate::tunnel_supervisor::TunnelSupervisor`] for RPC list.
    last_oauth_rpc_identity: Option<String>,
}

fn open_url_in_system_browser(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd").args(["/c", "start", "", url]).spawn();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}

fn pick_daemon_oauth_target(
    room: &Room,
) -> Option<(ParticipantIdentity, CodexOAuthParticipantInfo)> {
    for (_, p) in room.remote_participants() {
        let id_str = p.identity().to_string();
        if !id_str.starts_with("daemon-") {
            continue;
        }
        // Do not use `?` here: `None` means "no codex_oauth in metadata" — keep scanning other
        // `daemon-*` participants (e.g. idle sessions) instead of aborting the whole pick.
        let Some(info) = parse_codex_oauth_metadata(&p.metadata()) else {
            continue;
        };
        if !info.pending || info.authorize_url.is_none() {
            continue;
        }
        return Some((p.identity().clone(), info));
    }
    None
}

fn stop_listener(state: &mut SupervisorState) {
    if let Some(h) = state.listener.take() {
        h.abort();
    }
    state.active_binding = None;
}

fn refresh_listener_finished(state: &mut SupervisorState) {
    if let Some(h) = &state.listener {
        if h.is_finished() {
            state.listener = None;
            state.active_binding = None;
        }
    }
}

fn stop_tunnel(
    state: &mut SupervisorState,
    tunnel_supervisor: &Arc<crate::tunnel_supervisor::TunnelSupervisor>,
) {
    if let Some(id) = state.last_oauth_rpc_identity.take() {
        tunnel_supervisor.remove_advertisement(&id);
        log::debug!(
            target: LOG,
            "cleared tunnel RPC advertisement for session_correlation_id_len={}",
            id.len()
        );
    }
    stop_listener(state);
    state.last_authorize = None;
}

async fn scan_and_update(
    room: &Arc<Room>,
    state: &mut SupervisorState,
    tunnel_supervisor: &Arc<crate::tunnel_supervisor::TunnelSupervisor>,
) {
    refresh_listener_finished(state);
    let pick = pick_daemon_oauth_target(room.as_ref());
    let Some((target_identity, info)) = pick else {
        stop_tunnel(state, tunnel_supervisor);
        return;
    };

    let auth_url = match info.authorize_url.as_deref() {
        Some(u) if !u.is_empty() => u,
        _ => {
            stop_tunnel(state, tunnel_supervisor);
            return;
        }
    };

    if state.last_authorize.as_deref() != Some(auth_url) {
        open_url_in_system_browser(auth_url);
        state.last_authorize = Some(auth_url.to_string());
    }

    let listen_port = resolved_codex_oauth_callback_port(&info);
    let remote_loopback_port = listen_port;
    let binding = TunnelBinding {
        target_identity: target_identity.to_string(),
        listen_port,
    };

    let id = target_identity.to_string();
    if let Some(old) = state.last_oauth_rpc_identity.as_deref() {
        if old != id.as_str() {
            tunnel_supervisor.remove_advertisement(old);
        }
    }
    state.last_oauth_rpc_identity = Some(id.clone());
    tunnel_supervisor.ingest_pending_codex_oauth(&id, u32::from(listen_port), Some(auth_url));

    if state.active_binding.as_ref() != Some(&binding) {
        stop_listener(state);
    }

    if state.listener.is_none() {
        let room = room.clone();
        let tid = target_identity.clone();
        let lp = listen_port;
        let rp = remote_loopback_port;
        state.active_binding = Some(binding);
        state.listener = Some(tokio::spawn(async move {
            if let Err(e) = run_tcp_accept_loop(room, tid, lp, rp).await {
                log::error!(
                    target: LOG,
                    "OAuth loopback TCP bind/listen failed on 127.0.0.1:{}: {}",
                    lp,
                    e
                );
            }
        }));
    }
}

/// Polls [`room_slot`] until a common-room [`Room`] handle exists, then runs
/// [`run_oauth_tunnel_supervisor`]. Repeats after disconnect so OAuth works across discovery reconnects.
pub async fn run_oauth_tunnel_supervisor_follow_room_slot(
    room_slot: Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    tunnel_supervisor: Arc<crate::tunnel_supervisor::TunnelSupervisor>,
) {
    log::info!(
        target: LOG,
        "OAuth tunnel follower: waiting for LiveKit common-room connection (loopback TCP starts after connect + pending codex_oauth metadata)"
    );
    loop {
        let room = {
            let g = room_slot.read().await;
            g.clone()
        };
        if let Some(r) = room {
            log::info!(
                target: LOG,
                "OAuth tunnel follower: room handle ready, starting supervisor"
            );
            run_oauth_tunnel_supervisor(r, tunnel_supervisor.clone()).await;
            log::info!(
                target: LOG,
                "OAuth tunnel follower: supervisor ended; waiting for next room handle"
            );
        } else {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

/// Watches the common-room [`Room`] for `daemon-*` participants publishing pending Codex OAuth metadata,
/// opens the system browser, and accepts loopback TCP for tunneling to the session host.
pub async fn run_oauth_tunnel_supervisor(
    room: Arc<Room>,
    tunnel_supervisor: Arc<crate::tunnel_supervisor::TunnelSupervisor>,
) {
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(400));
    let mut events = room.subscribe();
    let mut state = SupervisorState::default();

    log::info!(
        target: LOG,
        "OAuth tunnel supervisor started (common-room LiveKit connection)"
    );

    loop {
        tokio::select! {
            _ = tick.tick() => {
                scan_and_update(&room, &mut state, &tunnel_supervisor).await;
            }
            ev = events.recv() => {
                match ev {
                    Some(RoomEvent::ParticipantConnected(_))
                    | Some(RoomEvent::ParticipantDisconnected(_)) => {
                        scan_and_update(&room, &mut state, &tunnel_supervisor).await;
                    }
                    Some(RoomEvent::Disconnected { .. }) | None => {
                        stop_tunnel(&mut state, &tunnel_supervisor);
                        log::info!(target: LOG, "OAuth tunnel supervisor stopping (room disconnected)");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors `pick_daemon_oauth_target` scan order for unit tests (no LiveKit `Room`).
    fn pick_oauth_scan(participants: &[(&str, &str)]) -> Option<CodexOAuthParticipantInfo> {
        for (id, meta) in participants {
            if !id.starts_with("daemon-") {
                continue;
            }
            let Some(info) = parse_codex_oauth_metadata(meta) else {
                continue;
            };
            if !info.pending || info.authorize_url.is_none() {
                continue;
            }
            return Some(info);
        }
        None
    }

    #[test]
    fn pick_skips_daemon_without_codex_then_finds_pending() {
        let list = [
            (
                "daemon-d74f5268-a73e-4c75-8fc6-b8bec0522cde",
                r#"{"owned_project_count":0}"#,
            ),
            (
                "daemon-019d7d74-3a7f-7b03-88d2-f50bb7efb2f0",
                r#"{"codex_oauth":{"pending":true,"authorize_url":"https://auth.example.com/o","callback_port":1455}}"#,
            ),
        ];
        let got = pick_oauth_scan(&list).expect("pending session after skipping idle daemon-*");
        assert!(got.pending);
        assert_eq!(
            got.authorize_url.as_deref(),
            Some("https://auth.example.com/o")
        );
    }

    #[test]
    fn tunnel_binding_eq_for_restart_detection() {
        let a = TunnelBinding {
            target_identity: "daemon-x-s1".to_string(),
            listen_port: 1455,
        };
        let b = TunnelBinding {
            target_identity: "daemon-x-s1".to_string(),
            listen_port: 1455,
        };
        assert_eq!(a, b);
    }
}
