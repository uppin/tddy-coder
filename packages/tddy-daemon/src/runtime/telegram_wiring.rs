use crate::config::DaemonConfig;
use crate::runtime::integrations::{Plugins, Registries};
use crate::runtime::tasks::{TelegramIdentityCheck, TelegramInbound};
use crate::runtime::RuntimeOptions;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};
use tddy_telegram_control::telegram_notifier::TelegramSender;
use teloxide::prelude::Bot;
use tokio::sync::Mutex;

/// What a configured Telegram bot contributes: the hooks every session-notification path publishes
/// through, the identity check that tells the status board whether its token works, and the inbound
/// dispatcher that serves the buttons an operator presses.
// TODO(restructure-engine): `move_item` widened `build_telegram` but not the private struct it returns.
pub(super) struct TelegramWiring {
    pub(super) hooks:
        Option<Arc<tddy_telegram_control::telegram_session_subscriber::TelegramDaemonHooks>>,
    pub(super) identity_check: Option<TelegramIdentityCheck>,
    pub(super) inbound: Option<TelegramInbound>,
}

impl TelegramWiring {
    /// A daemon with no bot: no hooks, nothing to check, nothing to dispatch.
    fn none() -> Self {
        Self {
            hooks: None,
            identity_check: None,
            inbound: None,
        }
    }
}

/// The sender the hooks make every send through — notifications, elicitations, the lifecycle
/// messages — reporting each outcome on Telegram's board entry, token redacted.
fn reporting_sender(
    bot: &Bot,
    bot_token: &str,
    telegram_status: &tddy_daemon_extensions::IntegrationReporter,
) -> Arc<dyn TelegramSender + Send + Sync> {
    Arc::new(tddy_telegram::status::ReportingSender::new(
        tddy_telegram_control::telegram_notifier::TeloxideSender::new(bot.clone()),
        telegram_status.clone(),
        bot_token,
    ))
}

/// The spawn worker a Telegram-started session is launched through.
#[cfg(unix)]
fn spawn_client_for_telegram(
    options: &RuntimeOptions,
) -> Option<Arc<tddy_spawn::spawn_worker::SpawnClient>> {
    options
        .spawn_client
        .as_ref()
        .map(|(c, _)| Arc::new(c.clone()))
}

/// No spawn worker exists off unix.
#[cfg(not(unix))]
fn spawn_client_for_telegram(
    _options: &RuntimeOptions,
) -> Option<Arc<tddy_spawn::spawn_worker::SpawnClient>> {
    None
}

/// The port a split session's room is hosted and its agent joined through: the LiveKit plugin's,
/// over `rooms` — the host's one registry, so deletion closes the room a split start opens — and
/// `room_tokens`, the one minter. A daemon with no `livekit:` block has no room integration, and
/// split placement is refused.
pub fn split_room_hosting(
    config: &DaemonConfig,
    rooms: Arc<tddy_daemon_livekit::session_room::SessionRoomRegistry>,
    room_tokens: Arc<dyn tddy_daemon_extensions::RoomTokenMinter>,
) -> Arc<dyn tddy_daemon_extensions::SplitRoomHosting> {
    match config.livekit {
        Some(_) => Arc::new(tddy_daemon_livekit::LiveKitSplitRooms::new(
            config.clone(),
            rooms,
            room_tokens,
        )),
        None => Arc::new(tddy_daemon_extensions::NoSplitRooms),
    }
}

pub(super) fn build_telegram(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    options: &RuntimeOptions,
    registries: &Registries,
    plugins: &Plugins,
) -> TelegramWiring {
    // No block, switched off, or no token: no bot. Which of the three it is was already reported
    // on the status board from the same block (`tddy_telegram::status::configured_state`).
    let tg = match config.telegram.as_ref() {
        Some(tg) if tg.enabled && !tg.bot_token.is_empty() => tg,
        _ => return TelegramWiring::none(),
    };

    let bot = Bot::new(tg.bot_token.clone());
    let teloxide_sender =
        Arc::new(tddy_telegram_control::telegram_notifier::TeloxideSender::new(bot.clone()));
    let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
    let sender = reporting_sender(&bot, &tg.bot_token, &plugins.telegram_status);
    let identity_check = Some(TelegramIdentityCheck::new(
        bot.clone(),
        plugins.telegram_status.clone(),
        &tg.bot_token,
    ));
    let elicitation_select_options: tddy_telegram_control::telegram_notifier::ElicitationSelectOptionsCache =
        Arc::new(StdMutex::new(HashMap::new()));
    let elicitation_multi_select_meta: tddy_telegram_control::telegram_notifier::ElicitationMultiSelectMetaCache =
        Arc::new(StdMutex::new(HashMap::new()));
    let active_elicitation = Arc::new(StdMutex::new(
        tddy_telegram::active_elicitation::ActiveElicitationCoordinator::new(),
    ));
    let telegram_tracked = Arc::new(StdMutex::new(
        tddy_telegram::telegram_tracked_session::TelegramTrackedSessionCoordinator::new(),
    ));
    let watcher = Arc::new(Mutex::new(
        tddy_telegram_control::telegram_notifier::TelegramSessionWatcher::with_elicitation_caches_coordinator_and_tracked(
            elicitation_select_options.clone(),
            elicitation_multi_select_meta.clone(),
            active_elicitation.clone(),
            telegram_tracked.clone(),
        ),
    ));
    let hooks = Arc::new(
        tddy_telegram_control::telegram_session_subscriber::TelegramDaemonHooks::new(
            config.clone(),
            sender.clone(),
            watcher,
        ),
    );

    let sessions_base =
        match tddy_session_lifecycle::user_sessions_path::tddy_data_root_matching_child(
            &user,
            Some(tddy_data_dir),
        ) {
            Some(base) => base,
            None => {
                log::warn!(
                    target: "tddy_daemon",
                    "telegram inbound session control disabled: could not resolve sessions base for USER={user}"
                );
                return TelegramWiring {
                    hooks: Some(hooks),
                    identity_check,
                    inbound: None,
                };
            }
        };

    let workflow_spawn = Some(Arc::new(
        tddy_telegram_control::telegram_session_control::TelegramWorkflowSpawn {
            config: Arc::new(config.clone()),
            spawn_client: spawn_client_for_telegram(options),
            spawn_backend: options.spawn_backend.clone(),
            os_user: user.clone(),
            tddy_data_dir: tddy_data_dir.to_path_buf(),
            projects_dir_override: None,
            telegram_hooks: Some(hooks.clone()),
            child_grpc_by_session: Arc::new(StdMutex::new(HashMap::new())),
            elicitation_select_options: elicitation_select_options.clone(),
            elicitation_multi_select_meta: elicitation_multi_select_meta.clone(),
            pending_elicitation_other: Arc::new(StdMutex::new(HashMap::new())),
            claude_cli_manager: Arc::clone(&registries.shared_claude_cli_manager),
        },
    ));
    let harness = Arc::new(Mutex::new(
        tddy_telegram_control::telegram_session_control::TelegramSessionControlHarness::with_workflow_spawn_and_telegram_tracked(
            tg.chat_ids.clone(),
            sessions_base,
            teloxide_sender,
            workflow_spawn,
            Some(active_elicitation),
            Some(telegram_tracked),
        )
        .with_session_rooms(registries.shared_session_rooms.clone())
        .with_terminal_exposure(Arc::clone(&plugins.terminal_exposure)),
    ));

    TelegramWiring {
        hooks: Some(hooks),
        identity_check,
        inbound: Some(TelegramInbound { bot, harness }),
    }
}
