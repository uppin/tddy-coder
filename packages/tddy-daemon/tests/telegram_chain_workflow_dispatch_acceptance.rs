//! Acceptance: inbound [`tddy_daemon::telegram_bot`] routes `/chain-workflow` like `/start-workflow`.

/// Telegram bot message dispatcher must parse `/chain-workflow` and delegate to
/// [`tddy_daemon::telegram_session_control::TelegramSessionControlHarness::handle_chain_workflow`].
#[test]
fn telegram_bot_rs_dispatches_chain_workflow_command() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/telegram_bot.rs"));
    assert!(
        src.contains("parse_chain_workflow_prompt") && src.contains("handle_chain_workflow"),
        "telegram_bot.rs must route /chain-workflow after parse_chain_workflow_prompt and call handle_chain_workflow"
    );
}
