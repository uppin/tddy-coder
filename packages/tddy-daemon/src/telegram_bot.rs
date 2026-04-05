//! Inbound Telegram updates: long-polling dispatcher for session control commands and callbacks.

use std::sync::Arc;

use teloxide::dispatching::Dispatcher;
use teloxide::payloads::AnswerCallbackQuerySetters;
use teloxide::prelude::*;
use teloxide::requests::Requester;
use teloxide::types::{CallbackQuery, ChatId, Message};
use tokio::sync::Mutex;

use crate::telegram_notifier::TeloxideSender;
use crate::telegram_session_control::{
    parse_answer_multi_command, parse_answer_text_command, parse_delete_command,
    parse_document_review_callback, parse_elicitation_select_callback,
    parse_recipe_callback_session_dir, parse_session_control_callback, parse_sessions_command,
    parse_start_workflow_prompt, parse_submit_feature_command, parse_telegram_agent_callback,
    parse_telegram_project_callback, SessionControlCallback, StartWorkflowCommand,
    TelegramCallback, TelegramSessionControlHarness,
};

type Harness = Arc<Mutex<TelegramSessionControlHarness<TeloxideSender>>>;

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn telegram_workflow_error_message(detail: String) -> String {
    format!(
        "{detail}\n\nIf `tddy-coder` exited on startup, check the child stderr file next to the daemon logs (e.g. `tmp/logs/child/<session_id>_stderr`)."
    )
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

    if let Some(prompt) = parse_start_workflow_prompt(&text) {
        let cmd = StartWorkflowCommand {
            chat_id,
            user_id,
            prompt,
        };
        let mut h = harness.lock().await;
        if h.is_authorized(chat_id) {
            h.handle_start_workflow(cmd).await?;
        } else {
            h.handle_start_workflow_unauthorized(cmd).await?;
        }
        return Ok(());
    }

    if let Some((session_key, body)) = parse_submit_feature_command(&text) {
        let h = harness.lock().await;
        if h.is_authorized(chat_id) {
            if let Err(e) = h.handle_submit_feature(chat_id, &session_key, &body).await {
                bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
            }
        } else {
            bot.send_message(
                ChatId(chat_id),
                "Access denied: this chat is not authorized to control workflows.",
            )
            .await?;
        }
        return Ok(());
    }

    if let Some((session_key, body)) = parse_answer_text_command(&text) {
        let h = harness.lock().await;
        if h.is_authorized(chat_id) {
            if let Err(e) = h
                .handle_answer_text_command(chat_id, &session_key, &body)
                .await
            {
                bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
            }
        } else {
            bot.send_message(
                ChatId(chat_id),
                "Access denied: this chat is not authorized to control workflows.",
            )
            .await?;
        }
        return Ok(());
    }

    if let Some((session_key, indices)) = parse_answer_multi_command(&text) {
        let h = harness.lock().await;
        if h.is_authorized(chat_id) {
            if let Err(e) = h
                .handle_answer_multi_command(chat_id, &session_key, &indices)
                .await
            {
                bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
            }
        } else {
            bot.send_message(
                ChatId(chat_id),
                "Access denied: this chat is not authorized to control workflows.",
            )
            .await?;
        }
        return Ok(());
    }

    if let Some(offset) = parse_sessions_command(&text) {
        let harness = harness.lock().await;
        harness.handle_list_sessions(chat_id, offset).await?;
        return Ok(());
    }

    if let Some(session_id) = parse_delete_command(&text) {
        let harness = harness.lock().await;
        match harness.handle_delete_session(chat_id, &session_id).await {
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

    if let Some(action) = parse_session_control_callback(&data) {
        let harness = harness.lock().await;
        match action {
            SessionControlCallback::Enter { session_id } => {
                harness.handle_enter_session(chat_id, &session_id).await?;
            }
            SessionControlCallback::Delete { session_id } => {
                harness.handle_delete_session(chat_id, &session_id).await?;
            }
            SessionControlCallback::More { offset } => {
                harness.handle_list_sessions(chat_id, offset).await?;
            }
        }
        bot.answer_callback_query(qid).await?;
        return Ok(());
    }

    if let Some((proj_idx, session_id)) = parse_telegram_project_callback(&data) {
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

    if let Some((agent_idx, proj_idx, session_id)) = parse_telegram_agent_callback(&data) {
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
        let _ = bot.answer_callback_query(qid.clone()).await;
        let h = harness.lock().await;
        if h.is_authorized(chat_id) {
            if let Err(e) = h
                .handle_document_review_action(chat_id, action, &session_id)
                .await
            {
                bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
            }
        } else {
            bot.send_message(
                ChatId(chat_id),
                "Access denied: this chat is not authorized to control workflows.",
            )
            .await?;
        }
        return Ok(());
    }

    if let Some((session_id, option_index)) = parse_elicitation_select_callback(&data) {
        let _ = bot.answer_callback_query(qid.clone()).await;
        let h = harness.lock().await;
        if h.is_authorized(chat_id) {
            if let Err(e) = h
                .handle_elicitation_select(chat_id, &session_id, option_index)
                .await
            {
                bot.send_message(ChatId(chat_id), format!("{e:#}")).await?;
            }
        } else {
            bot.send_message(
                ChatId(chat_id),
                "Access denied: this chat is not authorized to control workflows.",
            )
            .await?;
        }
        return Ok(());
    }

    if data.contains("recipe:") || data.starts_with("mr:") {
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
                        if let Err(e) = h.send_project_pick_after_recipe(chat_id, &session_id).await
                        {
                            bot.send_message(
                                ChatId(chat_id),
                                format!(
                                    "Recipe saved, but project selection could not be shown:\n{e:#}"
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
