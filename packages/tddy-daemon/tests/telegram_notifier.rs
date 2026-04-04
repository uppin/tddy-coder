//! Acceptance tests for Telegram session notifications (mock sender, no network).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::telegram_notifier::{TelegramSender, TelegramSessionWatcher};

#[derive(Default)]
struct MockTelegramSender {
    calls: Arc<std::sync::Mutex<Vec<(i64, String)>>>,
}

impl MockTelegramSender {
    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    fn messages(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(_, t)| t.clone())
            .collect()
    }
}

#[async_trait]
impl TelegramSender for MockTelegramSender {
    async fn send_message(&self, chat_id: i64, text: &str) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push((chat_id, text.to_string()));
        Ok(())
    }
}

fn write_config(yaml: &str) -> DaemonConfig {
    let dir = tempfile::tempdir().unwrap();
    let path: PathBuf = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    DaemonConfig::load(&path).unwrap()
}

fn telegram_enabled_config() -> DaemonConfig {
    write_config(
        r#"
telegram:
  enabled: true
  bot_token: "test-token"
  chat_ids: [424242]
"#,
    )
}

fn telegram_disabled_config() -> DaemonConfig {
    write_config(
        r#"
telegram:
  enabled: false
  bot_token: "test-token"
  chat_ids: [424242]
"#,
    )
}

#[tokio::test]
async fn telegram_config_disabled_skips_notifier() {
    let mut watcher = TelegramSessionWatcher::new();
    let config = telegram_disabled_config();
    let mock = MockTelegramSender::default();
    let sid = "018f1234-5678-7abc-8def-123456789abc";
    watcher
        .on_metadata_tick(&config, &mock, sid, "active", true)
        .await
        .unwrap();
    assert_eq!(
        mock.call_count(),
        0,
        "Telegram disabled → zero send_message calls"
    );
}

#[tokio::test]
async fn status_transition_triggers_single_telegram_message_mock() {
    let mut watcher = TelegramSessionWatcher::new();
    let config = telegram_enabled_config();
    let mock = MockTelegramSender::default();
    let sid = "018f1234-5678-7abc-8def-123456789abc";
    watcher
        .on_metadata_tick(&config, &mock, sid, "active", true)
        .await
        .unwrap();
    watcher
        .on_metadata_tick(&config, &mock, sid, "paused", true)
        .await
        .unwrap();
    assert_eq!(
        mock.call_count(),
        1,
        "exactly one Telegram send per distinct status transition"
    );
    let joined = mock.messages().join("\n");
    assert!(
        joined.contains("018f1234-5678"),
        "payload must include two-segment label; got: {joined:?}"
    );
}

#[tokio::test]
async fn terminal_session_not_spammed() {
    let mut watcher = TelegramSessionWatcher::new();
    let config = telegram_enabled_config();
    let mock = MockTelegramSender::default();
    let sid = "018f1234-5678-7abc-8def-123456789abc";
    watcher
        .on_metadata_tick(&config, &mock, sid, "active", true)
        .await
        .unwrap();
    watcher
        .on_metadata_tick(&config, &mock, sid, "completed", true)
        .await
        .unwrap();
    watcher
        .on_metadata_tick(&config, &mock, sid, "completed", true)
        .await
        .unwrap();
    assert_eq!(
        mock.call_count(),
        1,
        "baseline does not notify; one send when entering terminal; no send when terminal status repeats"
    );
}
