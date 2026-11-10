//! The Cursor launch's re-export of `chat` (`parse_created_chat_id`). Every cursor-cli start, resume
//! and re-attach is the generic CLI launch (`opencode_cli_spawn::start_cli_harness_session`), which
//! renders `cursor.yaml`.

pub use crate::chat;
pub use chat::*;
