//! The RPC entries mounted above the session host: terminal, files, agents, activity, host,
//! worktree, model, remote-git, action, BSP and VM services.

use super::build_context::BuildContext;
use super::mount::Mounted;
use super::*;

impl BuildContext {
    pub(super) fn mount_session_entries(&mut self, m: &Mounted) {
        let connection_arc = &m.hosted.connection_arc;
        // TerminalSessionService — the nine terminal methods, served by `tddy-terminal-rpc` on top
        // of the streaming bridge it already owned. `#unbundle` node 6 mounted this coordinate and
        // then removed the same nine from `the pre-unbundle monolithic RPC coordinate`, so this is the only
        // place they answer (`terminal_session_entry`).
        self.identity
            .rpc_entries
            .push(connection_arc.terminal_session_entry());

        // SessionFilesService — the thirteen session-file methods, served by `tddy-session-files`
        // on top of the ten modules it already owned. `#unbundle` node 6 mounted this coordinate
        // and then removed the same thirteen from `the pre-unbundle monolithic RPC coordinate`, so this is the
        // only place they answer. The entry is wrapped here rather than in the crate because eight
        // of the thirteen route by `daemon_instance_id` — browsing another host's documents is a
        // forward over the common room, which is this layer's to make (`session_files_entry`).
        self.identity
            .rpc_entries
            .push(connection_arc.session_files_entry());

        // SessionAgentService — the nine roster and conversation methods, served by
        // `tddy-session-agents` on top of the four modules it already owned. `#unbundle` node 7
        // mounted this coordinate and then removed the same nine from
        // `the pre-unbundle monolithic RPC coordinate`, so this is the only place they answer. The entry is
        // wrapped here rather than in the crate because seven of the nine route by
        // `daemon_instance_id` — a split session's roster lives on the daemon holding the codebase,
        // and reaching it is a forward over the common room, which is this layer's to make.
        self.identity
            .rpc_entries
            .push(connection_arc.session_agents_entry());

        // ActivityService — the eight activity, status, notification and ACP-replay methods,
        // served by `tddy-session-activity` on top of the two notification modules it already
        // owned. Mounted, and the old coordinate's eight removed, on the same terms as the nine
        // above.
        self.identity
            .rpc_entries
            .push(connection_arc.activity_entry());

        // SessionService — family C (`tddy-session-lifecycle`).
        self.identity
            .rpc_entries
            .push(connection_arc.session_lifecycle_entry());
    }

    pub(super) fn mount_family_entries(&mut self, m: &Mounted) {
        let connection_arc = &m.hosted.connection_arc;
        let rpc_handlers = &m.hosted.rpc_handlers;
        let livekit_reach = &self.plugins.livekit_reach;
        let session_tokens = &m.mount.session_tokens;
        // The families served from `tddy-daemon-rpc` — CatalogService (tools, agents, models and
        // subagents, `tddy-discovery`), ExecToolService (execute, stream, list tools and session
        // tool calls, `tddy-tool-engine`), ProjectService, family D (`tddy-projects`) and
        // PrStackService (stack planning and branch resolution, `tddy-pr-stack`) — through the same
        // handlers the host's session rooms serve.
        self.identity.rpc_entries.extend(rpc_handlers.entries());

        // SessionRoomService — LiveKit's own RPC: open a session's room, or fail naming it. The
        // plugin owns it; it resolves sessions through the host it extends.
        self.identity
            .rpc_entries
            .push(
                livekit_reach.session_room_entry(Arc::clone(connection_arc)
                    as Arc<dyn tddy_daemon_extensions::ReachableSessions>),
            );

        // DemoVmService — family O (`tddy-vm` coordinate, host logic on the session host).
        self.identity
            .rpc_entries
            .push(connection_arc.demo_vm_entry());

        self.identity
            .rpc_entries
            .push(tddy_daemon_auth::build_local_token_entry(Arc::new(
                session_tokens.signer().clone(),
            )));
    }

    pub(super) fn mount_host_entries(&mut self, m: &Mounted) {
        let host_service_impl = &m.served.host_service_impl;
        let livekit_service = &m.livekit_service;
        let worktree_service_impl = &m.served.worktree_service_impl;
        let index_daemon_registry = &self.index_daemon_registry;
        // HostService — the durable host registry, each host's tooling probe, its telemetry, the
        // prompts it raises and the keys it can be given. Registered here rather than beside the
        // local socket above so it rides the same entries as every other service: reachable over
        // HTTP `/rpc` and over the LiveKit common room, which is also how a *peer* daemon answers a
        // forwarded `GetHostTooling`.
        let host_server = tddy_service::HostServiceServer::from_arc(Arc::clone(host_service_impl));
        self.identity.rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "host.HostService",
            service: Arc::new(host_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // LiveKitService — the rooms this daemon can see on the LiveKit server and who is joined
        // to each. Family T left `the pre-unbundle monolithic RPC coordinate` in `#unbundle` node 4; the entry
        // comes from `tddy-daemon-livekit` assembled, so this wiring never names the poll cadence.
        self.identity
            .rpc_entries
            .push(tddy_daemon_livekit::build_livekit_entry(Arc::clone(
                livekit_service,
            )));

        // WorktreeService — listing, cleaning, sizing, restoring and reading a project's checkouts.
        let worktree_server =
            tddy_service::WorktreeServiceServer::from_arc(Arc::clone(worktree_service_impl));
        self.identity.rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "worktree.WorktreeService",
            service: Arc::new(worktree_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // CodeNavigationService — definition, references and hover for the code pane. Authorised by
        // the worktree service above, so it reaches no path that service would refuse to read, and
        // answered by the index daemon this runtime manages; without an `index_daemon:` section it
        // has none, and says so.
        self.identity.rpc_entries.push(
            tddy_daemon_rpc::code_navigation::build_code_navigation_entry(
                tddy_daemon_rpc::code_navigation::CodeNavigationServiceImpl::new(
                    Arc::clone(worktree_service_impl),
                    index_daemon_registry.clone().map(|registry| {
                        Arc::new(registry)
                            as Arc<dyn tddy_daemon_rpc::code_navigation::IndexChannelSource>
                    }),
                )
                .with_supports(Arc::clone(&self.index_supports)),
            ),
        );
    }

    pub(super) fn mount_model_entries(&mut self, m: &Mounted) {
        let model_registry = &m.shared.model_registry;
        let vm_user_resolver = m.mount.user_resolver.clone();
        let task_registry = m.hosted.task_registry.clone();
        let chat_workspace_roots = m.mount.chat_workspace_roots.clone();
        // ModelRegistryService — this daemon's providers, models and assistants. Rides the same
        // entries as every other service, so it is reachable over HTTP `/rpc` and LiveKit alike.
        // The entry comes from `tddy-model-registry` rather than being assembled here: the
        // subsystem's whole contract with this wiring layer is the `ServiceEntry` it returns.
        self.identity
            .rpc_entries
            .push(tddy_model_registry::build_model_registry_entry(
                Arc::clone(model_registry),
                Arc::new(tddy_model_registry::DefaultProviderClients),
                vm_user_resolver.clone(),
            ));

        // The model-addressed ACP surface: chatting with a registry model or assistant. The
        // *session*-addressed `acp.AcpService` is mounted per session process
        // (`session_view_adapter_surface`); this one is the daemon's own, so the Models & Agents
        // screen can open a chat without a session existing at all.
        self.identity
            .rpc_entries
            .push(tddy_model_registry::build_model_acp_entry(
                Arc::clone(model_registry),
                task_registry.clone(),
                vm_user_resolver.clone(),
                chat_workspace_roots,
            ));

        // TaskService — backed by the same registry as ConnectionService.
        // A `CancelTask` naming a peer daemon (a split start's codebase half) is forwarded to it
        // through the same common room the session host routes by.
        let mut task_service_impl = tddy_session_lifecycle::task_service::TaskServiceImpl::new(
            task_registry.clone(),
            vm_user_resolver.clone(),
        );
        if let Some(room) = m.hosted.connection_arc.common_room_livekit_room() {
            task_service_impl = task_service_impl.forwarding_to_peers(
                room,
                &tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(&self.config),
            );
        }
        let task_server = tddy_service::TaskServiceServer::new(task_service_impl);
        self.identity.rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "tasks.TaskService",
            service: Arc::new(task_server) as Arc<dyn tddy_rpc::RpcService>,
        });
    }

    pub(super) fn mount_remote_entries(&mut self, m: &Mounted) {
        let sessions_base_resolver = &m.mount.sessions_base_resolver;
        let projects_dir_resolver = m.mount.projects_dir_resolver.clone();
        let remote_git_user_resolver = m.mount.user_resolver.clone();
        let config_arc = &m.mount.config_arc;
        let session_admissions = m.hosted.session_admissions.clone();
        let shared_session_rooms = &self.registries.shared_session_rooms;
        // RemoteGitService — every project this daemon serves, usable as a git remote by any
        // client that can join the room (`GIT_SSH_COMMAND=tddy-remote-git-repo`).
        let pack_execution_resolver =
        tddy_session_lifecycle::remote_git_pack_execution::pack_execution_resolver_from_sessions(
            sessions_base_resolver.clone(),
            projects_dir_resolver.clone(),
        );
        let remote_git_server = tddy_service::RemoteGitServiceServer::new(
        tddy_session_lifecycle::remote_git_service::RemoteGitServiceImpl::new_with_pack_execution_resolver(
            remote_git_user_resolver.clone(),
            projects_dir_resolver,
            config_arc.clone(),
            pack_execution_resolver,
        ),
    );
        self.identity.rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "remote_git.RemoteGitService",
            service: Arc::new(remote_git_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // SessionAdmissionService — the room-admission handshake (PRD § "What attach does"
        // step 3). The facilitating daemon mints a scoped short-TTL token for an owning daemon
        // it has admitted, and revokes it when the last agent that daemon owns detaches. Served
        // on the common-room `daemon-{this}` participant so an owning daemon that is in the
        // session room (but whose token is expiring) can still reach this daemon over the
        // common room it never left.
        let session_admission_server = tddy_service::SessionAdmissionServiceServer::new(
            tddy_daemon_livekit::session_admission_service::SessionAdmissionServiceImpl::new(
                remote_git_user_resolver.clone(),
                config_arc.clone(),
                session_admissions,
                Arc::new({
                    let rooms = Arc::clone(shared_session_rooms);
                    move |session_id: &str| rooms.contains(session_id)
                }),
            ),
        );
        self.identity.rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "session_admission.SessionAdmissionService",
            service: Arc::new(session_admission_server) as Arc<dyn tddy_rpc::RpcService>,
        });
    }

    pub(super) fn mount_action_entries(&mut self, m: &Mounted) {
        let task_registry = m.hosted.task_registry.clone();
        let vm_user_resolver = m.mount.user_resolver.clone();
        let tddy_data_dir = &self.identity.keys.tddy_data_dir;
        let bsp_session_resolver = m.mount.bsp_session_resolver.clone();
        // ActionService — start tools by kind via tddy-actions runtimes.
        let action_service_impl = tddy_session_lifecycle::action_service::ActionServiceImpl::new(
            task_registry.clone(),
            tddy_actions::ActionCatalog::new(),
            vm_user_resolver.clone(),
        );
        let action_server = tddy_service::ActionServiceServer::new(action_service_impl);
        self.identity.rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "actions.ActionService",
            service: Arc::new(action_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // BSP build server — session-addressed: each request's token/session_id resolves to that
        // session's worktree + catalog.db (`bsp_service`), so daemon-managed claude-cli/cursor
        // sessions expose build targets over the same surface as ConnectionService.
        self.identity
            .rpc_entries
            .push(tddy_bsp::build_bsp_service_entry(
                bsp_session_resolver,
                tddy_data_dir.clone(),
            ));
    }

    pub(super) fn mount_vm_entry(&mut self, m: &Mounted) {
        let tddy_data_dir = &self.identity.keys.tddy_data_dir;
        let vm_user_resolver = m.mount.user_resolver.clone();
        let task_registry = m.hosted.task_registry.clone();
        // VM lifecycle service — gated on auth being configured (same as ConnectionService).
        // Per-VM manifest files under the VM & Image Library are the source of truth
        // (superseding the old single shared vm-registry.json); rooted at the same
        // config-only tddy data dir every other per-user store here resolves from.
        let vm_library = {
            let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
            let base = tddy_session_lifecycle::user_sessions_path::tddy_data_root_matching_child(
                &user,
                Some(tddy_data_dir),
            )
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
            let library = tddy_vm::VmLibrary::new(base);
            if let Err(e) = library.init() {
                log::error!(
                    "Failed to initialize VM & Image Library at {}: {e}",
                    library.root().display()
                );
            }
            library
        };
        let vm_manager = Arc::new(tddy_vm::VmManager::from_library(
            vm_library,
            Box::new(tddy_vm::QemuVm),
        ));
        let vm_service_impl =
            tddy_vm::VmServiceImpl::new(Arc::clone(&vm_manager), vm_user_resolver, task_registry);
        let vm_server = tddy_service::VmServiceServer::new(vm_service_impl);
        self.identity.rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "vm.VmService",
            service: Arc::new(vm_server) as Arc<dyn tddy_rpc::RpcService>,
        });
    }
}
