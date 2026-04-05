//! Integration acceptance tests: Telegram inbound control plane → session + changeset + presenter inputs.
//! Integration tests for [`tddy_daemon::telegram_session_control`] (harness + sender recording).

use std::sync::Arc;

use tddy_daemon::telegram_notifier::InMemoryTelegramSender;
use tddy_daemon::telegram_session_control::{
    drain_outbound_messages, map_elicitation_callback_to_presenter_input,
    read_changeset_routing_snapshot, StartWorkflowCommand, TelegramCallback,
    TelegramSessionControlHarness, WorkflowTransitionKind,
};

const AUTHORIZED_CHAT: i64 = 424_242;
const UNAUTHORIZED_CHAT: i64 = 999_001;

fn harness_with_sender(
    allowed: Vec<i64>,
    sessions_base: std::path::PathBuf,
) -> (TelegramSessionControlHarness, Arc<InMemoryTelegramSender>) {
    let sender = Arc::new(InMemoryTelegramSender::new());
    let h = TelegramSessionControlHarness::new(allowed, sessions_base, sender.clone());
    (h, sender)
}

/// `telegram_start_workflow_presents_recipe_keyboard_and_creates_session`
#[tokio::test]
async fn telegram_start_workflow_presents_recipe_keyboard_and_creates_session() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut harness, sender) =
        harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let cmd = StartWorkflowCommand {
        chat_id: AUTHORIZED_CHAT,
        user_id: 77,
        prompt: "implement Telegram session control".to_string(),
    };
    let outcome = harness
        .handle_start_workflow(cmd)
        .await
        .expect("handler should return Ok in harness");

    assert!(
        !outcome.session_id.is_empty(),
        "start-workflow must create a daemon-backed session id"
    );

    let sent = drain_outbound_messages(&sender, AUTHORIZED_CHAT);
    let labels: Vec<Vec<String>> = sent
        .iter()
        .flat_map(|m| m.inline_keyboard_labels.clone())
        .collect();
    assert!(
        labels
            .iter()
            .flatten()
            .any(|l| l.contains("tdd-small") || l.contains("recipe")),
        "must present recipe/agent inline keyboard; sent={sent:?}"
    );
}

/// `telegram_recipe_callback_persists_changeset_recipe_and_demo_options`
#[tokio::test]
async fn telegram_recipe_callback_persists_changeset_recipe_and_demo_options() {
    let tmp = tempfile::tempdir().unwrap();
    let session_dir = tmp.path().join("sess-1");
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(session_dir.join("changeset.yaml"), "recipe: placeholder\n").unwrap();

    let (mut harness, _sender) =
        harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());
    let cb = TelegramCallback {
        chat_id: AUTHORIZED_CHAT,
        user_id: 77,
        callback_data: "recipe:tdd-small|demo_options:{run:true}".to_string(),
    };
    harness
        .handle_recipe_callback(&session_dir, cb)
        .await
        .expect("recipe callback");

    let snap = read_changeset_routing_snapshot(&session_dir).expect("read changeset.yaml");
    assert_eq!(
        snap.recipe.as_deref(),
        Some("tdd-small"),
        "changeset must record selected recipe"
    );
    assert!(
        snap.demo_options.is_some(),
        "demo_options from Telegram must persist; got {snap:?}"
    );
}

/// `telegram_plan_content_delivered_and_approval_callback_advances_workflow`
#[tokio::test]
async fn telegram_plan_content_delivered_and_approval_callback_advances_workflow() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut harness, sender) =
        harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    const GOLDEN_PLAN: &str = "PLAN SECTION A\n---\nPLAN SECTION B\n";
    let approval = TelegramCallback {
        chat_id: AUTHORIZED_CHAT,
        user_id: 77,
        callback_data: "plan_review:approve".to_string(),
    };

    let (chunks, transition) = harness
        .handle_plan_review_phase("sess-plan-1", GOLDEN_PLAN, approval)
        .await
        .expect("plan review phase");

    let joined = chunks.join("");
    assert_eq!(
        joined, GOLDEN_PLAN,
        "full plan text must be delivered (chunked); joined={joined:?}"
    );

    assert!(
        sender.recorded().iter().any(|(cid, text)| {
            *cid == AUTHORIZED_CHAT && text.contains("PLAN SECTION") && text.contains("(continued)")
        }),
        "Telegram sends must include continuation markers when chunked; recorded={:?}",
        sender.recorded()
    );

    assert_eq!(transition, WorkflowTransitionKind::PlanReviewApproved);
}

/// `telegram_elicitation_choice_mapped_to_presenter_expected_input`
#[tokio::test]
async fn telegram_elicitation_choice_mapped_to_presenter_expected_input() {
    let payload = map_elicitation_callback_to_presenter_input("elicitation:single|opt-a");
    assert_eq!(
        payload.bytes,
        b"\x00single\x00opt-a".to_vec(),
        "single-select Telegram callback must match web presenter input encoding"
    );
}

/// `telegram_unauthorized_chat_cannot_control_session`
#[tokio::test]
async fn telegram_unauthorized_chat_cannot_control_session() {
    let tmp = tempfile::tempdir().unwrap();
    let (harness, sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let cmd = StartWorkflowCommand {
        chat_id: UNAUTHORIZED_CHAT,
        user_id: 1,
        prompt: "pwn".to_string(),
    };
    let denial = harness
        .handle_start_workflow_unauthorized(cmd)
        .await
        .expect("unauthorized handler");

    let msg = denial.expect("unauthorized chat must receive explicit denial message");
    assert!(
        msg.text.to_lowercase().contains("denied")
            || msg.text.to_lowercase().contains("not authorized"),
        "denial text must be explicit; got {:?}",
        msg.text
    );
    assert!(
        drain_outbound_messages(&sender, UNAUTHORIZED_CHAT)
            .iter()
            .any(|m| m.text == msg.text),
        "denial must be sent to the same chat"
    );
}
