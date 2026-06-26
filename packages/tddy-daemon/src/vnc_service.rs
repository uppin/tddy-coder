//! VNC control-plane service for tddy-daemon.
//!
//! Implements `VncService` from `vnc.proto` over the HTTP Connect transport.
//! Manages the encrypted vault per session, caches derived keys in memory, and
//! spawns/terminates `tddy-vnc` bridge processes on demand.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::vnc_vault::{vault_path, DerivedKey, VncVault};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::vnc::{
    AddVncTargetRequest, AddVncTargetResponse, ListVncTargetsRequest, ListVncTargetsResponse,
    RemoveVncTargetRequest, RemoveVncTargetResponse, StartVncStreamRequest, StartVncStreamResponse,
    StopVncStreamRequest, StopVncStreamResponse, UnlockVncVaultRequest, UnlockVncVaultResponse,
    VncService, VncTarget as ProtoVncTarget,
};

/// Per-session derived key cache: session_id → DerivedKey.
///
/// Populated by `UnlockVncVault`; read by `AddVncTarget` and `StartVncStream`.
pub type VncKeyCache = Arc<Mutex<HashMap<String, DerivedKey>>>;

/// Daemon-side implementation of `VncService`.
pub struct VncServiceImpl {
    /// Resolves a session token to its OS user login.
    user_resolver: Arc<dyn Fn(&str) -> Option<String> + Send + Sync>,
    /// Resolves an OS user to the base path of their sessions directory.
    sessions_base: Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>,
    /// In-memory cache of derived vault keys, keyed by session_id.
    key_cache: VncKeyCache,
    // FIXME: store daemon config reference here for bridge binary path resolution
    // when real bridge spawning is implemented.
}

impl VncServiceImpl {
    pub fn new(
        user_resolver: Arc<dyn Fn(&str) -> Option<String> + Send + Sync>,
        sessions_base: Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>,
        key_cache: VncKeyCache,
    ) -> Self {
        Self {
            user_resolver,
            sessions_base,
            key_cache,
        }
    }

    /// Resolve a session token to the session's directory.
    fn resolve_session_dir(
        &self,
        session_token: &str,
        session_id: &str,
    ) -> Result<PathBuf, Status> {
        let user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid session token"))?;
        let base = (self.sessions_base)(&user)
            .ok_or_else(|| Status::internal("sessions base not found for user"))?;
        Ok(base.join("sessions").join(session_id))
    }

    /// Retrieve the cached key for a session, returning `FailedPrecondition` if not found.
    async fn require_key(&self, session_id: &str) -> Result<DerivedKey, Status> {
        let cache = self.key_cache.lock().await;
        cache
            .get(session_id)
            .cloned()
            .ok_or_else(|| Status::failed_precondition("vault not unlocked"))
    }
}

fn vault_target_to_proto(t: &crate::vnc_vault::VncTarget) -> ProtoVncTarget {
    ProtoVncTarget {
        id: t.id.clone(),
        label: t.label.clone(),
        host: t.host.clone(),
        port: t.port as u32,
    }
}

#[async_trait]
impl VncService for VncServiceImpl {
    async fn list_vnc_targets(
        &self,
        request: Request<ListVncTargetsRequest>,
    ) -> Result<Response<ListVncTargetsResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.resolve_session_dir(&req.session_token, &req.session_id)?;
        let vault_file = vault_path(&session_dir);

        if !VncVault::exists(&vault_file) {
            return Ok(Response::new(ListVncTargetsResponse { targets: vec![] }));
        }

        // If the vault is unlocked, load it with the cached key to read non-secret metadata.
        // If the vault is locked, also load metadata — the target list is non-secret.
        let targets = {
            let cache = self.key_cache.lock().await;
            if let Some(key) = cache.get(&req.session_id).cloned() {
                drop(cache);
                let (vault, _) = VncVault::load_with_key(&vault_file, &key)
                    .map_err(|e| Status::internal(format!("failed to load vault: {}", e)))?;
                vault.list_targets()
            } else {
                drop(cache);
                // Vault is locked — still read target metadata (non-secret: id, label, host, port).
                let targets = VncVault::list_targets_from_file(&vault_file).map_err(|e| {
                    Status::internal(format!("failed to read vault metadata: {}", e))
                })?;
                targets
            }
        };

        let proto_targets: Vec<ProtoVncTarget> =
            targets.iter().map(vault_target_to_proto).collect();

        Ok(Response::new(ListVncTargetsResponse {
            targets: proto_targets,
        }))
    }

    async fn add_vnc_target(
        &self,
        request: Request<AddVncTargetRequest>,
    ) -> Result<Response<AddVncTargetResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.resolve_session_dir(&req.session_token, &req.session_id)?;
        let vault_file = vault_path(&session_dir);

        let key = self.require_key(&req.session_id).await?;

        let (mut vault, _) = VncVault::load_with_key(&vault_file, &key)
            .map_err(|e| Status::internal(format!("failed to load vault: {}", e)))?;

        let target = vault
            .add_target(&req.label, &req.host, req.port as u16, &req.password, &key)
            .map_err(|e| Status::internal(format!("failed to add target: {}", e)))?;

        Ok(Response::new(AddVncTargetResponse {
            target: Some(vault_target_to_proto(&target)),
        }))
    }

    async fn remove_vnc_target(
        &self,
        request: Request<RemoveVncTargetRequest>,
    ) -> Result<Response<RemoveVncTargetResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.resolve_session_dir(&req.session_token, &req.session_id)?;
        let vault_file = vault_path(&session_dir);

        let key = self.require_key(&req.session_id).await?;

        let (mut vault, _) = VncVault::load_with_key(&vault_file, &key)
            .map_err(|e| Status::internal(format!("failed to load vault: {}", e)))?;

        vault
            .remove_target(&req.target_id)
            .map_err(|e| Status::not_found(format!("target not found: {}", e)))?;

        Ok(Response::new(RemoveVncTargetResponse { ok: true }))
    }

    async fn unlock_vnc_vault(
        &self,
        request: Request<UnlockVncVaultRequest>,
    ) -> Result<Response<UnlockVncVaultResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.resolve_session_dir(&req.session_token, &req.session_id)?;
        let vault_file = vault_path(&session_dir);

        let key = if VncVault::exists(&vault_file) {
            // Vault exists — unlock it (validates passphrase).
            let (_vault, key) = VncVault::unlock(&vault_file, &req.passphrase)
                .map_err(|_| Status::unauthenticated("invalid passphrase"))?;
            key
        } else {
            // No vault yet — create it (treating the passphrase as the new vault passphrase).
            let (_vault, key) = VncVault::create(&vault_file, &req.passphrase)
                .map_err(|e| Status::internal(format!("failed to create vault: {}", e)))?;
            key
        };

        // Cache the derived key for this session.
        let mut key_cache = self.key_cache.lock().await;
        key_cache.insert(req.session_id, key);
        drop(key_cache);

        Ok(Response::new(UnlockVncVaultResponse { ok: true }))
    }

    async fn start_vnc_stream(
        &self,
        request: Request<StartVncStreamRequest>,
    ) -> Result<Response<StartVncStreamResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.resolve_session_dir(&req.session_token, &req.session_id)?;

        // Require vault to be unlocked.
        let _ = self.require_key(&req.session_id).await?;

        // Read session metadata to get the LiveKit room.
        let metadata = tddy_core::session_metadata::read_session_metadata(&session_dir)
            .map_err(|e| Status::internal(format!("failed to read session metadata: {}", e)))?;

        let livekit_room = metadata.livekit_room.unwrap_or_default();
        // FIXME: populate livekit_url from session metadata or daemon LiveKit config.
        let livekit_url = String::new();
        let bridge_identity = format!("vnc-{}-{}", req.session_id, req.target_id);
        let track_name = format!("vnc:{}", req.target_id);

        // FIXME: spawn the `tddy-vnc` bridge process here, passing the decrypted password
        // via stdin. Use `resolve_vnc_binary_path` from config to find the binary.

        Ok(Response::new(StartVncStreamResponse {
            livekit_room,
            livekit_url,
            bridge_identity,
            track_name,
            width: 1920,
            height: 1080,
        }))
    }

    async fn stop_vnc_stream(
        &self,
        request: Request<StopVncStreamRequest>,
    ) -> Result<Response<StopVncStreamResponse>, Status> {
        let _req = request.into_inner();
        // FIXME: kill the bridge process for this session+target.
        Ok(Response::new(StopVncStreamResponse { ok: true }))
    }
}
