//! The session-bound half of `build`: everything that needs a way to resolve a session token to a
//! login — the resolvers, the host, the families served above it and the local socket.
//!
//! TODO(restructure-engine): `runtime::build` was decomposed by hand around `BuildContext` (D1);
//! every body below is a verbatim slice of the old function, bound to its old local names by the
//! prologue of the method that holds it.

use super::build_context::BuildContext;
use super::*;
use tddy_daemon_auth::SessionTokens;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};
use tddy_daemon_livekit::common_room_supervisor::PeerDiscoveryHandles;
use tddy_daemon_livekit::livekit_peer_discovery::{AdvertisedSigningKey, LiveKitDiscoveryHandles};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::remote_git_service::ProjectsDirResolver;

/// What every service mounted below is built from.
pub(super) struct Mount {
    pub(super) config_arc: Arc<DaemonConfig>,
    pub(super) session_tokens: SessionTokens,
    pub(super) livekit_discovery: Option<LiveKitDiscoveryHandles>,
    pub(super) user_resolver: SessionUserResolver,
    pub(super) sessions_base_resolver: SessionsBaseResolver,
    pub(super) projects_dir_resolver: ProjectsDirResolver,
    pub(super) bsp_session_resolver: tddy_bsp::bsp_service::SessionPathsResolver,
    pub(super) chat_workspace_roots: tddy_model_registry::ChatWorkspaceRoots,
}

/// The services shared by the host and the entries mounted beside it.
pub(super) struct Shared {
    pub(super) model_registry: Arc<tddy_model_registry::ModelRegistryStore>,
    pub(super) session_notification_bus:
        Arc<tddy_session_lifecycle::session_notifications::SessionNotificationBus>,
    pub(super) host_prompts: Arc<dyn tddy_session_lifecycle::host_prompts::HostPromptRegistry>,
}

/// The two services a host serves and the local socket re-serves.
pub(super) struct Served {
    pub(super) host_service_impl: Arc<tddy_host_service::HostServiceImpl>,
    pub(super) worktree_service_impl: Arc<tddy_worktree_service::WorktreeServiceImpl>,
}

/// What mounting the session host produced.
pub(super) struct Hosted {
    pub(super) connection_arc: Arc<DaemonSessionHost>,
    pub(super) rpc_handlers: tddy_daemon_rpc::RpcHandlers,
    pub(super) task_registry: tddy_task::TaskRegistry,
    pub(super) session_admissions:
        Arc<tddy_daemon_livekit::session_admission_service::SessionAdmissionRegistry>,
}

/// Everything the entries below are mounted from.
pub(super) struct Mounted {
    pub(super) mount: Mount,
    pub(super) shared: Shared,
    pub(super) served: Served,
    pub(super) hosted: Hosted,
    pub(super) livekit_service: Arc<tddy_daemon_livekit::livekit_service::LiveKitServiceImpl>,
}

impl BuildContext {
    /// Mount the services that need a session identity; a daemon with no way to authenticate
    /// mounts none of them.
    pub(super) async fn mount_session_services(&mut self) -> anyhow::Result<()> {
        let identity = &mut self.identity;
        let (Some(user_resolver), Some(session_tokens), Some(advertised_signing_key)) = (
            identity.user_resolver.take(),
            identity.session_tokens.take(),
            identity.advertised_signing_key.take(),
        ) else {
            return Ok(());
        };
        let config_arc = Arc::new(self.config.clone());
        self.record_own_host();
        let livekit_discovery = self.join_peer_discovery(&config_arc, advertised_signing_key);
        let mut mount = self.open_mount(config_arc, user_resolver, session_tokens);
        mount.livekit_discovery = livekit_discovery;
        let shared = self.open_shared().await?;
        let served = self.open_served(&mount, &shared);
        self.manage_index_daemon(&mount.config_arc);
        let askpass_endpoint = self.bind_askpass(&shared, &served).await?;
        let connection_impl = self.build_session_host(&mut mount, &shared, askpass_endpoint);
        let hosted = self.install_session_host(connection_impl);
        self.register_code_intelligence(&hosted.task_registry);
        let livekit_service = livekit_service(&mount);
        let mounted = Mounted {
            mount,
            shared,
            served,
            hosted,
            livekit_service,
        };
        self.mount_local_socket(&mounted);
        self.mount_session_entries(&mounted);
        self.mount_family_entries(&mounted);
        self.mount_host_entries(&mounted);
        self.mount_model_entries(&mounted);
        self.mount_remote_entries(&mounted);
        self.mount_action_entries(&mounted);
        self.mount_vm_entry(&mounted);
        self.mount_screen_sharing_entry(&mounted);
        self.mount_accounts_entry(&mounted);
        Ok(())
    }

    pub(super) fn record_own_host(&self) {
        let host_registry = &self.identity.keys.host_registry;
        let config = &self.config;
        // The local row is the daemon's own, and nothing else records it: the peer registry
        // deliberately holds only *remote* participants, and a daemon with LiveKit switched off
        // never syncs a room at all. Recorded at startup so `first_seen` for this machine means
        // "since tddy first ran here" rather than "since a browser first asked".
        if let Err(e) = host_registry.record_sighting(
            &tddy_session_lifecycle::host_registry::local_host_sighting(config),
            tddy_session_lifecycle::host_registry::now_unix_ms(),
        ) {
            log::warn!("host registry: could not record this daemon's own entry: {e}");
        }
    }

    pub(super) fn join_peer_discovery(
        &mut self,
        config_arc: &Arc<DaemonConfig>,
        advertised_signing_key: AdvertisedSigningKey,
    ) -> Option<LiveKitDiscoveryHandles> {
        // Peer discovery over the common room. The registry and the room slot are the handles the
        // roster is built from; the task that fills them is the host's to start.
        match self.identity.keys.common_room_peers.take() {
            Some((target, registry)) => {
                let room_slot = Arc::new(tokio::sync::RwLock::new(None));
                log::info!(
                    "LiveKit common-room peer discovery configured (room {:?})",
                    target.room()
                );
                self.peer_discovery = Some(PeerDiscoveryHandles {
                    registry: registry.clone(),
                    room_slot: room_slot.clone(),
                    // Announced on every connection to the common room, so a peer holding one of
                    // this daemon's tokens can find the key that verifies it.
                    signing_key: advertised_signing_key,
                    status: self.plugins.livekit_status.clone(),
                });
                self.tasks.oauth_loopback_tunnel = Some(OauthLoopbackTunnel {
                    config: config_arc.clone(),
                    room_slot: room_slot.clone(),
                });
                self.credential_sync_room = Some((registry.clone(), room_slot.clone()));
                Some(tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles {
                    eligible_daemon_source: Arc::new(
                        tddy_daemon_livekit::livekit_peer_discovery::LiveKitEligibleDaemonSource::new(
                            config_arc.clone(),
                            registry,
                            room_slot.clone(),
                        ),
                    )
                        as Arc<dyn tddy_session_lifecycle::multi_host::EligibleDaemonSource>,
                    common_room_livekit_room: room_slot,
                })
            }
            None => None,
        }
    }

    pub(super) fn open_mount(
        &self,
        config_arc: Arc<DaemonConfig>,
        user_resolver: SessionUserResolver,
        session_tokens: SessionTokens,
    ) -> Mount {
        let config = &self.config;
        let tddy_data_dir = &self.identity.keys.tddy_data_dir;
        let sessions_base_resolver: tddy_daemon_kernel::SessionsBaseResolver = {
            let dd = tddy_data_dir.clone();
            Arc::new(move |user: &str| {
                tddy_session_lifecycle::user_sessions_path::sessions_base_for_user(user, Some(&dd))
            })
        };
        // Every project a daemon serves is resolved against *that OS user's own* registry, so
        // this mirrors `sessions_base_resolver` one directory down.
        let projects_dir_resolver: tddy_session_lifecycle::remote_git_service::ProjectsDirResolver = {
            let dd = tddy_data_dir.clone();
            Arc::new(move |user: &str| {
                tddy_session_lifecycle::user_sessions_path::projects_path_for_user(user, Some(&dd))
            })
        };
        let bsp_session_resolver =
            bsp_session_resolver(&user_resolver, config, &sessions_base_resolver);
        let chat_workspace_roots = chat_workspace_roots(
            &user_resolver,
            config,
            &sessions_base_resolver,
            &projects_dir_resolver,
        );
        Mount {
            config_arc,
            session_tokens,
            livekit_discovery: None,
            user_resolver,
            sessions_base_resolver,
            projects_dir_resolver,
            bsp_session_resolver,
            chat_workspace_roots,
        }
    }
}

fn livekit_service(mount: &Mount) -> Arc<tddy_daemon_livekit::livekit_service::LiveKitServiceImpl> {
    tddy_daemon_livekit::build_livekit_service(
        tddy_daemon_livekit::livekit_rooms_stream::room_roster_from_config(
            mount.config_arc.livekit.as_ref(),
        ),
        mount.user_resolver.clone(),
    )
}

fn bsp_session_resolver(
    user_resolver: &SessionUserResolver,
    config: &DaemonConfig,
    sessions_base_resolver: &SessionsBaseResolver,
) -> tddy_bsp::bsp_service::SessionPathsResolver {
    let user_resolver = user_resolver.clone();
    let config = config.clone();
    let sessions_base_resolver = sessions_base_resolver.clone();
    Arc::new(move |token: &str, session_id: &str| {
        let github_user = (user_resolver)(token)
            .ok_or_else(|| tddy_rpc::Status::unauthenticated("invalid or expired session"))?;
        let os_user = config
            .os_user_for_github(&github_user)
            .ok_or_else(|| tddy_rpc::Status::permission_denied("user not mapped to OS user"))?
            .to_string();
        tddy_core::session_lifecycle::validate_session_id_segment(session_id)
            .map_err(|e| tddy_rpc::Status::invalid_argument(e.message()))?;
        let sessions_base = (sessions_base_resolver)(&os_user)
            .ok_or_else(|| tddy_rpc::Status::internal("could not resolve sessions path"))?;
        let repo_root =
            tddy_session_lifecycle::workspace_session::resolve_worktree_root_for_session(
                &sessions_base,
                session_id,
            )?;
        let session_dir =
            tddy_core::session_lifecycle::unified_session_dir_path(&sessions_base, session_id);
        Ok((session_dir, repo_root))
    })
}

fn chat_workspace_roots(
    user_resolver: &SessionUserResolver,
    config: &DaemonConfig,
    sessions_base_resolver: &SessionsBaseResolver,
    projects_dir_resolver: &ProjectsDirResolver,
) -> tddy_model_registry::ChatWorkspaceRoots {
    let user_resolver = user_resolver.clone();
    let config = config.clone();
    let sessions_base_resolver = sessions_base_resolver.clone();
    let projects_dir_resolver = projects_dir_resolver.clone();
    Arc::new(move |token: &str| {
        operator_workspace_roots(
            &config,
            &user_resolver,
            &sessions_base_resolver,
            &projects_dir_resolver,
            token,
        )
    })
}

/// Where a model chat may run an assistant's tools, for the operator holding `token`.
fn operator_workspace_roots(
    config: &DaemonConfig,
    user_resolver: &SessionUserResolver,
    sessions_base_resolver: &SessionsBaseResolver,
    projects_dir_resolver: &ProjectsDirResolver,
    token: &str,
) -> Result<Vec<PathBuf>, tddy_model_registry::ModelRegistryError> {
    use tddy_model_registry::ModelRegistryError;
    let github_user = (user_resolver)(token).ok_or_else(|| {
        ModelRegistryError::PermissionDenied("invalid or expired session token".to_string())
    })?;
    let os_user = config
        .os_user_for_github(&github_user)
        .ok_or_else(|| {
            ModelRegistryError::PermissionDenied("user not mapped to OS user".to_string())
        })?
        .to_string();
    let sessions_base = (sessions_base_resolver)(&os_user).ok_or_else(|| {
        ModelRegistryError::InvalidWorkspace(
            "could not resolve this operator's sessions path".to_string(),
        )
    })?;
    let projects_dir = (projects_dir_resolver)(&os_user).ok_or_else(|| {
        ModelRegistryError::InvalidWorkspace(
            "could not resolve this operator's projects path".to_string(),
        )
    })?;
    // Every session worktree this operator has, plus every project checkout their
    // own registry names — including per-host checkouts of the same project.
    let mut roots = vec![sessions_base.join("sessions")];
    let projects =
        tddy_session_lifecycle::project_storage::read_projects(&projects_dir).map_err(|e| {
            ModelRegistryError::InvalidWorkspace(format!(
                "could not read this operator's projects: {e}"
            ))
        })?;
    for project in projects {
        roots.push(std::path::PathBuf::from(&project.main_repo_path));
        roots.extend(
            project
                .host_repo_paths
                .values()
                .map(std::path::PathBuf::from),
        );
    }
    Ok(roots)
}
