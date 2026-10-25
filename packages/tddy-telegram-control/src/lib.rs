//! The Telegram control plane: everything that turns a chat into a session front-end on top of the
//! daemon's session lifecycle.
//!
//! - [`telegram_bot`] — the long-polling dispatcher, routing inbound messages and callbacks.
//! - [`telegram_session_control`] — the harness those updates drive: pickers, session start,
//!   elicitation answers, chaining, the session list.
//! - [`telegram_notifier`] — the session watcher that turns presenter events into chat messages.
//! - [`telegram_multi_select_shortcuts`] — the *choose none* / *choose recommended* callbacks.
//! - [`telegram_session_subscriber`] — [`TelegramDaemonHooks`], the daemon's shared Telegram
//!   handles, and the operator channel the session host and the notification bus are wired with.
//!
//! It sits between [`tddy_telegram`] (the transport, and the chat-side state) and
//! [`tddy_session_lifecycle`] (the sessions it controls), because it needs both and each of them
//! would close a cycle by holding it. No session crate names this crate or `tddy-telegram`: they
//! reach Telegram only through `tddy_daemon_kernel::operator_channel::OperatorChannel`, which
//! `tddy-daemon`'s `runtime` installs.
//!
//! [`TelegramDaemonHooks`]: telegram_session_subscriber::TelegramDaemonHooks

pub mod agent_notification_telegram;
pub mod telegram_bot;
pub mod telegram_multi_select_shortcuts;
pub mod telegram_notifier;
pub mod telegram_session_control;
pub mod telegram_session_subscriber;
