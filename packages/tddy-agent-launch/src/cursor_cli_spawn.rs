//! What Telegram's launcher still calls of the Cursor hand-built launch: the hooks installer and the
//! chat mint (`chat`). Every other cursor-cli start, resume and re-attach is the generic CLI launch
//! (`opencode_cli_spawn::start_cli_harness_session`), which renders `cursor.yaml`.
// TODO(cli-dispatch-collapse): `#agent-tools 18/19` `telegram-spec-launch` deletes this module with
// `chat::{install_cursor_hooks_in_worktree, mint_cursor_chat_id}`
// (docs/dev/todo/2026-11-03-cursor-hand-built-launch-paths.md).

pub use crate::chat;
pub use chat::*;
