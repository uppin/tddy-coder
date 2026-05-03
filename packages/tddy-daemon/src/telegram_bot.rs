//! Inbound Telegram updates: long-polling dispatcher for session control commands and callbacks.

use std::sync::Arc;

use teloxide::dispatching::Dispatcher;
use teloxide::payloads::AnswerCallbackQuerySetters;
use teloxide::prelude::*;
use teloxide::requests::Requester;
use teloxide::types::{CallbackQuery, CallbackQueryId, ChatId, Message};
use tokio::sync::Mutex;

use crate::telegram_notifier::TeloxideSender;
use crate::telegram_session_control::{
    parse_answer_multi_command, parse_answer_text_command, parse_chain_workflow_prompt,
    parse_delete_command, parse_document_review_callback, parse_elicitation_multi_select_shortcut,
    parse_elicitation_other_callback, parse_elicitation_select_callback,
    parse_recipe_callback_session_dir, parse_session_control_callback, parse_sessions_command,
    parse_start_workflow_prompt, parse_submit_feature_command, parse_telegram_agent_callback,
    parse_telegram_branch_callback, parse_telegram_branch_more_callback,
    parse_telegram_chain_parent_callback, parse_telegram_intent_callback,
    parse_telegram_project_callback, ChainWorkflowCommand, SessionControlCallback,
    StartWorkflowCommand, TelegramCallback, TelegramSessionControlHarness,
    CB_TELEGRAM_CHAIN_PARENT,
};
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};

type Harness = Arc<Mutex<TelegramSessionControlHarness<TeloxideSender>>>;

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// **Phase 2 session chaining**: live `tcp:` parent-picker callbacks are wired (see [`maybe_dispatch_tcp_chain_parent_callback`]).
pub fn session_chaining_phase2_live_tcp_dispatch_ready() -> bool {
    true
}

fn chain_phase2_tcp_dispatch_marker(child_id: Option<&str>) {
    let id = child_id.unwrap_or("");
    log::debug!(
        target: "tddy_daemon::telegram_bot",
        "chain tcp marker child_id={id}"
    );
}

/// Emits the same diagnostic path as the live `tcp:` parent-picker (for tests / logging capture).
pub fn chain_phase2_tcp_dispatch_marker_probe() {
    chain_phase2_tcp_dispatch_marker(None);
}

/// Try to handle `tcp:<parent_idx>|s:<child_session_id>` parent-picker callbacks (long-polling).
///
/// Returns `Some(Ok(()))` when the callback was recognized as a chain-parent pick. Returns `None` when
/// `data` is not a chain parent callback.
///
/// Resolves the child session directory and calls [`TelegramSessionControlHarness::handle_chain_parent_callback`].
async fn maybe_dispatch_tcp_chain_parent_callback(
    bot: &Bot,
    harness: &Harness,
    chat_id: i64,
    user_id: u64,
    data: &str,
    qid: CallbackQueryId,
) -> Option<HandlerResult> {
    if !data.starts_with(CB_TELEGRAM_CHAIN_PARENT) {
        return None;
    }
    let Some((_parent_idx, child_id)) = parse_telegram_chain_parent_callback(data) else {
        let _ = bot.answer_callback_query(qid.clone()).await;
        let _ = bot
            .send_message(
                ChatId(chat_id),
                telegram_workflow_error_message(
                    "Invalid chain parent callback payload (could not parse `tcp:` line)."
                        .to_string(),
                ),
            )
            .await;
        return Some(Ok(()));
    };
    chain_phase2_tcp_dispatch_marker(Some(child_id.as_str()));

    if let Err(e) = validate_session_id_segment(&child_id) {
        let _ = bot.answer_callback_query(qid.clone()).await;
        let _ = bot
            .send_message(
                ChatId(chat_id),
                telegram_workflow_error_message(format!(
                    "Invalid child session id in `tcp:` callback: {}",
                    e.message()
                )),
            )
            .await;
        return Some(Ok(()));
    }

    if workflow_callback_gate_authorized(bot, harness, chat_id, qid.clone()).await {
        return Some(Ok(()));
    }

    let child_dir = {
        let h = harness.lock().await;
        unified_session_dir_path(h.sessions_base(), &child_id)
    };
    let mut h = harness.lock().await;
    let cb = TelegramCallback {
        chat_id,
        user_id,
        callback_data: data.to_string(),
    };
    match h.handle_chain_parent_callback(&child_dir, cb).await {
        Ok(()) => {
            let _ = bot.answer_callback_query(qid).await;
            Some(Ok(()))
        }
        Err(e) => {
            let _ = bot.answer_callback_query(qid.clone()).await;
            let _ = bot
                .send_message(
                    ChatId(chat_id),
                    telegram_workflow_error_message(format!("{e:#}")),
                )
                .await;
            Some(Ok(()))
        }
    }
}

/// Authorize the chat and ensure `session_id` holds the active elicitation token for interactive
/// surfaces (`eli:s:` / `eli:o:` / `eli:mn:` / `eli:mr:` / document-review). If not, answers `qid` and returns `true` (caller
/// should return `Ok(())`).
async fn authorized_elicitation_surface_gate(
    bot: &Bot,
    harness: &Harness,
    chat_id: i64,
    session_id: &str,
    qid: CallbackQueryId,
    kind: &'static str,
) -> bool {
    let h = harness.lock().await;
    if !h.is_authorized(chat_id) {
        drop(h);
        let _ = bot.answer_callback_query(qid).await;
        return true;
    }
    if !h.elicitation_callback_permitted(chat_id, session_id) {
        log::info!(
            target: "tddy_daemon::telegram_bot",
            "{kind} callback ignored: session not active for chat chat_id={} session_id={}",
            chat_id,
            session_id
        );
        drop(h);
        let _ = bot
            .answer_callback_query(qid)
            .text(
                "That elicitation is not active for this chat. Finish the current prompt or use the web UI.",
            )
            .show_alert(true)
            .await;
        return true;
    }
    false
}

fn telegram_workflow_error_message(detail: String) -> String {
    format!(
        "{detail}\n\nIf `tddy-coder` exited on startup, check the child stderr file next to the daemon logs (e.g. `tmp/logs/child/<session_id>_stderr`)."
    )
}

/// Returns `true` when the chat is not allowlisted — caller should return `Ok(())` after the empty answer.
async fn workflow_callback_gate_authorized(
    bot: &Bot,
    harness: &Harness,
    chat_id: i64,
    qid: CallbackQueryId,
) -> bool {
    let h = harness.lock().await;
    if !h.is_authorized(chat_id) {
        drop(h);
        let _ = bot.answer_callback_query(qid).await;
        return true;
    }
    false
}

/// Run teloxide long-polling until the process terminates or dispatch ends.
pub async fn run_telegram_bot(bot: Bot, harness: Harness) -> anyhow::Result<()> {
    let h_msg = harness.clone();
    let h_cb = harness;

    let handler = dptree::entry()
        .branch(
            Update::filter_message().endpoint(move |b: Bot, msg: Message| {
                let h = h_msg.clone();
                async move { telegram_message_handler(b, h, msg).await }
            }),
        )
        .branch(
            Update::filter_callback_query().endpoint(move |b: Bot, q: CallbackQuery| {
                let h = h_cb.clone();
                async move { telegram_callback_handler(b, h, q).await }
            }),
        );

    Dispatcher::builder(bot, handler).build().dispatch().await;
    Ok(())
}

async fn telegram_message_handler(bot: Bot, harness: Harness, msg: Message) -> HandlerResult {
    let Some(text) = msg.text().map(|s| s.to_string()) else {
        return Ok(());
    };
    let chat_id = msg.chat.id.0;
    let user_id = msg.from.map(|u| u.id.0).unwrap_or(0);

    log::info!(
        target: "tddy_daemon::telegram_bot",
        "{}",
        crate::telegram_tracked_session::format_inbound_message_traffic_log(
            chat_id,
            text.len(),
            None,
        )
    );
    crate::telegram_tracked_session::log_inbound_telegram_message_body(chat_id, &text);

    if !text.trim_start().starts_with('/') {
        let h = harness.lock().await;
        if h.is_authorized(chat_id) {
            match h
                .handle_elicitation_other_followup_plain_message(chat_id, &text)
                .await
            {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(e) => {
                    bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
                    return Ok(());
                }
            }
        }
    }

    if let Some(prompt) = parse_start_workflow_prompt(&text) {
        let cmd = StartWorkflowCommand {
            chat_id,
            user_id,
            prompt,
        };
        let mut h = harness.lock().await;
        if h.is_authorized(chat_id) {
            h.handle_start_workflow(cmd).await?;
        }
        // Not configured for this chat: ignore (multi-daemon — each instance has its own allowlist).
        return Ok(());
    }

    if let Some(prompt) = parse_chain_workflow_prompt(&text) {
        let cmd = ChainWorkflowCommand {
            chat_id,
            user_id,
            prompt,
        };
        let mut h = harness.lock().await;
        if h.is_authorized(chat_id) {
            h.handle_chain_workflow(cmd).await?;
        }
        return Ok(());
    }

    if let Some((session_key, body)) = parse_submit_feature_command(&text) {
        let h = harness.lock().await;
        if !h.is_authorized(chat_id) {
            return Ok(());
        }
        if let Err(e) = h.handle_submit_feature(chat_id, &session_key, &body).await {
            bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
        }
        return Ok(());
    }

    if let Some((session_key, body)) = parse_answer_text_command(&text) {
        let h = harness.lock().await;
        if !h.is_authorized(chat_id) {
            return Ok(());
        }
        if let Err(e) = h
            .handle_answer_text_command(chat_id, &session_key, &body)
            .await
        {
            bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
        }
        return Ok(());
    }

    if let Some((session_key, indices)) = parse_answer_multi_command(&text) {
        let h = harness.lock().await;
        if !h.is_authorized(chat_id) {
            return Ok(());
        }
        if let Err(e) = h
            .handle_answer_multi_command(chat_id, &session_key, &indices)
            .await
        {
            bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
        }
        return Ok(());
    }

    if let Some(offset) = parse_sessions_command(&text) {
        let h = harness.lock().await;
        if !h.is_authorized(chat_id) {
            return Ok(());
        }
        h.handle_list_sessions(chat_id, offset).await?;
        return Ok(());
    }

    if let Some(session_id) = parse_delete_command(&text) {
        let h = harness.lock().await;
        if !h.is_authorized(chat_id) {
            return Ok(());
        }
        match h.handle_delete_session(chat_id, &session_id).await {
            Ok(_) => {}
            Err(e) => {
                bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
            }
        }
        return Ok(());
    }

    Ok(())
}

async fn telegram_callback_handler(bot: Bot, harness: Harness, q: CallbackQuery) -> HandlerResult {
    let Some(data) = q.data else {
        return Ok(());
    };
    let qid = q.id.clone();

    let Some(m) = q.message.as_ref() else {
        log::warn!(
            target: "tddy_daemon::telegram_bot",
            "callback_query has no message (inline_message_id only?); cannot route session/recipe"
        );
        let _ = bot
            .answer_callback_query(qid)
            .text("Open this bot in a private chat and try again (callback had no chat message).")
            .show_alert(true)
            .await;
        return Ok(());
    };
    let chat_id = m.chat().id.0;
    let user_id = q.from.id.0;

    let cb_prefix = data.get(..48).unwrap_or(data.as_str());
    log::info!(
        target: "tddy_daemon::telegram_bot",
        "{}",
        crate::telegram_tracked_session::format_inbound_callback_traffic_log(
            chat_id,
            data.len(),
            None,
            cb_prefix,
        )
    );

    if let Some(res) = maybe_dispatch_tcp_chain_parent_callback(
        &bot,
        &harness,
        chat_id,
        user_id,
        &data,
        qid.clone(),
    )
    .await
    {
        return res;
    }

    if let Some(action) = parse_session_control_callback(&data) {
        let h = harness.lock().await;
        if !h.is_authorized(chat_id) {
            drop(h);
            bot.answer_callback_query(qid).await?;
            return Ok(());
        }
        match action {
            SessionControlCallback::Enter { session_id } => {
                h.handle_enter_session(chat_id, &session_id).await?;
            }
            SessionControlCallback::Delete { session_id } => {
                h.handle_delete_session(chat_id, &session_id).await?;
            }
            SessionControlCallback::More { offset } => {
                h.handle_list_sessions(chat_id, offset).await?;
            }
        }
        bot.answer_callback_query(qid).await?;
        return Ok(());
    }

    if let Some((intent, session_id)) = parse_telegram_intent_callback(&data) {
        if workflow_callback_gate_authorized(&bot, &harness, chat_id, qid.clone()).await {
            return Ok(());
        }
        let _ = bot.answer_callback_query(qid.clone()).await;
        let h = harness.lock().await;
        match h
            .handle_telegram_intent_callback(chat_id, intent, &session_id)
            .await
        {
            Ok(()) => {}
            Err(e) => {
                bot.send_message(
                    ChatId(chat_id),
                    telegram_workflow_error_message(format!("{e:#}")),
                )
                .await?;
            }
        }
        return Ok(());
    }

    if let Some((proj_idx, session_id)) = parse_telegram_project_callback(&data) {
        if workflow_callback_gate_authorized(&bot, &harness, chat_id, qid.clone()).await {
            return Ok(());
        }
        let _ = bot.answer_callback_query(qid.clone()).await;
        let h = harness.lock().await;
        match h
            .handle_telegram_project_callback(chat_id, proj_idx, &session_id)
            .await
        {
            Ok(()) => {}
            Err(e) => {
                bot.send_message(
                    ChatId(chat_id),
                    telegram_workflow_error_message(format!("{e:#}")),
                )
                .await?;
            }
        }
        return Ok(());
    }

    if let Some((next_off, proj_idx, session_id)) = parse_telegram_branch_more_callback(&data) {
        if workflow_callback_gate_authorized(&bot, &harness, chat_id, qid.clone()).await {
            return Ok(());
        }
        let _ = bot.answer_callback_query(qid.clone()).await;
        let h = harness.lock().await;
        match h
            .handle_telegram_branch_more_callback(chat_id, next_off, proj_idx, &session_id)
            .await
        {
            Ok(()) => {}
            Err(e) => {
                bot.send_message(
                    ChatId(chat_id),
                    telegram_workflow_error_message(format!("{e:#}")),
                )
                .await?;
            }
        }
        return Ok(());
    }

    if let Some((branch_idx, list_offset, proj_idx, session_id)) =
        parse_telegram_branch_callback(&data)
    {
        if workflow_callback_gate_authorized(&bot, &harness, chat_id, qid.clone()).await {
            return Ok(());
        }
        let _ = bot.answer_callback_query(qid.clone()).await;
        let h = harness.lock().await;
        match h
            .handle_telegram_branch_callback(
                chat_id,
                branch_idx,
                list_offset,
                proj_idx,
                &session_id,
            )
            .await
        {
            Ok(()) => {}
            Err(e) => {
                bot.send_message(
                    ChatId(chat_id),
                    telegram_workflow_error_message(format!("{e:#}")),
                )
                .await?;
            }
        }
        return Ok(());
    }

    if let Some((agent_idx, proj_idx, session_id)) = parse_telegram_agent_callback(&data) {
        if workflow_callback_gate_authorized(&bot, &harness, chat_id, qid.clone()).await {
            return Ok(());
        }
        let _ = bot.answer_callback_query(qid.clone()).await;
        let h = harness.lock().await;
        match h
            .handle_telegram_agent_callback(chat_id, agent_idx, proj_idx, &session_id)
            .await
        {
            Ok(()) => {}
            Err(e) => {
                bot.send_message(
                    ChatId(chat_id),
                    telegram_workflow_error_message(format!("{e:#}")),
                )
                .await?;
            }
        }
        return Ok(());
    }

    if let Some((action, session_id)) = parse_document_review_callback(&data) {
        if authorized_elicitation_surface_gate(
            &bot,
            &harness,
            chat_id,
            &session_id,
            qid.clone(),
            "document_review",
        )
        .await
        {
            return Ok(());
        }
        let _ = bot.answer_callback_query(qid.clone()).await;
        let h = harness.lock().await;
        if let Err(e) = h
            .handle_document_review_action(chat_id, action, &session_id)
            .await
        {
            bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
        }
        return Ok(());
    }

    if let Some(session_id) = parse_elicitation_other_callback(&data) {
        if authorized_elicitation_surface_gate(
            &bot,
            &harness,
            chat_id,
            &session_id,
            qid.clone(),
            "elicitation_other",
        )
        .await
        {
            return Ok(());
        }
        let _ = bot.answer_callback_query(qid.clone()).await;
        let h = harness.lock().await;
        if let Err(e) = h.handle_elicitation_other(chat_id, &session_id).await {
            bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
        }
        return Ok(());
    }

    if let Some((session_id_shortcut, qi, shortcut_kind)) =
        parse_elicitation_multi_select_shortcut(&data)
    {
        if authorized_elicitation_surface_gate(
            &bot,
            &harness,
            chat_id,
            &session_id_shortcut,
            qid.clone(),
            "elicitation_multi_select_shortcut",
        )
        .await
        {
            return Ok(());
        }
        let _ = bot.answer_callback_query(qid.clone()).await;
        let h = harness.lock().await;
        if let Err(e) = h
            .handle_elicitation_multi_select_shortcut(
                chat_id,
                &session_id_shortcut,
                qi,
                shortcut_kind,
            )
            .await
        {
            bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
        }
        return Ok(());
    }

    if let Some((session_id, option_index)) = parse_elicitation_select_callback(&data) {
        if authorized_elicitation_surface_gate(
            &bot,
            &harness,
            chat_id,
            &session_id,
            qid.clone(),
            "elicitation_select",
        )
        .await
        {
            return Ok(());
        }
        let _ = bot.answer_callback_query(qid.clone()).await;
        let h = harness.lock().await;
        if let Err(e) = h
            .handle_elicitation_select(chat_id, &session_id, option_index)
            .await
        {
            bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
        }
        return Ok(());
    }

    if data.contains("recipe:") || data.starts_with("mr:") {
        if workflow_callback_gate_authorized(&bot, &harness, chat_id, qid.clone()).await {
            return Ok(());
        }
        log::info!(
            target: "tddy_daemon::telegram_bot",
            "recipe callback chat_id={} data_len={}",
            chat_id,
            data.len()
        );
        // Answer immediately so Telegram clears the loading state; feedback is a chat message (toasts are easy to miss).
        let _ = bot.answer_callback_query(qid.clone()).await;

        let base = {
            let h = harness.lock().await;
            h.sessions_base().to_path_buf()
        };
        let Some(session_dir) = parse_recipe_callback_session_dir(&data, &base) else {
            bot.send_message(
                ChatId(chat_id),
                "Could not resolve session from this button (bad or truncated callback_data).",
            )
            .await?;
            return Ok(());
        };
        let skip_project_pick = data.contains("recipe:more");
        let mut h = harness.lock().await;
        let cb = TelegramCallback {
            chat_id,
            user_id,
            callback_data: data,
        };
        match h.handle_recipe_callback(&session_dir, cb).await {
            Ok(()) => {
                bot.send_message(
                    ChatId(chat_id),
                    "Recipe saved: changeset.yaml updated for this session.",
                )
                .await?;
                if !skip_project_pick {
                    let session_id = session_dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();
                    if !session_id.is_empty() {
                        if let Err(e) = h.send_intent_pick_keyboard(chat_id, &session_id).await {
                            bot.send_message(
                                ChatId(chat_id),
                                format!(
                                    "Recipe saved, but intent selection could not be shown:\n{e:#}"
                                ),
                            )
                            .await?;
                        }
                    }
                }
            }
            Err(e) => {
                bot.send_message(ChatId(chat_id), format!("Recipe save failed:\n{e:#}"))
                    .await?;
            }
        }
        return Ok(());
    }

    bot.answer_callback_query(qid).await?;
    Ok(())
}
