//! Acceptance tests: telegram-tracked session gate + traffic logging (PRD Testing Plan §2).
//!
//! These tests encode the target behavior for **gated workflow keyboards**, **Enter session** replay,
//! and **structured Telegram traffic logs**. They are expected to fail until production wiring lands.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Once};

use log::{LevelFilter, Log, Metadata, Record};
use tddy_core::SessionMetadata;
use tddy_daemon::active_elicitation::{
    ActiveElicitationCoordinator, SharedActiveElicitationCoordinator,
};
use tddy_daemon::config::{DaemonConfig, TelegramConfig};
use tddy_daemon::telegram_notifier::{
    ElicitationSelectOptionsCache, InMemoryTelegramSender, TelegramSessionWatcher,
};
use tddy_daemon::telegram_session_control::{TelegramSessionControlHarness, CB_ENTER};
use tddy_daemon::telegram_tracked_session::{
    SharedTelegramTrackedSessionCoordinator, TelegramTrackedSessionCoordinator,
};
use tddy_service::gen::app_mode_proto::Variant;
use tddy_service::gen::server_message::Event;
use tddy_service::gen::{
    AppModeProto, AppModeSelect, ClarificationQuestionProto, ModeChanged, QuestionOptionProto,
    ServerMessage,
};

const AUTHORIZED_CHAT: i64 = 424_242;

fn telegram_config_with_token(bot_token: &str) -> DaemonConfig {
    DaemonConfig {
        telegram: Some(TelegramConfig {
            enabled: true,
            bot_token: bot_token.to_string(),
            chat_ids: vec![AUTHORIZED_CHAT],
        }),
        ..Default::default()
    }
}

fn select_elicitation_server_message(question: &str, opt_a: &str, opt_b: &str) -> ServerMessage {
    ServerMessage {
        event: Some(Event::ModeChanged(ModeChanged {
            mode: Some(AppModeProto {
                variant: Some(Variant::Select(AppModeSelect {
                    question: Some(ClarificationQuestionProto {
                        header: "Clarify".into(),
                        question: question.into(),
                        options: vec![
                            QuestionOptionProto {
                                label: opt_a.into(),
                                description: String::new(),
                            },
                            QuestionOptionProto {
                                label: opt_b.into(),
                                description: String::new(),
                            },
                        ],
                        multi_select: false,
                        allow_other: false,
                        recommended_other: String::new(),
                    }),
                    question_index: 0,
                    total_questions: 1,
                    initial_selected: 0,
                })),
            }),
        })),
    }
}

/// Workflow action keyboards that must be suppressed until the chat is telegram-tracked for the session.
fn is_suppressed_workflow_keyboard_callback(callback_data: &str) -> bool {
    callback_data.starts_with("eli:")
        || callback_data.starts_with("doc:")
        || callback_data.starts_with("plan_review:")
        || callback_data.starts_with("recipe:")
        || callback_data.starts_with("mr:")
        || callback_data.starts_with("intent:")
        || callback_data.starts_with("tp:")
        || callback_data.starts_with("ta:")
        || callback_data.starts_with("tb:")
        || callback_data.starts_with("tbm:")
}

fn all_callbacks_for_chat(sender: &InMemoryTelegramSender, chat_id: i64) -> Vec<String> {
    sender
        .recorded_with_keyboards()
        .iter()
        .filter(|(cid, _, _)| *cid == chat_id)
        .flat_map(|(_, _, rows)| rows.iter().flatten().map(|(_, cb)| cb.clone()))
        .collect()
}

fn write_minimal_running_session(sessions_base: &std::path::Path, session_id: &str) {
    let session_dir = sessions_base.join("sessions").join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let metadata = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "proj-acceptance".to_string(),
        created_at: "2026-05-02T10:00:00Z".to_string(),
        updated_at: "2026-05-02T10:00:30Z".to_string(),
        status: "running".to_string(),
        repo_path: Some("/tmp/repo".to_string()),
        pid: None,
        tool: Some("tddy-coder".to_string()),
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
    };
    tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();
}

// ---------------------------------------------------------------------------
// Log capture (global logger — only used from this test module’s logging test)
// ---------------------------------------------------------------------------

static INSTALL_CAPTURE_LOGGER: Once = Once::new();
static LOG_CAPTURE_SLOT: Mutex<Option<Arc<Mutex<Vec<String>>>>> = Mutex::new(None);

struct TelegramTrafficCaptureLogger;

impl Log for TelegramTrafficCaptureLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        let t = metadata.target();
        t.starts_with("tddy_daemon::telegram") || t.starts_with("tddy_daemon::telegram_bot")
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("{} {}", record.target(), record.args());
        let slot = LOG_CAPTURE_SLOT.lock().expect("LOG_CAPTURE_SLOT poisoned");
        if let Some(buf) = slot.as_ref() {
            buf.lock().expect("capture buf poisoned").push(line);
        }
    }

    fn flush(&self) {}
}

fn install_telegram_traffic_capture_logger() {
    INSTALL_CAPTURE_LOGGER.call_once(|| {
        let _ = log::set_boxed_logger(Box::new(TelegramTrafficCaptureLogger))
            .map(|_| log::set_max_level(LevelFilter::Trace));
    });
}

/// `telegram_untracked_chat_suppresses_workflow_keyboards_shows_enter_only`
#[tokio::test]
#[serial_test::serial]
async fn telegram_untracked_chat_suppresses_workflow_keyboards_shows_enter_only() {
    let sid = "01900000-0000-7000-8000-0000000000aa";
    let mut watcher = TelegramSessionWatcher::new();
    let secret_token = "acceptance-test-bot-token-do-not-log-raw";
    let cfg = telegram_config_with_token(secret_token);
    let mem = InMemoryTelegramSender::new();
    let msg = select_elicitation_server_message("Tracked-session gate question", "A", "B");

    watcher
        .on_server_message(&cfg, &mem, sid, &msg)
        .await
        .unwrap();

    let callbacks = all_callbacks_for_chat(&mem, AUTHORIZED_CHAT);
    let forbidden: Vec<&str> = callbacks
        .iter()
        .map(String::as_str)
        .filter(|cb| is_suppressed_workflow_keyboard_callback(cb))
        .collect();

    assert!(
        forbidden.is_empty(),
        "authorized chat with no telegram-tracked session must not receive workflow action \
         inline keyboards (eli/doc/plan/recipe/intent/…); got forbidden callbacks={forbidden:?} — \
         full recording={:?}",
        mem.recorded_with_keyboards()
    );

    let enter_needle = format!("{CB_ENTER}{sid}");
    assert!(
        mem.recorded_with_keyboards().iter().any(|(cid, _, rows)| {
            *cid == AUTHORIZED_CHAT
                && rows
                    .iter()
                    .flatten()
                    .any(|(_, cb)| cb.as_str() == enter_needle.as_str())
        }),
        "when keyboards are gated, outbound must include Enter session callback {:?}; recorded={:?}",
        enter_needle,
        mem.recorded_with_keyboards()
    );

    let transcript = mem
        .recorded()
        .into_iter()
        .map(|(_, t)| t)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !transcript.contains(secret_token),
        "Telegram outbound must never embed raw bot_token substrings in message bodies"
    );
}

/// `telegram_enter_session_tracks_chat_and_replays_elicitation_with_full_keyboard`
#[tokio::test]
#[serial_test::serial]
async fn telegram_enter_session_tracks_chat_and_replays_elicitation_with_full_keyboard() {
    let tmp = tempfile::tempdir().unwrap();
    let sid = "01900000-0000-7000-8000-0000000000dd";
    write_minimal_running_session(tmp.path(), sid);

    let opts: ElicitationSelectOptionsCache = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let coord: SharedActiveElicitationCoordinator =
        Arc::new(std::sync::Mutex::new(ActiveElicitationCoordinator::new()));
    let tracked: SharedTelegramTrackedSessionCoordinator =
        Arc::new(Mutex::new(TelegramTrackedSessionCoordinator::new()));

    let cfg = telegram_config_with_token("secondary-token-not-in-assertions");

    let watcher_inner =
        TelegramSessionWatcher::with_elicitation_select_options_coordinator_and_tracked(
            opts,
            coord.clone(),
            tracked.clone(),
        );
    let watcher_arc = Arc::new(tokio::sync::Mutex::new(watcher_inner));

    let sender = Arc::new(InMemoryTelegramSender::new());
    let harness = TelegramSessionControlHarness::with_workflow_spawn_and_telegram_tracked(
        vec![AUTHORIZED_CHAT],
        tmp.path().to_path_buf(),
        sender.clone(),
        None,
        Some(coord),
        Some(tracked),
    );
    harness.connect_telegram_elicitation_replay_bridge(cfg.clone(), watcher_arc.clone());

    let msg = select_elicitation_server_message("Replay after Enter", "P", "Q");

    {
        let mut w = watcher_arc.lock().await;
        w.on_server_message(&cfg, sender.as_ref(), sid, &msg)
            .await
            .unwrap();
    }

    let before_len = sender.recorded_with_keyboards().len();
    harness
        .handle_enter_session(AUTHORIZED_CHAT, sid)
        .await
        .expect("enter existing session");

    let recorded = sender.recorded_with_keyboards();
    let tail = &recorded[before_len..];
    let replayed_eli_s = tail
        .iter()
        .any(|(_, _, rows)| rows.iter().flatten().any(|(_, cb)| cb.contains("eli:s:")));

    assert!(
        replayed_eli_s,
        "after Enter session, chat must be tracked for {sid} and pending elicitation must be \
         re-delivered with a full clarification keyboard (eli:s:…); messages_after_enter={tail:?}"
    );
}

/// `telegram_inbound_outbound_log_lines_include_chat_id_and_direction`
#[tokio::test]
#[serial_test::serial]
async fn telegram_inbound_outbound_log_lines_include_chat_id_and_direction() {
    install_telegram_traffic_capture_logger();
    let lines = Arc::new(Mutex::new(Vec::new()));
    {
        let mut slot = LOG_CAPTURE_SLOT.lock().expect("slot");
        *slot = Some(lines.clone());
    }

    let sid = "01900000-0000-7000-8000-0000000000ee";
    let secret_token = "log-proof-bot-token-leak-check";
    let mut watcher = TelegramSessionWatcher::new();
    let cfg = telegram_config_with_token(secret_token);
    let mem = InMemoryTelegramSender::new();
    let msg = select_elicitation_server_message("Logging probe", "U", "V");

    watcher
        .on_server_message(&cfg, &mem, sid, &msg)
        .await
        .unwrap();

    let snapshot = lines.lock().expect("lines").join("\n");

    {
        let mut slot = LOG_CAPTURE_SLOT.lock().expect("slot");
        *slot = None;
    }

    assert!(
        snapshot.contains(&format!("{AUTHORIZED_CHAT}")),
        "telegram traffic logs must include numeric chat_id; captured:\n{snapshot}"
    );
    assert!(
        snapshot.contains("direction=inbound") && snapshot.contains("direction=outbound"),
        "telegram traffic logs must label inbound vs outbound events (structured logging); \
         captured:\n{snapshot}"
    );
    assert!(
        snapshot.contains(&format!("session_id={sid}")),
        "telegram traffic logs must carry session context when routing applies; captured:\n{snapshot}"
    );
    assert!(
        !snapshot.contains(secret_token),
        "structured telegram logs must never include raw bot_token material; captured:\n{snapshot}"
    );
}
