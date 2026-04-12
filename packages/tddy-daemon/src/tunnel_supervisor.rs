//! Generalized tunnel advertisement and binding state shared by OAuth loopback and Connect RPC.

use std::collections::HashMap;
use std::sync::Mutex;

use tddy_service::proto::tunnel_management::{TunnelBindingState, TunnelKind};

const LOG: &str = "tddy_daemon::tunnel_supervisor";

/// Snapshot row for RPC list responses (mirrors proto fields; avoids coupling callers to prost types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelAdvertisementSnapshot {
    pub operator_loopback_port: u32,
    pub session_correlation_id: String,
    pub kind: i32,
    pub state: i32,
    pub authorize_url: String,
}

/// Port policy alignment with session host (`open_port >= 1024`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelPortValidationError {
    pub operator_bind_port: u32,
}

impl std::fmt::Display for TunnelPortValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "operator_bind_port {} is not allowed (minimum 1024)",
            self.operator_bind_port
        )
    }
}

impl std::error::Error for TunnelPortValidationError {}

#[derive(Debug, Default)]
struct TunnelSupervisorState {
    rows: HashMap<String, TunnelAdvertisementSnapshot>,
}

/// Owns pending/active tunnel advertisements for the operator daemon (shared with OAuth path).
#[derive(Debug)]
pub struct TunnelSupervisor {
    inner: Mutex<TunnelSupervisorState>,
}

impl Default for TunnelSupervisor {
    fn default() -> Self {
        Self {
            inner: Mutex::new(TunnelSupervisorState::default()),
        }
    }
}

impl TunnelSupervisor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enforces operator bind policy (>= 1024).
    pub fn validate_operator_bind_port(
        &self,
        operator_bind_port: u32,
    ) -> Result<(), TunnelPortValidationError> {
        log::debug!(
            target: LOG,
            "validate_operator_bind_port port={}",
            operator_bind_port
        );
        if operator_bind_port < 1024 {
            return Err(TunnelPortValidationError { operator_bind_port });
        }
        Ok(())
    }

    /// Ingest metadata for a pending Codex OAuth tunnel row (overwrites same `session_correlation_id`).
    pub fn ingest_pending_codex_oauth(
        &self,
        session_correlation_id: &str,
        operator_loopback_port: u32,
        authorize_url: Option<&str>,
    ) {
        let url = authorize_url.unwrap_or("").to_string();
        let mut g = self.inner.lock().expect("tunnel supervisor mutex poisoned");
        let is_new = !g.rows.contains_key(session_correlation_id);
        if is_new {
            log::info!(
                target: LOG,
                "ingest_pending_codex_oauth new session_correlation_id_len={} port={} authorize_url_len={}",
                session_correlation_id.len(),
                operator_loopback_port,
                url.len()
            );
        } else {
            log::debug!(
                target: LOG,
                "ingest_pending_codex_oauth refresh session_correlation_id_len={} port={}",
                session_correlation_id.len(),
                operator_loopback_port
            );
        }
        g.rows.insert(
            session_correlation_id.to_string(),
            TunnelAdvertisementSnapshot {
                operator_loopback_port,
                session_correlation_id: session_correlation_id.to_string(),
                kind: TunnelKind::CodexOauth as i32,
                state: TunnelBindingState::Pending as i32,
                authorize_url: url,
            },
        );
    }

    /// Upsert a row as ACTIVE after a validated StartTunnel (or refresh).
    pub fn upsert_active_binding(
        &self,
        session_correlation_id: &str,
        operator_loopback_port: u32,
        kind: i32,
    ) {
        log::info!(
            target: LOG,
            "upsert_active_binding session_correlation_id_len={} port={} kind={}",
            session_correlation_id.len(),
            operator_loopback_port,
            kind
        );
        let mut g = self.inner.lock().expect("tunnel supervisor mutex poisoned");
        let authorize_url = g
            .rows
            .get(session_correlation_id)
            .map(|r| r.authorize_url.clone())
            .unwrap_or_default();
        g.rows.insert(
            session_correlation_id.to_string(),
            TunnelAdvertisementSnapshot {
                operator_loopback_port,
                session_correlation_id: session_correlation_id.to_string(),
                kind,
                state: TunnelBindingState::Active as i32,
                authorize_url,
            },
        );
    }

    pub fn remove_advertisement(&self, session_correlation_id: &str) {
        log::debug!(
            target: LOG,
            "remove_advertisement session_correlation_id_len={}",
            session_correlation_id.len()
        );
        let mut g = self.inner.lock().expect("tunnel supervisor mutex poisoned");
        g.rows.remove(session_correlation_id);
    }

    /// Current advertisements for `ListTunnelAdvertisements` (stable sort by session id).
    pub fn snapshot_advertisements(&self) -> Vec<TunnelAdvertisementSnapshot> {
        let g = self.inner.lock().expect("tunnel supervisor mutex poisoned");
        let mut out: Vec<_> = g.rows.values().cloned().collect();
        out.sort_by(|a, b| a.session_correlation_id.cmp(&b.session_correlation_id));
        log::debug!(target: LOG, "snapshot_advertisements count={}", out.len());
        out
    }

    /// Stop binding for a session (idempotent: unknown session still yields IDLE).
    pub fn stop_binding(&self, session_correlation_id: &str) -> i32 {
        log::info!(
            target: LOG,
            "stop_binding session_correlation_id_len={}",
            session_correlation_id.len()
        );
        let mut g = self.inner.lock().expect("tunnel supervisor mutex poisoned");
        g.rows.remove(session_correlation_id);
        TunnelBindingState::Idle as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_service::proto::tunnel_management::TunnelKind;

    #[test]
    fn tunnel_supervisor_validate_operator_bind_port_rejects_below_1024() {
        let sup = TunnelSupervisor::new();
        let err = sup
            .validate_operator_bind_port(80)
            .expect_err("privileged operator_bind_port must be rejected");
        assert_eq!(err.operator_bind_port, 80);
    }

    #[test]
    fn tunnel_supervisor_snapshot_lists_pending_after_codex_oauth_ingest() {
        let sup = TunnelSupervisor::new();
        sup.ingest_pending_codex_oauth("unit-session-pending-1", 9876, None);
        let ads = sup.snapshot_advertisements();
        assert_eq!(ads.len(), 1, "expected one PENDING row after ingest");
        let a = &ads[0];
        assert_eq!(a.operator_loopback_port, 9876);
        assert_eq!(a.session_correlation_id, "unit-session-pending-1");
        assert_eq!(a.kind, TunnelKind::CodexOauth as i32);
        assert_eq!(a.state, TunnelBindingState::Pending as i32);
    }

    #[test]
    fn tunnel_supervisor_stop_binding_reports_idle() {
        let sup = TunnelSupervisor::new();
        sup.ingest_pending_codex_oauth("unit-stop-1", 5000, None);
        let state = sup.stop_binding("unit-stop-1");
        assert_eq!(
            state,
            TunnelBindingState::Idle as i32,
            "StopTunnel must surface IDLE when binding ends"
        );
    }
}
