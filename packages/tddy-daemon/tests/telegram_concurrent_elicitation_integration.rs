//! Acceptance tests: concurrent Telegram sessions sharing one chat — single active elicitation token.
//!
//! PRD: `Telegram: concurrent sessions with single visible question` (Testing Plan §2).

use std::sync::Arc;

use tddy_daemon::telegram_notifier::{InMemoryTelegramSender, TelegramSessionWatcher};
use tddy_daemon::telegram_session_control::TelegramSessionControlHarness;
use tddy_service::gen::app_mode_proto::Variant;
use tddy_service::gen::server_message::Event;
use tddy_service::gen::{
    AppModeMultiSelect, AppModeProto, AppModeRunning, AppModeSelect, ClarificationQuestionProto,
    ModeChanged, QuestionOptionProto, ServerMessage,
};

const AUTHORIZED_CHAT: i64 = 424_242;

fn running_mode_message() -> ServerMessage {
    ServerMessage {
        event: Some(Event::ModeChanged(ModeChanged {
            mode: Some(AppModeProto {
                variant: Some(Variant::Running(AppModeRunning {})),
            }),
        })),
    }
}

fn multi_select_elicitation_message(question: &str, recommended_other: &str) -> ServerMessage {
    ServerMessage {
        event: Some(Event::ModeChanged(ModeChanged {
            mode: Some(AppModeProto {
                variant: Some(Variant::MultiSelect(AppModeMultiSelect {
                    question: Some(ClarificationQuestionProto {
                        header: "Clarify".into(),
                        question: question.into(),
                        options: vec![
                            QuestionOptionProto {
                                label: "X".into(),
                                description: String::new(),
                            },
                            QuestionOptionProto {
                                label: "Y".into(),
                                description: String::new(),
                            },
                        ],
                        multi_select: true,
                        allow_other: true,
                        recommended_other: recommended_other.into(),
                    }),
                    question_index: 0,
                    total_questions: 1,
                })),
            }),
        })),
    }
}

/// Outbound messages whose keyboards expose MultiSelect shortcut (`Choose none`).
fn count_primary_multi_shortcut_keyboards(sender: &InMemoryTelegramSender, chat_id: i64) -> usize {
    sender
        .recorded_with_keyboards()
        .iter()
        .filter(|(cid, _, rows)| {
            *cid == chat_id
                && rows
                    .iter()
                    .flatten()
                    .any(|(_, cb)| cb.starts_with("eli:mn:"))
        })
        .count()
}

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
    tddy_daemon::config::DaemonConfig {
        telegram: Some(tddy_daemon::config::TelegramConfig {
            enabled: true,
            bot_token: "x".to_string(),
            chat_ids: vec![AUTHORIZED_CHAT],
        }),
        ..Default::default()
    }
}

/// When the head session leaves elicitation (e.g. answered on web), the queue advances and the
/// promoted session is re-notified with a primary `eli:s:` keyboard.
#[tokio::test]
async fn telegram_queue_advances_when_head_leaves_elicitation_without_telegram() {
    let mut watcher = TelegramSessionWatcher::new();
    let cfg = telegram_config();
    let mem = InMemoryTelegramSender::new();
    let sid_a = "01900000-0000-7000-8000-0000000000aa";
    let sid_b = "01900000-0000-7000-8000-0000000000bb";

    watcher.bind_telegram_tracked_session_for_chat(AUTHORIZED_CHAT, sid_a);

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

    assert_eq!(
        count_eli_s_primary_keyboards(&mem, AUTHORIZED_CHAT),
        1,
        "only session A should have a primary clarification keyboard initially"
    );

    let running_a = running_mode_message();
    watcher
        .on_server_message(&cfg, &mem, sid_a, &running_a)
        .await
        .unwrap();

    let primary_after = count_eli_s_primary_keyboards(&mem, AUTHORIZED_CHAT);
    assert!(
        primary_after >= 2,
        "after A leaves elicitation, B should be re-sent with a primary keyboard; expected >= 2 eli:s keyboards, got {primary_after} — recorded={:?}",
        mem.recorded_with_keyboards()
    );
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

#[tokio::test]
async fn telegram_wrong_tracked_outbound_elicitation_does_not_block_tracked_session_fifo() {
    let mut watcher = TelegramSessionWatcher::new();
    let cfg = telegram_config();
    let mem = InMemoryTelegramSender::new();
    let sid_a = "01900000-0000-7000-8000-0000000000aa";
    let sid_b = "01900000-0000-7000-8000-0000000000bb";

    watcher.bind_telegram_tracked_session_for_chat(AUTHORIZED_CHAT, sid_b);

    let msg_a = select_elicitation_message("Question from session A", "X", "Y");
    let msg_b = select_elicitation_message("Question from tracked session B", "P", "Q");

    watcher
        .on_server_message(&cfg, &mem, sid_a, &msg_a)
        .await
        .unwrap();
    watcher
        .on_server_message(&cfg, &mem, sid_b, &msg_b)
        .await
        .unwrap();

    let queued_snippet = "elicitation queued";
    let any_queued = mem
        .recorded()
        .iter()
        .any(|(_, t)| t.contains(queued_snippet));
    assert!(
        !any_queued,
        "expected tracked session B to own the primary elicitation surface — recorded={:?}",
        mem.recorded()
    );
    assert!(
        count_eli_s_primary_keyboards(&mem, AUTHORIZED_CHAT) >= 1,
        "tracked session must receive eli:s clarification keyboard — recorded={:?}",
        mem.recorded_with_keyboards()
    );
}

/// PRD MultiSelect: concurrent shortcut keyboards obey single primary interactive surface per chat.
#[tokio::test]
async fn telegram_concurrent_queue_unchanged_guarantees() {
    let mut watcher = TelegramSessionWatcher::new();
    let cfg = telegram_config();
    let mem = InMemoryTelegramSender::new();
    let sid_a = "01900000-0000-7000-8000-0000000000aa";
    let sid_b = "01900000-0000-7000-8000-0000000000bb";

    watcher.bind_telegram_tracked_session_for_chat(AUTHORIZED_CHAT, sid_a);

    let msg_a = multi_select_elicitation_message("Question from session A", "");
    let msg_b = multi_select_elicitation_message("Question from session B", "");

    watcher
        .on_server_message(&cfg, &mem, sid_a, &msg_a)
        .await
        .unwrap();
    watcher
        .on_server_message(&cfg, &mem, sid_b, &msg_b)
        .await
        .unwrap();

    assert_eq!(
        count_primary_multi_shortcut_keyboards(&mem, AUTHORIZED_CHAT),
        1,
        "FIFO head must own the lone primary MultiSelect shortcut keyboard for this chat"
    );

    let running_a = running_mode_message();
    watcher
        .on_server_message(&cfg, &mem, sid_a, &running_a)
        .await
        .unwrap();

    let total_shortcut_surfaces = count_primary_multi_shortcut_keyboards(&mem, AUTHORIZED_CHAT);
    assert!(
        total_shortcut_surfaces >= 2,
        "after head clears elicitation, promoted MultiSelect session must emit another primary shortcut keyboard; expected>=2 got {total_shortcut_surfaces}"
    );
}
