//! The registries, plugins and optional integrations `build` assembles once the identity is known:
//! the common room handle, the shared session managers, LiveKit's ports, Telegram and relay mode.

use super::identity::Keys;
use super::telegram_wiring::build_telegram;
use super::*;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_daemon_livekit::common_room_supervisor::SupervisedCommonRoom;
use tddy_daemon_livekit::session_admission_service::SessionAdmissionRegistry;
use tddy_daemon_livekit::session_room::SessionRoomRegistry;
use tddy_session_lifecycle::cli_session_manager::CliSessionManager;
use tddy_telegram_control::telegram_session_subscriber::TelegramDaemonHooks;

/// The handles the whole daemon shares: one of each, however many services hold it.
pub(super) struct Registries {
    pub(super) common_room: Arc<SupervisedCommonRoom>,
    pub(super) config_service_authenticator: tddy_service::SessionTokenAuthenticator,
    pub(super) shared_claude_cli_manager: Arc<CliSessionManager>,
    pub(super) shared_session_rooms: Arc<SessionRoomRegistry>,
    pub(super) shared_session_admissions: Arc<SessionAdmissionRegistry>,
}

impl Registries {
    pub(super) fn open(config: &DaemonConfig, user_resolver: Option<&SessionUserResolver>) -> Self {
        // The handle the config service reconfigures the LiveKit connection through. Built before the
        // roster because the roster holds it; the connection it supervises is assembled at the end,
        // once there is a roster to serve on the room.
        let common_room = Arc::new(SupervisedCommonRoom::new(config.livekit.as_ref()));
        // The same rule that admits a caller to every other gated daemon RPC — this one reads and
        // writes the daemon's own LiveKit credentials.
        let config_service_authenticator =
            tddy_session_lifecycle::auth::session_token_authenticator(
                user_resolver,
                DaemonConfigServiceServer::<DaemonConfigServiceImpl>::NAME,
            );

        // Create one shared ClaudeCliSessionManager — injected into both the Telegram spawn path and
        // DaemonSessionHost so that Telegram-launched sessions are attachable via the terminal RPCs.
        let shared_claude_cli_manager =
            Arc::new(tddy_session_lifecycle::cli_session_manager::CliSessionManager::new());

        // One registry of hosted session rooms for the whole daemon: `StartSession` opens rooms in it
        // and both deletion paths — the `DeleteSession` RPC and Telegram's Delete button — close them
        // there. Two registries would mean a room only one of them could ever stop hosting.
        let shared_session_rooms =
            Arc::new(tddy_daemon_livekit::session_room::SessionRoomRegistry::new());
        // The owning daemons admitted to those rooms: the host records the first admit, the admission
        // service served on the common room refreshes it, and the delete paths revoke against the same
        // set.
        let shared_session_admissions = Arc::new(
            tddy_daemon_livekit::session_admission_service::SessionAdmissionRegistry::new(),
        );
        Self {
            common_room,
            config_service_authenticator,
            shared_claude_cli_manager,
            shared_session_rooms,
            shared_session_admissions,
        }
    }
}

/// What each optional integration reports about itself, and the LiveKit plugin behind the ports.
pub(super) struct Plugins {
    pub(super) integration_board: Arc<tddy_daemon_extensions::IntegrationStatusBoard>,
    pub(super) livekit_status: tddy_daemon_extensions::IntegrationReporter,
    /// Telegram's entry: what the `telegram:` block says at start, then the bot's identity check
    /// and every send the hooks make.
    pub(super) telegram_status: tddy_daemon_extensions::IntegrationReporter,
    pub(super) livekit_reach: Arc<tddy_daemon_livekit::LiveKitSessionReach>,
    pub(super) terminal_exposure: Arc<dyn tddy_daemon_extensions::TerminalExposure>,
    pub(super) split_rooms: Arc<dyn tddy_daemon_extensions::SplitRoomHosting>,
}

impl Plugins {
    pub(super) fn open(
        config: &DaemonConfig,
        shared_session_rooms: &Arc<SessionRoomRegistry>,
        room_tokens: &Arc<dyn tddy_daemon_extensions::RoomTokenMinter>,
    ) -> Self {
        // What each optional integration reports about itself, read by `GetIntegrationStatus`; and the
        // LiveKit plugin behind the session host's `SessionReach` port. The plugin shares the host's
        // room registry so the rooms it opens are the ones deletion closes.
        let integration_board = tddy_daemon_extensions::IntegrationStatusBoard::new();
        // One reporter, cloned to the discovery loop below: both report into the same board entry.
        let livekit_status =
            integration_board.reporter(tddy_daemon_livekit::LIVEKIT_INTEGRATION_ID, "LiveKit");
        let telegram_status =
            integration_board.reporter(tddy_telegram::status::TELEGRAM_INTEGRATION_ID, "Telegram");
        // Reported whatever the block says: an enabled block with no token starts no bot, and this
        // entry is the only place an operator is told why.
        telegram_status.report(tddy_telegram::status::configured_state(
            config.telegram.as_ref(),
        ));
        let livekit_reach = tddy_daemon_livekit::LiveKitSessionReach::new(
            config.clone(),
            Arc::clone(shared_session_rooms),
            livekit_status.clone(),
        );

        // The LiveKit plugin behind the `TerminalExposure` port, shared by the session host and the
        // Telegram start that announces its terminal's room at once. Without credentials there is
        // nothing to join, and the port's null object says so.
        let terminal_exposure: Arc<dyn tddy_daemon_extensions::TerminalExposure> =
            match tddy_daemon_livekit::terminal_exposure::LiveKitTerminalExposure::from_config(
                config,
            ) {
                Some(exposure) => exposure,
                None => Arc::new(tddy_daemon_extensions::NoTerminalExposure),
            };

        let split_rooms = split_room_hosting(
            config,
            Arc::clone(shared_session_rooms),
            Arc::clone(room_tokens),
        );
        Self {
            integration_board,
            livekit_status,
            telegram_status,
            livekit_reach,
            terminal_exposure,
            split_rooms,
        }
    }
}

/// A configured Telegram bot: its hooks, its identity check, its inbound dispatcher and the
/// lifecycle chat.
pub(super) struct Telegram {
    pub(super) hooks: Option<Arc<TelegramDaemonHooks>>,
    pub(super) identity_check: Option<crate::runtime::tasks::TelegramIdentityCheck>,
    pub(super) inbound: Option<crate::runtime::tasks::TelegramInbound>,
    pub(super) lifecycle: Option<(DaemonConfig, Arc<dyn TelegramSender + Send + Sync>)>,
}

impl Telegram {
    pub(super) fn open(
        config: &DaemonConfig,
        options: &RuntimeOptions,
        keys: &Keys,
        registries: &Registries,
        plugins: &Plugins,
    ) -> Self {
        let telegram = build_telegram(config, &keys.tddy_data_dir, options, registries, plugins);
        let telegram_hooks = telegram.hooks;
        let lifecycle_telegram = telegram_hooks.as_ref().map(|h| {
            (
                config.clone(),
                h.sender.clone() as Arc<dyn TelegramSender + Send + Sync>,
            )
        });
        Self {
            hooks: telegram_hooks,
            identity_check: telegram.identity_check,
            inbound: telegram.inbound,
            lifecycle: lifecycle_telegram,
        }
    }
}

/// Relay mode: an idle tracker the RPC surface touches, plus the channel its monitor fires.
pub(super) struct RelayIdle {
    pub(super) idle_tracker: Option<Arc<tddy_session_lifecycle::relay_idle::IdleTimeoutTracker>>,
    pub(super) relay_idle_monitor: Option<(
        Arc<tddy_session_lifecycle::relay_idle::IdleTimeoutTracker>,
        tokio::sync::oneshot::Sender<()>,
    )>,
    pub(super) relay_shutdown: Option<tokio::sync::oneshot::Receiver<()>>,
}

impl RelayIdle {
    pub(super) fn open(options: &RuntimeOptions) -> Self {
        // Relay mode: an idle tracker the RPC surface touches, plus the channel its monitor fires.
        let (idle_tracker, relay_idle_monitor, relay_shutdown) = match options.relay_idle_timeout {
            Some(timeout) => {
                let tracker = Arc::new(
                    tddy_session_lifecycle::relay_idle::IdleTimeoutTracker::new(timeout),
                );
                let (tx, rx) = tokio::sync::oneshot::channel::<()>();
                (Some(Arc::clone(&tracker)), Some((tracker, tx)), Some(rx))
            }
            None => (None, None, None),
        };
        Self {
            idle_tracker,
            relay_idle_monitor,
            relay_shutdown,
        }
    }
}
