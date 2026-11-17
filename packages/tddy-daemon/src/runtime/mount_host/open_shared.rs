use crate::runtime::build_context::BuildContext;
use crate::runtime::mount::Shared;

use crate::runtime::mount_host::RegisteredSubscriber;
use crate::runtime::*;

impl BuildContext {
    pub(in crate::runtime) async fn open_shared(
        &self,
        sessions_base_resolver: &tddy_daemon_kernel::SessionsBaseResolver,
    ) -> anyhow::Result<Shared> {
        let config = &self.config;
        let tddy_data_dir = &self.identity.keys.tddy_data_dir;
        // This daemon's model registry: the providers it talks to, their models, and the
        // assistants composed from them. Opened before ConnectionService so an assistant is
        // listed by `ListAgents` as a selectable `--agent`.
        // The coding agents this host's `--agent` answers to (built-ins and `<tddyhome>/tools`), so
        // an assistant cannot take the name of any of them.
        let reserved_coding_agent_ids =
            tddy_coder::agent_registry_cli::reserved_agent_ids(self.coding_agents.agents());
        let model_registry = Arc::new(
            tddy_model_registry::ModelRegistryStore::open(
                &tddy_data_dir.join("models.db"),
                &tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(config),
                // The same directory `DaemonSessionHost` resolves YAML defs from, so an
                // assistant cannot be created under a name one of them already answers to.
                &tddy_data_dir.join("agents"),
            )
            .await?
            .reserving_agent_ids(config.allowed_agents().iter().map(|a| a.id.clone()))
            .reserving_coding_agent_ids(reserved_coding_agent_ids),
        );
        // The project notification log the project-home inbox tools read: every attention
        // notification of a project's sessions, kept across restarts. Opened before the bus, which
        // it is registered on, so nothing published after the bus is assembled goes unrecorded.
        let project_notification_log = Arc::new(
            tddy_session_activity::project_notification_log::ProjectNotificationLog::open(
                tddy_data_dir,
                Arc::clone(sessions_base_resolver),
            )?,
        );
        // The daemon's session-notification bus: the operator channel (Telegram's, on a daemon
        // that has one) takes the attention-worthy events from the activity-status path, and
        // `StreamSessionNotifications` relays every event to the browsers driving the drawer's
        // indicators. Assembled here because `DaemonSessionHost::new` installs no bus of its own,
        // and because the stream subscriber must be the very one the RPC handler subscribes to.
        let (wake_trigger, wake_cues) =
            tddy_daemon_rpc::exec_tool::watch_wake::WakeTrigger::channel();
        let session_notification_bus = {
            let bus = tddy_session_lifecycle::session_notifications::SessionNotificationBus::new()
                .with_subscriber(Arc::new(
                    tddy_session_lifecycle::session_notifications::OperatorChannelSubscriber::new(
                        self.operator_channel(),
                    ),
                ));
            let bus = bus.with_subscriber(Arc::new(
                tddy_session_lifecycle::session_notification_subscribers::SessionNotificationStreamSubscriber::new(),
            ));
            let bus = bus.with_subscriber(Arc::clone(&project_notification_log));
            // After the log, so a wake reads a record the log already holds.
            let bus = bus.with_subscriber(Arc::new(wake_trigger));
            // The subscribers an embedding host registered, after both built-ins in registration order.
            let bus = self
                .options
                .notification_subscribers
                .iter()
                .fold(bus, |bus, registered| {
                    bus.with_subscriber(Arc::new(RegisteredSubscriber(Arc::clone(registered))))
                });
            Arc::new(bus)
        };
        // One registry for the whole daemon. `ConnectionService` streams and answers prompts;
        // `ScreenSharingService` raises them for a desktop password. A prompt raised on one
        // instance and answered on another is a question nobody can answer, so both are handed
        // this one.
        let host_prompts: Arc<dyn tddy_session_lifecycle::host_prompts::HostPromptRegistry> =
            Arc::new(tddy_session_lifecycle::host_prompts::InMemoryHostPromptRegistry::new());
        Ok(Shared {
            model_registry,
            session_notification_bus,
            project_notification_log,
            session_watches: open_session_watches(tddy_data_dir)?,
            wake_cues: std::sync::Mutex::new(Some(wake_cues)),
            host_prompts,
        })
    }
}

/// The orchestrators' session watches, kept per project beside the notification log they read
/// (`#activity-wake` 2/3).
fn open_session_watches(
    tddy_data_dir: &std::path::Path,
) -> anyhow::Result<Arc<tddy_session_activity::session_watches::SessionWatchStore>> {
    Ok(Arc::new(
        tddy_session_activity::session_watches::SessionWatchStore::open(tddy_data_dir)?,
    ))
}
