//! Acceptance tests: Telegram ↔ GitHub OAuth link and OS-user mapping (PRD Testing Plan).
//!
//! Integration tests for Telegram ↔ GitHub binding and OS-user resolution.

use tddy_daemon::config::{DaemonConfig, UserMapping};
use tddy_daemon::telegram_github_link::{
    complete_telegram_link_via_stub_exchange, resolved_os_user_for_telegram_workflow,
    TelegramGithubMappingStore, TelegramOAuthStateSigner,
};
use tddy_daemon::telegram_notifier::InMemoryTelegramSender;
use tddy_daemon::telegram_session_control::{StartWorkflowCommand, TelegramSessionControlHarness};
use tddy_github::{GitHubUser, StubGitHubProvider};

const AUTHORIZED_CHAT: i64 = 424_242;

/// `telegram_oauth_state_roundtrip_binds_telegram_user`
#[test]
fn telegram_oauth_state_roundtrip_binds_telegram_user() {
    let signer = TelegramOAuthStateSigner::new(b"01234567890123456789012345678901");
    let uid = 99_887_766u64;
    let state = signer
        .encode_telegram_user(uid)
        .expect("encode must succeed for a signed state");
    assert_eq!(
        signer
            .verify_and_extract_telegram_user(&state)
            .expect("verify valid state"),
        uid
    );
    let mut tampered = state.clone();
    tampered.pop();
    assert!(
        signer.verify_and_extract_telegram_user(&tampered).is_err(),
        "tampered state must fail validation"
    );
}

/// `telegram_link_persists_github_login_across_restart` (reload = new store instance, same path)
#[test]
fn telegram_link_persists_github_login_across_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("telegram_github_mapping.json");
    let telegram_user_id = 77u64;
    let github_login = "octocat";

    {
        let mut store = TelegramGithubMappingStore::open(&path).expect("open store");
        store
            .put(telegram_user_id, github_login)
            .expect("persist mapping");
    }

    let store2 = TelegramGithubMappingStore::open(&path).expect("reopen after simulated restart");
    assert_eq!(
        store2.get_github_login(telegram_user_id).as_deref(),
        Some(github_login),
        "lookup after reload must return the same github_login"
    );
}

/// `telegram_start_workflow_uses_os_user_from_github_mapping`
#[test]
fn telegram_start_workflow_uses_os_user_from_github_mapping() {
    let mut config = DaemonConfig::default();
    config.users.push(UserMapping {
        github_user: "mapped-gh".to_string(),
        os_user: "mapped-os".to_string(),
    });

    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("map.json");
    let mut store = TelegramGithubMappingStore::open(&path).expect("open");
    store
        .put(42, "mapped-gh")
        .expect("link telegram user to github login");

    assert_eq!(
        resolved_os_user_for_telegram_workflow(&config, &store, 42).as_deref(),
        Some("mapped-os"),
        "spawn and session paths must use the OS user from daemon users: mapping for the linked GitHub login"
    );
}

/// `telegram_unlinked_user_receives_explicit_error`
#[tokio::test]
async fn telegram_unlinked_user_receives_explicit_error() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sender = std::sync::Arc::new(InMemoryTelegramSender::new());
    let mapping_path = tmp.path().join("telegram_github_mapping.json");
    let mut harness = TelegramSessionControlHarness::with_telegram_github_link(
        vec![AUTHORIZED_CHAT],
        tmp.path().to_path_buf(),
        sender.clone(),
        mapping_path,
    );

    let cmd = StartWorkflowCommand {
        chat_id: AUTHORIZED_CHAT,
        user_id: 101,
        prompt: "feature without github link".to_string(),
    };

    match harness.handle_start_workflow(cmd).await {
        Err(e) => {
            let s = e.to_string().to_lowercase();
            assert!(
                s.contains("link") || s.contains("github") || s.contains("auth"),
                "unlinked user must get an explicit instruction to connect GitHub; got: {e}"
            );
        }
        Ok(outcome) => {
            panic!(
                "expected start-workflow to fail for a Telegram user without a completed GitHub link; got Ok (session_id={}, silent success is forbidden)",
                outcome.session_id
            );
        }
    }
}

/// `stub_github_exchange_maps_stub_login_for_telegram`
#[test]
fn stub_github_exchange_maps_stub_login_for_telegram() {
    let callback = "http://127.0.0.1:9/auth/callback";
    let stub = StubGitHubProvider::new_with_callback(callback, "stub-client-id");
    let login = "stubby-user";
    stub.register_code(
        "stub-code-1",
        GitHubUser {
            id: 42,
            login: login.to_string(),
            avatar_url: format!("https://github.com/{login}.png"),
            name: login.to_string(),
        },
    );

    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("map.json");
    let mut store = TelegramGithubMappingStore::open(&path).expect("open");
    let telegram_user_id = 500u64;

    let resolved_login = complete_telegram_link_via_stub_exchange(
        &stub,
        "stub-code-1",
        telegram_user_id,
        &mut store,
    )
    .expect("stub exchange must complete linking like production OAuth");

    assert_eq!(resolved_login, login);
    assert_eq!(
        store.get_github_login(telegram_user_id).as_deref(),
        Some(login)
    );

    let mut config = DaemonConfig::default();
    config.users.push(UserMapping {
        github_user: login.to_string(),
        os_user: "stub-os".to_string(),
    });
    assert_eq!(
        resolved_os_user_for_telegram_workflow(&config, &store, telegram_user_id).as_deref(),
        Some("stub-os")
    );
}
