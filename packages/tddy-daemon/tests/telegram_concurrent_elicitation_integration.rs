//! Acceptance tests: concurrent Telegram sessions sharing one chat — single active elicitation token.
//!
//! PRD: `Telegram: concurrent sessions with single visible question` (Testing Plan §2).

use std::sync::Arc;

use tddy_daemon::telegram_notifier::{InMemoryTelegramSender, TelegramSessionWatcher};
use tddy_daemon::telegram_session_control::TelegramSessionControlHarness;
use tddy_service::gen::app_mode_proto::Variant;
use tddy_service::gen::server_message::Event;
use tddy_service::gen::{
    AppModeProto, AppModeSelect, ClarificationQuestionProto, ModeChanged, QuestionOptionProto,
    ServerMessage,
};

const AUTHORIZED_CHAT: i64 = 424_242;

fn select_elicitation_message(question: &str, opt_a: &str, opt_b: &str) -> ServerMessage {
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
                    }),
                    question_index: 0,
                    total_questions: 1,
                    initial_selected: 0,
                })),
            }),
        })),
    }
}

/// Counts outbound messages for `chat_id` whose inline keyboard includes an `eli:s:` clarification callback.
fn count_eli_s_primary_keyboards(sender: &InMemoryTelegramSender, chat_id: i64) -> usize {
    sender
        .recorded_with_keyboards()
        .iter()
        .filter(|(cid, _, rows)| {
            *cid == chat_id && rows.iter().flatten().any(|(_, cb)| cb.contains("eli:s:"))
        })
        .count()
}

fn telegram_config() -> tddy_daemon::config::DaemonConfig {
    let mut cfg = tddy_daemon::config::DaemonConfig::default();
    cfg.telegram = Some(tddy_daemon::config::TelegramConfig {
        enabled: true,
        bot_token: "x".to_string(),
        chat_ids: vec![AUTHORIZED_CHAT],
    });
    cfg
}

/// `telegram_single_chat_two_sessions_second_prompt_is_queued_or_deferred`
#[tokio::test]
async fn telegram_single_chat_two_sessions_second_prompt_is_queued_or_deferred() {
    let mut watcher = TelegramSessionWatcher::new();
    let cfg = telegram_config();
    let mem = InMemoryTelegramSender::new();
    let sid_a = "01900000-0000-7000-8000-0000000000aa";
    let sid_b = "01900000-0000-7000-8000-0000000000bb";

    let msg_a = select_elicitation_message("Question from session A", "X", "Y");
    let msg_b = select_elicitation_message("Question from session B", "P", "Q");

    watcher
        .on_server_message(&cfg, &mem, sid_a, &msg_a)
        .await
        .unwrap();
    watcher
        .on_server_message(&cfg, &mem, sid_b, &msg_b)
        .await
        .unwrap();

    let primary_keyboards = count_eli_s_primary_keyboards(&mem, AUTHORIZED_CHAT);
    assert!(
        primary_keyboards <= 1,
        "with two sessions in the same chat, at most one primary `eli:s:` inline keyboard may be visible; \
         additional prompts must be queued, collapsed, or deferred (got {primary_keyboards} full keyboards) — recorded={:?}",
        mem.recorded_with_keyboards()
    );
}

/// `telegram_active_session_token_routes_plain_text_answer_correctly`
#[tokio::test]
async fn telegram_active_session_token_routes_plain_text_answer_correctly() {
    let tmp = tempfile::tempdir().unwrap();
    let sender = Arc::new(InMemoryTelegramSender::new());
    let harness =
        TelegramSessionControlHarness::new(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf(), sender);

    let sid_active = "01900000-0000-7000-8000-0000000000aa";
    harness.register_elicitation_surface_request(AUTHORIZED_CHAT, sid_active.to_string());
    assert_eq!(
        harness.active_elicitation_session_for_chat(AUTHORIZED_CHAT),
        Some(sid_active.to_string()),
        "plain-text and command-based answers must resolve to the active session for the chat; \
         the harness must expose the active elicitation session id once coordination is wired"
    );
}

/// `telegram_callback_for_non_active_session_is_rejected_or_ignored_per_policy`
#[tokio::test]
async fn telegram_callback_for_non_active_session_is_rejected_or_ignored_per_policy() {
    let tmp = tempfile::tempdir().unwrap();
    let sender = Arc::new(InMemoryTelegramSender::new());
    let harness =
        TelegramSessionControlHarness::new(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf(), sender);

    let sid_active = "01900000-0000-7000-8000-0000000000aa";
    let sid_other = "01900000-0000-7000-8000-0000000000bb";
    harness.register_elicitation_surface_request(AUTHORIZED_CHAT, sid_active.to_string());
    harness.register_elicitation_surface_request(AUTHORIZED_CHAT, sid_other.to_string());
    assert!(
        harness.elicitation_callback_permitted(AUTHORIZED_CHAT, sid_active),
        "callback for the active session must be permitted"
    );
    assert!(
        !harness.elicitation_callback_permitted(AUTHORIZED_CHAT, sid_other),
        "callback encoded for session {sid_other} must be rejected or ignored while {sid_active} owns the active token"
    );
}

/// `telegram_active_token_transfers_when_session_completes_elicitation`
#[tokio::test]
async fn telegram_active_token_transfers_when_session_completes_elicitation() {
    let tmp = tempfile::tempdir().unwrap();
    let sender = Arc::new(InMemoryTelegramSender::new());
    let mut harness =
        TelegramSessionControlHarness::new(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf(), sender);

    let sid_a = "01900000-0000-7000-8000-0000000000aa";
    let sid_b = "01900000-0000-7000-8000-0000000000bb";

    harness.register_elicitation_surface_request(AUTHORIZED_CHAT, sid_a.to_string());
    harness.register_elicitation_surface_request(AUTHORIZED_CHAT, sid_b.to_string());

    let next = harness.advance_after_elicitation_completion(AUTHORIZED_CHAT, sid_a);
    assert_eq!(
        next.as_deref(),
        Some(sid_b),
        "when session A completes its elicitation gate, session B (queued for the same chat) must become active"
    );
}

/// `telegram_regression_single_session_elicitation_still_works`
#[tokio::test]
async fn telegram_regression_single_session_elicitation_still_works() {
    let tmp = tempfile::tempdir().unwrap();
    let sender = Arc::new(InMemoryTelegramSender::new());
    let harness =
        TelegramSessionControlHarness::new(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf(), sender);

    let only_sid = "01900000-0000-7000-8000-0000000000cc";
    harness.register_elicitation_surface_request(AUTHORIZED_CHAT, only_sid.to_string());
    assert_eq!(
        harness.active_elicitation_session_for_chat(AUTHORIZED_CHAT),
        Some(only_sid.to_string()),
        "single-session Telegram elicitation must still surface exactly one active token for the chat"
    );
}
