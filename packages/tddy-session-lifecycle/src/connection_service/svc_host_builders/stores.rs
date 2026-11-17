use crate::CliSessionManager;
use std::sync::Arc;
use tddy_task::TaskRegistry;

pub(super) fn default_agent_tool_sockets(
) -> Arc<tddy_daemon_kernel::agent_tool_sockets::AgentToolSockets> {
    Arc::new(
        tddy_daemon_kernel::agent_tool_sockets::AgentToolSockets::new(
            tddy_daemon_kernel::agent_tool_sockets::AgentToolSocketPlacement::PerOsUser,
        ),
    )
}

pub(super) struct HostStores {
    pub(super) workspace_sandbox_provisioner:
        tddy_daemon_sandbox::workspace_tool_sandbox::JailedWorkspaceSandboxProvisioner,
    pub(super) task_registry: TaskRegistry,
    pub(super) demo_vm_state: Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<String, tddy_demo_vm_service::activity_hub::DemoVmHandle>,
        >,
    >,
    pub(super) host_session_sockets:
        Arc<tddy_agent_launch::host_session_socket::HostSessionSockets>,
    pub(super) session_agent_clones:
        Arc<tddy_session_agents::session_agent_clone::SessionAgentCloneStore>,
    pub(super) worktree_size_calculator:
        Arc<tddy_worktree_service::worktrees::WorktreeSizeCalculator>,
}

pub(super) fn build_host_stores(
    tddy_data_dir: &std::path::Path,
    cli_manager: &Arc<CliSessionManager>,
) -> HostStores {
    let workspace_sandbox_provisioner =
        tddy_daemon_sandbox::workspace_tool_sandbox::JailedWorkspaceSandboxProvisioner::for_tddy_home(
            tddy_data_dir,
        );
    let task_registry = cli_manager.task_registry();
    let demo_vm_state = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
    let host_session_sockets =
        Arc::new(super::super::host_session_socket::HostSessionSockets::default());
    // Built here rather than inline below because the roster store reads it: an entry's
    // `clone_state` is the state of the checkout serving it, and two stores would let a roster
    // report READY for a clone nobody built.
    let session_agent_clones = Arc::new(crate::session_agent_clone::SessionAgentCloneStore::new());
    let worktree_size_calculator = Arc::new(
        tddy_worktree_service::worktrees::WorktreeSizeCalculator::new(
            tddy_worktree_service::worktrees::projects_stats_cache_root(tddy_data_dir),
            tddy_worktree_service::worktrees::DAEMON_SIZE_WALK_PERMITS,
        ),
    );
    HostStores {
        workspace_sandbox_provisioner,
        task_registry,
        demo_vm_state,
        host_session_sockets,
        session_agent_clones,
        worktree_size_calculator,
    }
}
