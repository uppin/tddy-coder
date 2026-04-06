//! Integration acceptance tests: Telegram inbound control plane → session + changeset + presenter inputs.
//! Integration tests for [`tddy_daemon::telegram_session_control`] (harness + sender recording).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tddy_core::changeset::{read_changeset, write_changeset, BranchWorktreeIntent, Changeset};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF;
use tddy_daemon::config::{AllowedAgent, DaemonConfig};
use tddy_daemon::project_storage::{self, ProjectData};
use tddy_daemon::telegram_notifier::InMemoryTelegramSender;
use tddy_daemon::telegram_session_control::{
    collect_outbound_messages, map_elicitation_callback_to_presenter_input,
    read_changeset_routing_snapshot, StartWorkflowCommand, TelegramCallback,
    TelegramSessionControlHarness, TelegramWorkflowSpawn, WorkflowTransitionKind,
    SESSIONS_PAGE_SIZE,
};

const AUTHORIZED_CHAT: i64 = 424_242;
const UNAUTHORIZED_CHAT: i64 = 999_001;

fn harness_with_sender(
    allowed: Vec<i64>,
    sessions_base: std::path::PathBuf,
) -> (
    TelegramSessionControlHarness<InMemoryTelegramSender>,
    Arc<InMemoryTelegramSender>,
) {
    let sender = Arc::new(InMemoryTelegramSender::new());
    let h = TelegramSessionControlHarness::new(allowed, sessions_base, sender.clone());
    (h, sender)
}

fn harness_with_workflow_projects(
    allowed: Vec<i64>,
    sessions_base: std::path::PathBuf,
    projects_dir: std::path::PathBuf,
) -> (
    TelegramSessionControlHarness<InMemoryTelegramSender>,
    Arc<InMemoryTelegramSender>,
) {
    let sender = Arc::new(InMemoryTelegramSender::new());
    let workflow_spawn = Arc::new(TelegramWorkflowSpawn {
        config: Arc::new(DaemonConfig::default()),
        spawn_client: None,
        os_user: "n/a".to_string(),
        projects_dir_override: Some(projects_dir),
        telegram_hooks: None,
        child_grpc_by_session: Arc::new(Mutex::new(HashMap::new())),
        elicitation_select_options: Arc::new(Mutex::new(HashMap::new())),
        pending_elicitation_other: Arc::new(Mutex::new(HashMap::new())),
    });
    let h = TelegramSessionControlHarness::with_workflow_spawn(
        allowed,
        sessions_base,
        sender.clone(),
        Some(workflow_spawn),
    );
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

    let session_dir = unified_session_dir_path(tmp.path(), &outcome.session_id);
    assert!(
        session_dir.exists(),
        "handle_start_workflow must create session directory under sessions_base/sessions/"
    );

    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    let labels: Vec<Vec<String>> = sent
        .iter()
        .flat_map(|m| {
            m.inline_keyboard
                .iter()
                .map(|row| row.iter().map(|(lab, _)| lab.clone()).collect())
        })
        .collect();
    assert!(
        labels
            .iter()
            .flatten()
            .any(|l| l.contains("tdd") || l.contains("recipe")),
        "must present recipe/agent inline keyboard; sent={sent:?}"
    );

    let snap = read_changeset_routing_snapshot(&session_dir).expect("read changeset.yaml");
    assert_eq!(
        snap.initial_prompt.as_deref(),
        Some("implement Telegram session control"),
        "Telegram prompt after /start-workflow must persist as changeset initial_prompt so the child workflow does not block on feature input"
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
        callback_data: "recipe:tdd|demo_options:{run:true}".to_string(),
    };
    harness
        .handle_recipe_callback(&session_dir, cb)
        .await
        .expect("recipe callback");

    let snap = read_changeset_routing_snapshot(&session_dir).expect("read changeset.yaml");
    assert_eq!(
        snap.recipe.as_deref(),
        Some("tdd"),
        "changeset must record selected recipe (CLI name)"
    );
    let demo = snap
        .demo_options
        .expect("demo_options from Telegram must persist");
    let run_val = demo.get("run").expect("demo_options should have 'run' key");
    assert_eq!(
        run_val.as_bool(),
        Some(true),
        "demo_options run should be true; got {demo:?}"
    );
}

/// Recipe selection after `/start-workflow` must keep `initial_prompt` alongside `recipe`.
#[tokio::test]
async fn telegram_recipe_callback_keeps_initial_prompt_from_start_workflow() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut harness, _sender) =
        harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let cmd = StartWorkflowCommand {
        chat_id: AUTHORIZED_CHAT,
        user_id: 77,
        prompt: "fix notification delivery".to_string(),
    };
    let outcome = harness
        .handle_start_workflow(cmd)
        .await
        .expect("start workflow");

    let session_dir = unified_session_dir_path(tmp.path(), &outcome.session_id);
    let cb = TelegramCallback {
        chat_id: AUTHORIZED_CHAT,
        user_id: 77,
        callback_data: format!("recipe:tdd|session:{}", outcome.session_id),
    };
    harness
        .handle_recipe_callback(&session_dir, cb)
        .await
        .expect("recipe callback");

    let snap = read_changeset_routing_snapshot(&session_dir).expect("read changeset.yaml");
    assert_eq!(snap.recipe.as_deref(), Some("tdd"));
    assert_eq!(
        snap.initial_prompt.as_deref(),
        Some("fix notification delivery")
    );
}

/// `recipe:more` sends a follow-up message with additional recipe buttons (`mr:` callbacks).
#[tokio::test]
async fn telegram_more_recipes_sends_second_keyboard() {
    let tmp = tempfile::tempdir().unwrap();
    let session_id = "019d5c8f-71b0-79d1-8492-cfaf08fc6ab2";
    let session_dir = tmp.path().join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    let (mut harness, sender) =
        harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());
    let cb = TelegramCallback {
        chat_id: AUTHORIZED_CHAT,
        user_id: 77,
        callback_data: format!("recipe:more|session:{session_id}"),
    };
    harness
        .handle_recipe_callback(&session_dir, cb)
        .await
        .expect("more recipes callback");

    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    let last = sent.last().expect("must send more recipes message");
    assert!(
        last.text.to_lowercase().contains("more recipes"),
        "expected more-recipes intro; got {:?}",
        last.text
    );
    let mr_buttons: Vec<&str> = last
        .inline_keyboard
        .iter()
        .flatten()
        .filter(|(_, data)| data.starts_with("mr:"))
        .map(|(label, _)| label.as_str())
        .collect();
    assert_eq!(
        mr_buttons.len(),
        tddy_daemon::telegram_session_control::RECIPE_MORE_PAGE.len(),
        "expected one button per RECIPE_MORE_PAGE entry; got {mr_buttons:?}"
    );
}

/// Compact `mr:` callback persists the mapped recipe name (e.g. grill-me).
#[tokio::test]
async fn telegram_mr_recipe_callback_persists_recipe_name() {
    let tmp = tempfile::tempdir().unwrap();
    let session_id = "019d5c8f-71b0-79d1-8492-cfaf08fc6ab2";
    let session_dir = tmp.path().join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    let (mut harness, _sender) =
        harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());
    let cb = TelegramCallback {
        chat_id: AUTHORIZED_CHAT,
        user_id: 77,
        callback_data: format!("mr:3|{session_id}"),
    };
    harness
        .handle_recipe_callback(&session_dir, cb)
        .await
        .expect("mr recipe callback");

    let snap = read_changeset_routing_snapshot(&session_dir).expect("read changeset.yaml");
    assert_eq!(
        snap.recipe.as_deref(),
        Some("grill-me"),
        "mr:3 must map to grill-me"
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

/// `telegram_unauthorized_start_workflow_is_silent`
#[tokio::test]
async fn telegram_unauthorized_start_workflow_is_silent() {
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

    assert!(
        denial.is_none(),
        "unauthorized chat must not get a captured denial message (silent ignore for multi-daemon)"
    );
    assert!(
        collect_outbound_messages(&sender, UNAUTHORIZED_CHAT).is_empty(),
        "unauthorized chat must receive no Telegram traffic; got {:?}",
        collect_outbound_messages(&sender, UNAUTHORIZED_CHAT)
    );
}

/// `telegram_authorized_chat_returns_none_from_unauthorized_handler`
#[tokio::test]
async fn telegram_authorized_chat_returns_none_from_unauthorized_handler() {
    let tmp = tempfile::tempdir().unwrap();
    let (harness, _sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());
    let cmd = StartWorkflowCommand {
        chat_id: AUTHORIZED_CHAT,
        user_id: 1,
        prompt: "should return None".to_string(),
    };
    let result = harness
        .handle_start_workflow_unauthorized(cmd)
        .await
        .expect("handler should succeed for authorized chat");
    assert!(
        result.is_none(),
        "authorized chat calling unauthorized handler must get None (caller should use handle_start_workflow)"
    );
}

// -------------------------------------------------------------------------
// /sessions — session listing with pagination
// -------------------------------------------------------------------------

/// Helper: create N fake session directories with `.session.yaml` under `{base}/sessions/`.
fn create_fake_sessions(base: &std::path::Path, count: usize) -> Vec<String> {
    let sessions_dir = base.join("sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let mut ids = Vec::with_capacity(count);
    for i in 0..count {
        let id = format!("sess-{:04}", i);
        let session_dir = sessions_dir.join(&id);
        std::fs::create_dir_all(&session_dir).unwrap();
        let metadata = tddy_core::SessionMetadata {
            session_id: id.clone(),
            project_id: "proj-1".to_string(),
            created_at: format!("2026-04-05T08:{:02}:00Z", i),
            updated_at: format!("2026-04-05T08:{:02}:30Z", i),
            status: "running".to_string(),
            repo_path: Some("/tmp/repo".to_string()),
            pid: None,
            tool: Some("tddy-coder".to_string()),
            livekit_room: None,
            pending_elicitation: false,
        };
        tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();
        ids.push(id);
    }
    ids
}

/// `/sessions` with fewer than 10 sessions shows all and no "More" button.
#[tokio::test]
async fn telegram_list_sessions_shows_all_when_fewer_than_page_size() {
    let tmp = tempfile::tempdir().unwrap();
    let session_ids = create_fake_sessions(tmp.path(), 3);
    let (harness, sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let page = harness
        .handle_list_sessions(AUTHORIZED_CHAT, 0)
        .await
        .expect("handle_list_sessions should succeed");

    assert_eq!(
        page.entries.len(),
        3,
        "page must contain all 3 sessions; got {}",
        page.entries.len()
    );
    assert!(
        !page.has_more,
        "has_more must be false when sessions fit in one page"
    );

    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    assert!(
        !sent.is_empty(),
        "handler must send at least one Telegram message"
    );
    for id in &session_ids {
        assert!(
            sent.iter()
                .any(|m| m.text.contains(id) || m.text.contains(&id[..9.min(id.len())])),
            "session {id} must appear in sent messages; sent={sent:?}"
        );
    }

    let has_enter_button = sent.iter().any(|m| {
        m.inline_keyboard
            .iter()
            .flatten()
            .any(|(l, _)| l.to_lowercase().contains("enter"))
    });
    assert!(
        has_enter_button,
        "each session entry must have an 'Enter' button; sent={sent:?}"
    );

    let has_delete_button = sent.iter().any(|m| {
        m.inline_keyboard
            .iter()
            .flatten()
            .any(|(l, _)| l.to_lowercase().contains("delete"))
    });
    assert!(
        has_delete_button,
        "each session entry must have a 'Delete' button; sent={sent:?}"
    );

    let has_more_button = sent.iter().any(|m| {
        m.inline_keyboard
            .iter()
            .flatten()
            .any(|(l, _)| l.to_lowercase().contains("more"))
    });
    assert!(
        !has_more_button,
        "no 'More' button when all sessions fit on one page; sent={sent:?}"
    );
}

/// `/sessions` with more than 10 sessions paginates and shows "More" button.
#[tokio::test]
async fn telegram_list_sessions_paginates_with_more_button() {
    let tmp = tempfile::tempdir().unwrap();
    let _session_ids = create_fake_sessions(tmp.path(), 15);
    let (harness, sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let page = harness
        .handle_list_sessions(AUTHORIZED_CHAT, 0)
        .await
        .expect("handle_list_sessions should succeed");

    assert_eq!(
        page.entries.len(),
        SESSIONS_PAGE_SIZE,
        "first page must contain exactly {SESSIONS_PAGE_SIZE} sessions; got {}",
        page.entries.len()
    );
    assert!(
        page.has_more,
        "has_more must be true when more sessions exist beyond the page"
    );
    assert_eq!(
        page.next_offset, SESSIONS_PAGE_SIZE,
        "next_offset must be {SESSIONS_PAGE_SIZE}; got {}",
        page.next_offset
    );

    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    let has_more_button = sent.iter().any(|m| {
        m.inline_keyboard
            .iter()
            .flatten()
            .any(|(l, _)| l.to_lowercase().contains("more"))
    });
    assert!(
        has_more_button,
        "must show 'More' button when additional sessions exist; sent={sent:?}"
    );
}

/// Second page of `/sessions` via offset returns remaining sessions.
#[tokio::test]
async fn telegram_list_sessions_second_page_returns_remaining() {
    let tmp = tempfile::tempdir().unwrap();
    let _session_ids = create_fake_sessions(tmp.path(), 15);
    let (harness, _sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let page2 = harness
        .handle_list_sessions(AUTHORIZED_CHAT, SESSIONS_PAGE_SIZE)
        .await
        .expect("handle_list_sessions page 2 should succeed");

    assert_eq!(
        page2.entries.len(),
        5,
        "second page must contain remaining 5 sessions; got {}",
        page2.entries.len()
    );
    assert!(!page2.has_more, "has_more must be false on the last page");
}

/// `/sessions` with zero sessions returns empty page.
#[tokio::test]
async fn telegram_list_sessions_empty_shows_no_sessions_message() {
    let tmp = tempfile::tempdir().unwrap();
    let (harness, sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let page = harness
        .handle_list_sessions(AUTHORIZED_CHAT, 0)
        .await
        .expect("handle_list_sessions should succeed for empty list");

    assert!(
        page.entries.is_empty(),
        "page must be empty when no sessions exist"
    );
    assert!(
        !page.has_more,
        "has_more must be false when no sessions exist"
    );

    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    assert!(
        !sent.is_empty(),
        "handler must send a message even when no sessions exist (e.g. 'No sessions found')"
    );
}

/// Unauthorized chat cannot list sessions.
#[tokio::test]
async fn telegram_list_sessions_unauthorized_is_denied() {
    let tmp = tempfile::tempdir().unwrap();
    let (harness, _sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let result = harness.handle_list_sessions(UNAUTHORIZED_CHAT, 0).await;

    assert!(
        result.is_err(),
        "unauthorized chat must be denied listing sessions"
    );
}

// -------------------------------------------------------------------------
// /delete — session deletion
// -------------------------------------------------------------------------

/// `/delete <session_id>` removes the session and sends confirmation.
#[tokio::test]
async fn telegram_delete_session_removes_and_confirms() {
    let tmp = tempfile::tempdir().unwrap();
    let ids = create_fake_sessions(tmp.path(), 1);
    let target_id = &ids[0];
    let session_dir = tmp.path().join("sessions").join(target_id);
    assert!(
        session_dir.exists(),
        "setup: session dir must exist before delete"
    );

    let (harness, sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let outcome = harness
        .handle_delete_session(AUTHORIZED_CHAT, target_id)
        .await
        .expect("handle_delete_session should succeed");

    assert_eq!(
        outcome.session_id, *target_id,
        "outcome must reference the deleted session"
    );
    assert!(
        !session_dir.exists(),
        "session directory must be removed after deletion"
    );

    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    assert!(
        sent.iter()
            .any(|m| m.text.to_lowercase().contains("deleted")
                || m.text.to_lowercase().contains("removed")),
        "confirmation message must indicate deletion; sent={sent:?}"
    );
}

/// `/delete` with non-existent session id returns an error.
#[tokio::test]
async fn telegram_delete_nonexistent_session_returns_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (harness, _sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let result = harness
        .handle_delete_session(AUTHORIZED_CHAT, "no-such-session")
        .await;

    assert!(
        result.is_err(),
        "deleting a non-existent session must return an error"
    );
}

/// Unauthorized chat cannot delete sessions.
#[tokio::test]
async fn telegram_delete_session_unauthorized_is_denied() {
    let tmp = tempfile::tempdir().unwrap();
    let ids = create_fake_sessions(tmp.path(), 1);
    let (harness, _sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let result = harness
        .handle_delete_session(UNAUTHORIZED_CHAT, &ids[0])
        .await;

    assert!(
        result.is_err(),
        "unauthorized chat must be denied session deletion"
    );
    let session_dir = tmp.path().join("sessions").join(&ids[0]);
    assert!(
        session_dir.exists(),
        "session directory must not be removed by unauthorized request"
    );
}

// -------------------------------------------------------------------------
// Branch/worktree intent (Telegram /start-workflow)
// -------------------------------------------------------------------------

/// After recipe selection, intent keyboard exposes both intent callbacks.
#[tokio::test]
async fn telegram_intent_pick_shown_after_recipe() {
    let tmp = tempfile::tempdir().unwrap();
    let session_id = "019d5c8f-71b0-79d1-8492-cfaf08fc6ab2";
    let session_dir = unified_session_dir_path(tmp.path(), session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let mut cs = Changeset::default();
    cs.recipe = Some("tdd".to_string());
    write_changeset(&session_dir, &cs).expect("write changeset");

    let (mut harness, sender) =
        harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());
    let cb = TelegramCallback {
        chat_id: AUTHORIZED_CHAT,
        user_id: 77,
        callback_data: format!("recipe:tdd|session:{session_id}"),
    };
    harness
        .handle_recipe_callback(&session_dir, cb)
        .await
        .expect("recipe callback");
    harness
        .send_intent_pick_keyboard(AUTHORIZED_CHAT, session_id)
        .await
        .expect("intent keyboard");

    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    let intent_msg = sent
        .iter()
        .find(|m| m.text.contains("branch/worktree intent"))
        .expect("intent prompt message");
    let callbacks: Vec<&str> = intent_msg
        .inline_keyboard
        .iter()
        .flatten()
        .map(|(_, d)| d.as_str())
        .collect();
    let nb = format!("intent:nb|s:{session_id}");
    let ws = format!("intent:ws|s:{session_id}");
    assert!(
        callbacks.iter().any(|d| *d == nb.as_str()),
        "expected nb intent callback; got {callbacks:?}"
    );
    assert!(
        callbacks.iter().any(|d| *d == ws.as_str()),
        "expected ws intent callback; got {callbacks:?}"
    );
}

#[tokio::test]
async fn telegram_intent_persists_to_changeset() {
    let tmp = tempfile::tempdir().unwrap();
    let session_id = "019d5c8f-71b0-79d1-8492-cfaf08fc6ab2";
    let session_dir = unified_session_dir_path(tmp.path(), session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let mut cs = Changeset::default();
    cs.recipe = Some("tdd".to_string());
    write_changeset(&session_dir, &cs).expect("write changeset");

    let (harness, _sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());
    harness
        .handle_telegram_intent_callback(
            AUTHORIZED_CHAT,
            BranchWorktreeIntent::NewBranchFromBase,
            session_id,
        )
        .await
        .expect("intent callback");

    let snap = read_changeset_routing_snapshot(&session_dir).expect("read changeset.yaml");
    assert_eq!(
        snap.workflow
            .as_ref()
            .and_then(|w| w.branch_worktree_intent),
        Some(BranchWorktreeIntent::NewBranchFromBase)
    );
}

#[tokio::test]
async fn telegram_intent_then_project_pick_continues_flow() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("proj-registry");
    std::fs::create_dir_all(&projects_dir).unwrap();
    let repo_path = tmp.path().join("fake-repo");
    std::fs::create_dir_all(&repo_path).unwrap();
    project_storage::write_projects(
        &projects_dir,
        &[ProjectData {
            project_id: "proj-a".to_string(),
            name: "Project A".to_string(),
            git_url: "https://example.invalid/a.git".to_string(),
            main_repo_path: repo_path.to_string_lossy().to_string(),
            main_branch_ref: None,
            host_repo_paths: HashMap::new(),
        }],
    )
    .expect("write projects");

    let session_id = "019d5c8f-71b0-79d1-8492-cfaf08fc6ab2";
    let session_dir = unified_session_dir_path(tmp.path(), session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let mut cs = Changeset::default();
    cs.recipe = Some("tdd".to_string());
    write_changeset(&session_dir, &cs).expect("write changeset");

    let (harness, sender) = harness_with_workflow_projects(
        vec![AUTHORIZED_CHAT],
        tmp.path().to_path_buf(),
        projects_dir,
    );
    harness
        .handle_telegram_intent_callback(
            AUTHORIZED_CHAT,
            BranchWorktreeIntent::WorkOnSelectedBranch,
            session_id,
        )
        .await
        .expect("intent callback");

    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    let proj_msg = sent
        .iter()
        .find(|m| m.text.contains("Choose a project"))
        .expect("project pick message");
    let tp = format!("tp:0|s:{session_id}");
    assert!(
        proj_msg
            .inline_keyboard
            .iter()
            .flatten()
            .any(|(_, d)| d == &tp),
        "expected project callback {tp}; keyboards={:?}",
        proj_msg.inline_keyboard
    );
}

/// Branch selection lists at most 10 remotes per page; more than 10 requires a **More…** row (`tbm:`).
#[tokio::test]
async fn telegram_branch_pick_shows_more_when_more_than_ten_remote_branches() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("proj-registry");
    std::fs::create_dir_all(&projects_dir).unwrap();
    let clone = init_repo_with_n_origin_branches(tmp.path(), 11);
    project_storage::write_projects(
        &projects_dir,
        &[ProjectData {
            project_id: "proj-a".to_string(),
            name: "Project A".to_string(),
            git_url: "https://example.invalid/a.git".to_string(),
            main_repo_path: clone.to_string_lossy().to_string(),
            main_branch_ref: None,
            host_repo_paths: HashMap::new(),
        }],
    )
    .expect("write projects");

    let session_id = "019d5c8f-71b0-79d1-8492-cfaf08fc6ab2";
    let session_dir = unified_session_dir_path(tmp.path(), session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let mut cs = Changeset::default();
    cs.recipe = Some("tdd".to_string());
    write_changeset(&session_dir, &cs).expect("write changeset");

    let (harness, sender) = harness_with_workflow_projects(
        vec![AUTHORIZED_CHAT],
        tmp.path().to_path_buf(),
        projects_dir,
    );
    harness
        .handle_telegram_intent_callback(
            AUTHORIZED_CHAT,
            BranchWorktreeIntent::NewBranchFromBase,
            session_id,
        )
        .await
        .expect("intent callback");
    harness
        .handle_telegram_project_callback(AUTHORIZED_CHAT, 0, session_id)
        .await
        .expect("project callback");

    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    let branch_msg = sent
        .iter()
        .find(|m| m.text.contains("Choose integration base"))
        .expect("branch pick message");
    let has_more = branch_msg
        .inline_keyboard
        .iter()
        .flatten()
        .any(|(l, d)| d.starts_with("tbm:") || l == "More…");
    assert!(
        has_more,
        "must show More… when >10 origin branches; keyboards={:?}",
        branch_msg.inline_keyboard
    );
}

#[tokio::test]
async fn telegram_branch_pick_no_more_when_at_most_ten_remote_branches() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("proj-registry");
    std::fs::create_dir_all(&projects_dir).unwrap();
    let clone = init_repo_with_n_origin_branches(tmp.path(), 10);
    project_storage::write_projects(
        &projects_dir,
        &[ProjectData {
            project_id: "proj-a".to_string(),
            name: "Project A".to_string(),
            git_url: "https://example.invalid/a.git".to_string(),
            main_repo_path: clone.to_string_lossy().to_string(),
            main_branch_ref: None,
            host_repo_paths: HashMap::new(),
        }],
    )
    .expect("write projects");

    let session_id = "019d5c8f-71b0-79d1-8492-cfaf08fc6ab3";
    let session_dir = unified_session_dir_path(tmp.path(), session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let mut cs = Changeset::default();
    cs.recipe = Some("tdd".to_string());
    write_changeset(&session_dir, &cs).expect("write changeset");

    let (harness, sender) = harness_with_workflow_projects(
        vec![AUTHORIZED_CHAT],
        tmp.path().to_path_buf(),
        projects_dir,
    );
    harness
        .handle_telegram_intent_callback(
            AUTHORIZED_CHAT,
            BranchWorktreeIntent::NewBranchFromBase,
            session_id,
        )
        .await
        .expect("intent callback");
    harness
        .handle_telegram_project_callback(AUTHORIZED_CHAT, 0, session_id)
        .await
        .expect("project callback");

    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    let branch_msg = sent
        .iter()
        .find(|m| m.text.contains("Choose integration base"))
        .expect("branch pick message");
    let has_more = branch_msg
        .inline_keyboard
        .iter()
        .flatten()
        .any(|(l, d)| d.starts_with("tbm:") || l == "More…");
    assert!(
        !has_more,
        "no More… when at most 10 origin branches; keyboards={:?}",
        branch_msg.inline_keyboard
    );
}

// -------------------------------------------------------------------------
// Enter workflow — connect to existing session
// -------------------------------------------------------------------------

/// "Enter" button connects to a session and shows workflow state.
#[tokio::test]
async fn telegram_enter_session_shows_workflow_state() {
    let tmp = tempfile::tempdir().unwrap();
    let ids = create_fake_sessions(tmp.path(), 1);
    let target_id = &ids[0];

    let (harness, sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let outcome = harness
        .handle_enter_session(AUTHORIZED_CHAT, target_id)
        .await
        .expect("handle_enter_session should succeed");

    assert_eq!(
        outcome.session_id, *target_id,
        "outcome must reference the entered session"
    );
    assert!(
        !outcome.messages.is_empty(),
        "entering a session must produce at least one message showing workflow state"
    );

    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    assert!(
        !sent.is_empty(),
        "handler must send messages to Telegram when entering a session"
    );
}

/// "Enter" on non-existent session returns error.
#[tokio::test]
async fn telegram_enter_nonexistent_session_returns_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (harness, _sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let result = harness
        .handle_enter_session(AUTHORIZED_CHAT, "no-such-session")
        .await;

    assert!(
        result.is_err(),
        "entering a non-existent session must return an error"
    );
}

/// Unauthorized chat cannot enter sessions.
#[tokio::test]
async fn telegram_enter_session_unauthorized_is_denied() {
    let tmp = tempfile::tempdir().unwrap();
    let ids = create_fake_sessions(tmp.path(), 1);
    let (harness, _sender) = harness_with_sender(vec![AUTHORIZED_CHAT], tmp.path().to_path_buf());

    let result = harness
        .handle_enter_session(UNAUTHORIZED_CHAT, &ids[0])
        .await;

    assert!(
        result.is_err(),
        "unauthorized chat must be denied entering a session"
    );
}

// -------------------------------------------------------------------------
// Branch callback — work_on_selected_branch must set selected_branch_to_work_on
// -------------------------------------------------------------------------

/// Bare remote + clone with `master` and `n - 1` `feature/more-*` branches (total `n` `origin/*` refs).
fn init_repo_with_n_origin_branches(root: &std::path::Path, n: usize) -> std::path::PathBuf {
    assert!(n >= 1);
    let bare = root.join("origin-many.git");
    let clone = root.join("work-many");
    git(&["init", "--bare", bare.to_str().unwrap()], root);
    git(&["clone", bare.to_str().unwrap(), "work-many"], root);
    std::fs::write(clone.join("README.md"), "# init\n").unwrap();
    git(&["config", "user.email", "test@test.com"], &clone);
    git(&["config", "user.name", "test"], &clone);
    git(&["add", "README.md"], &clone);
    git(&["commit", "-m", "init"], &clone);
    git(&["branch", "-M", "master"], &clone);
    git(&["push", "-u", "origin", "master"], &clone);
    for i in 0..n.saturating_sub(1) {
        let name = format!("feature/more-{i}");
        git(&["checkout", "-b", &name], &clone);
        std::fs::write(clone.join("README.md"), format!("# {i}\n")).unwrap();
        git(&["add", "README.md"], &clone);
        git(&["commit", "-m", &format!("more {i}")], &clone);
        git(&["push", "-u", "origin", &name], &clone);
    }
    git(&["checkout", "master"], &clone);
    clone
}

fn git(args: &[&str], cwd: &std::path::Path) -> std::process::Output {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("run git {:?} in {}: {e}", args, cwd.display()));
    assert!(
        out.status.success(),
        "git {:?} failed in {}:\nstdout={}\nstderr={}",
        args,
        cwd.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn init_repo_with_remote_branch(root: &std::path::Path) -> std::path::PathBuf {
    let bare = root.join("origin.git");
    let clone = root.join("work");

    git(&["init", "--bare", bare.to_str().unwrap()], root);
    git(&["clone", bare.to_str().unwrap(), "work"], root);
    std::fs::write(clone.join("README.md"), "# init\n").unwrap();
    git(&["config", "user.email", "test@test.com"], &clone);
    git(&["config", "user.name", "test"], &clone);
    git(&["add", "README.md"], &clone);
    git(&["commit", "-m", "init"], &clone);
    git(&["branch", "-M", "master"], &clone);
    git(&["push", "-u", "origin", "master"], &clone);

    git(&["checkout", "-b", "feature/test-branch"], &clone);
    std::fs::write(clone.join("README.md"), "# feature\n").unwrap();
    git(&["add", "README.md"], &clone);
    git(&["commit", "-m", "feature work"], &clone);
    git(&["push", "-u", "origin", "feature/test-branch"], &clone);

    git(&["checkout", "master"], &clone);

    clone
}

fn harness_with_workflow_projects_and_agents(
    allowed: Vec<i64>,
    sessions_base: std::path::PathBuf,
    projects_dir: std::path::PathBuf,
    agents: Vec<AllowedAgent>,
) -> (
    TelegramSessionControlHarness<InMemoryTelegramSender>,
    Arc<InMemoryTelegramSender>,
) {
    let sender = Arc::new(InMemoryTelegramSender::new());
    let mut config = DaemonConfig::default();
    config.allowed_agents = agents;
    let workflow_spawn = Arc::new(TelegramWorkflowSpawn {
        config: Arc::new(config),
        spawn_client: None,
        os_user: "n/a".to_string(),
        projects_dir_override: Some(projects_dir),
        telegram_hooks: None,
        child_grpc_by_session: Arc::new(Mutex::new(HashMap::new())),
        elicitation_select_options: Arc::new(Mutex::new(HashMap::new())),
        pending_elicitation_other: Arc::new(Mutex::new(HashMap::new())),
    });
    let h = TelegramSessionControlHarness::with_workflow_spawn(
        allowed,
        sessions_base,
        sender.clone(),
        Some(workflow_spawn),
    );
    (h, sender)
}

#[tokio::test]
async fn telegram_branch_callback_work_on_selected_sets_selected_branch_to_work_on() {
    let tmp = tempfile::tempdir().unwrap();
    let sessions_base = tmp.path().join("sessions");
    std::fs::create_dir_all(&sessions_base).unwrap();

    let clone = init_repo_with_remote_branch(tmp.path());

    let projects_dir = tmp.path().join("proj-registry");
    std::fs::create_dir_all(&projects_dir).unwrap();
    project_storage::write_projects(
        &projects_dir,
        &[ProjectData {
            project_id: "proj-a".to_string(),
            name: "Project A".to_string(),
            git_url: "https://example.invalid/a.git".to_string(),
            main_repo_path: clone.to_string_lossy().to_string(),
            main_branch_ref: None,
            host_repo_paths: HashMap::new(),
        }],
    )
    .expect("write projects");

    let session_id = "019d6392-3cff-0001-aaaa-000000000001";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    let mut cs = Changeset::default();
    cs.recipe = Some("merge-pr".to_string());
    write_changeset(&session_dir, &cs).expect("write changeset");

    let dummy_agent = AllowedAgent {
        id: "claude".to_string(),
        label: Some("Claude".to_string()),
    };
    let (harness, _sender) = harness_with_workflow_projects_and_agents(
        vec![AUTHORIZED_CHAT],
        sessions_base,
        projects_dir,
        vec![dummy_agent],
    );

    harness
        .handle_telegram_intent_callback(
            AUTHORIZED_CHAT,
            BranchWorktreeIntent::WorkOnSelectedBranch,
            session_id,
        )
        .await
        .expect("intent callback");

    // branch_idx=1 selects the first remote branch (origin/feature/test-branch);
    // the agent picker keyboard is shown instead of spawning (one allowed agent configured).
    harness
        .handle_telegram_branch_callback(AUTHORIZED_CHAT, 1, 0, 0, session_id)
        .await
        .expect("branch callback");

    let cs = read_changeset(&session_dir).expect("read changeset after branch callback");

    assert!(
        cs.workflow
            .as_ref()
            .and_then(|w| w.selected_branch_to_work_on.as_deref())
            .is_some(),
        "when intent is work_on_selected_branch, handle_telegram_branch_callback must set \
         workflow.selected_branch_to_work_on; changeset: worktree_integration_base_ref={:?}, \
         workflow={:?}",
        cs.worktree_integration_base_ref,
        cs.workflow
    );

    let selected = cs
        .workflow
        .as_ref()
        .and_then(|w| w.selected_branch_to_work_on.as_deref())
        .unwrap();
    assert!(
        selected.contains("feature/test-branch"),
        "selected_branch_to_work_on should contain the branch name; got {:?}",
        selected
    );
}

#[tokio::test]
async fn telegram_branch_callback_new_branch_from_base_sets_selected_integration_base_only() {
    let tmp = tempfile::tempdir().unwrap();
    let sessions_base = tmp.path().join("sessions");
    std::fs::create_dir_all(&sessions_base).unwrap();

    let clone = init_repo_with_remote_branch(tmp.path());

    let projects_dir = tmp.path().join("proj-registry");
    std::fs::create_dir_all(&projects_dir).unwrap();
    project_storage::write_projects(
        &projects_dir,
        &[ProjectData {
            project_id: "proj-a".to_string(),
            name: "Project A".to_string(),
            git_url: "https://example.invalid/a.git".to_string(),
            main_repo_path: clone.to_string_lossy().to_string(),
            main_branch_ref: None,
            host_repo_paths: HashMap::new(),
        }],
    )
    .expect("write projects");

    let session_id = "a9f84aa1-8d9b-4c2e-9f00-000000000001";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    let mut cs = Changeset::default();
    cs.recipe = Some("tdd".to_string());
    write_changeset(&session_dir, &cs).expect("write changeset");

    let dummy_agent = AllowedAgent {
        id: "claude".to_string(),
        label: Some("Claude".to_string()),
    };
    let (harness, _sender) = harness_with_workflow_projects_and_agents(
        vec![AUTHORIZED_CHAT],
        sessions_base,
        projects_dir,
        vec![dummy_agent],
    );

    harness
        .handle_telegram_intent_callback(
            AUTHORIZED_CHAT,
            BranchWorktreeIntent::NewBranchFromBase,
            session_id,
        )
        .await
        .expect("intent callback");

    harness
        .handle_telegram_branch_callback(AUTHORIZED_CHAT, 0, 0, 0, session_id)
        .await
        .expect("branch callback default integration base");

    let cs = read_changeset(&session_dir).expect("read changeset after branch callback");
    let wf = cs.workflow.as_ref().expect("workflow block");
    assert_eq!(
        wf.selected_integration_base_ref.as_deref(),
        Some(DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF),
        "default branch row must persist project integration base ref"
    );
    assert!(
        wf.new_branch_name.is_none(),
        "new_branch_name comes from the LLM (plan or bugfix analyze submit), not Telegram"
    );
}
