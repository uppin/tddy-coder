//! Acceptance tests (RED): Telegram MultiSelect shortcuts (`Choose none`, `Choose recommended`).
//!
//! Grounded in PRD Testing Plan — outbound keyboards + recommended metadata wiring.

use tddy_daemon::telegram_notifier::{
    InMemoryTelegramSender, InlineKeyboardRows, TelegramSessionWatcher,
};
use tddy_daemon::telegram_session_control::parse_elicitation_multi_select_shortcut;
use tddy_service::gen::app_mode_proto::Variant;
use tddy_service::gen::server_message::Event;
use tddy_service::gen::{
    AppModeMultiSelect, AppModeProto, ClarificationQuestionProto, ModeChanged, QuestionOptionProto,
    ServerMessage,
};

const AUTHORIZED_CHAT: i64 = 424_242;

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

fn multi_select_mode_changed(recommended_other: &str) -> ServerMessage {
    ServerMessage {
        event: Some(Event::ModeChanged(ModeChanged {
            mode: Some(AppModeProto {
                variant: Some(Variant::MultiSelect(AppModeMultiSelect {
                    question: Some(ClarificationQuestionProto {
                        header: "Clarify".into(),
                        question: "Pick any combination".into(),
                        options: vec![
                            QuestionOptionProto {
                                label: "Option A".into(),
                                description: String::new(),
                            },
                            QuestionOptionProto {
                                label: "Option B".into(),
                                description: String::new(),
                            },
                        ],
                        multi_select: true,
                        allow_other: true,
                        recommended_other: recommended_other.to_string(),
                    }),
                    question_index: 0,
                    total_questions: 1,
                })),
            }),
        })),
    }
}

fn keyboards_for_chat(sender: &InMemoryTelegramSender, chat_id: i64) -> Vec<InlineKeyboardRows> {
    sender
        .recorded_with_keyboards()
        .into_iter()
        .filter(|(cid, _, rows)| *cid == chat_id && !rows.is_empty())
        .map(|(_, _, rows)| rows)
        .collect()
}

fn assert_all_callbacks_within_telegram_limit(rows: &InlineKeyboardRows) {
    for row in rows {
        for (_label, cb) in row {
            assert!(
                cb.len() <= 64,
                "Telegram inline callback_data must be <= 64 bytes; got {} bytes on {:?}",
                cb.len(),
                cb
            );
        }
    }
}

/// PRD: synthetic MultiSelect ModeChanged produces outbound shortcut buttons + compact callbacks.
#[tokio::test]
async fn telegram_multi_select_shortcuts_emit_expected_callbacks() {
    let mut watcher = TelegramSessionWatcher::new();
    let cfg = telegram_config();
    let mem = InMemoryTelegramSender::new();
    let sid = "01900000-0000-7000-8000-0000000000aa";
    let msg = multi_select_mode_changed("recommended aggregate text");

    watcher.bind_telegram_tracked_session_for_chat(AUTHORIZED_CHAT, sid);

    watcher
        .on_server_message(&cfg, &mem, sid, &msg)
        .await
        .expect("watcher accepts ModeChanged");

    let keyboards = keyboards_for_chat(&mem, AUTHORIZED_CHAT);
    assert!(
        !keyboards.is_empty(),
        "MultiSelect elicitation must attach at least one inline keyboard message on the primary token; recorded={:?}",
        mem.recorded_with_keyboards()
    );

    let flat: Vec<(String, String)> = keyboards.iter().flatten().flatten().cloned().collect();

    let choose_none = flat
        .iter()
        .find(|(lab, cb)| lab.to_lowercase().contains("choose none") && cb.starts_with("eli:mn:"));
    assert!(
        choose_none.is_some(),
        "expected Choose none button with compact `eli:mn:` prefix; buttons={flat:?}"
    );

    let choose_rec = flat
        .iter()
        .find(|(lab, cb)| lab.to_lowercase().contains("recommended") && cb.starts_with("eli:mr:"));
    assert!(
        choose_rec.is_some(),
        "when recommended_other is set, expected Choose recommended with `eli:mr:` prefix; buttons={flat:?}"
    );

    for rows in &keyboards {
        assert_all_callbacks_within_telegram_limit(rows);
    }
}

/// PRD: inbound Choose-none shortcuts must decode for the same compact wire encoding as outbound.
#[test]
fn telegram_choose_none_submits_empty_multi_via_presenter() {
    let sid = "01900000-0000-7000-8000-0000000000aa";
    let cb = tddy_daemon::telegram_multi_select_shortcuts::compose_choose_none_callback(sid, 0);
    assert!(
        parse_elicitation_multi_select_shortcut(&cb).is_some(),
        "`eli:mn:` payloads must parse so GREEN can invoke answer_clarification_multi_select_localhost with empty indices; cb={cb:?}"
    );
}

/// PRD: omit Choose recommended when metadata absent; emit when recommended_other populated.
#[tokio::test]
async fn telegram_choose_recommended_requires_metadata() {
    let cfg = telegram_config();
    let sid = "01900000-0000-7000-8000-0000000000cc";

    let mut watcher_a = TelegramSessionWatcher::new();
    let mem_a = InMemoryTelegramSender::new();
    let without = multi_select_mode_changed("");
    watcher_a
        .on_server_message(&cfg, &mem_a, sid, &without)
        .await
        .unwrap();
    let flat_a: Vec<(String, String)> = keyboards_for_chat(&mem_a, AUTHORIZED_CHAT)
        .iter()
        .flatten()
        .flatten()
        .cloned()
        .collect();
    assert!(
        !flat_a.iter().any(|(_, cb)| cb.starts_with("eli:mr:")),
        "Choose recommended callback must be absent when recommended_other is empty; got {flat_a:?}"
    );

    let mut watcher_b = TelegramSessionWatcher::new();
    let mem_b = InMemoryTelegramSender::new();
    let with_rec = multi_select_mode_changed("Use this recommendation");
    watcher_b.bind_telegram_tracked_session_for_chat(AUTHORIZED_CHAT, sid);
    watcher_b
        .on_server_message(&cfg, &mem_b, sid, &with_rec)
        .await
        .unwrap();
    let flat_b: Vec<(String, String)> = keyboards_for_chat(&mem_b, AUTHORIZED_CHAT)
        .iter()
        .flatten()
        .flatten()
        .cloned()
        .collect();
    assert!(
        flat_b.iter().any(|(_, cb)| cb.starts_with("eli:mr:")),
        "Choose recommended must surface `eli:mr:` when recommended_other is non-empty; got {flat_b:?}"
    );
}
